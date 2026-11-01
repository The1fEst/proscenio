mod state;

use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::config::{self, BLUETOOTH_MEDIA_BUTTONS};
use crate::core::watch;
use crate::services::media::mpris::{Mpris, Track};
use crate::services::network::bluez::{BUS, Bluez};
use state::{Action, Properties, build_dict, list_changes, list_properties, route};

const INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const PATH: &str = "/dev/fEst/Proscenio/BluezPlayer";
const MEDIA_INTERFACE: &str = "org.bluez.Media1";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER_INTERFACE: &str = "org.freedesktop.DBus.ObjectManager";
const ACCESS_DENIED_ERROR_NAME: &str = "org.freedesktop.DBus.Error.AccessDenied";
const NOT_SUPPORTED_ERROR_NAME: &str = "org.freedesktop.DBus.Error.NotSupported";
const ALREADY_EXISTS_ERROR_NAME: &str = "org.bluez.Error.AlreadyExists";
const REGISTER_TIMEOUT_MILLIS: i32 = 5000;
const REGISTER_RETRY_INTERVAL: Duration = Duration::from_secs(30);
const REGISTER_RETRY_INTERVAL_MAX: Duration = Duration::from_secs(1800);

const INTROSPECTION: &str = r#"<node>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Play"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <method name="Next"/>
    <method name="Previous"/>
  </interface>
</node>"#;

#[derive(Debug, Default, PartialEq)]
struct RegistrationPlan {
    adapters_to_register: Vec<String>,
    adapters_to_unregister: Vec<String>,
}

fn is_from_owner(sender: Option<&str>, owner: Option<&str>) -> bool {
    sender.is_some() && sender == owner
}

fn read_removed_media_adapter(parameters: &Variant) -> Option<String> {
    let path = parameters
        .try_child_value(0)?
        .get::<glib::variant::ObjectPath>()?;
    let interfaces = parameters.try_child_value(1)?.get::<Vec<String>>()?;
    interfaces
        .iter()
        .any(|interface| interface == MEDIA_INTERFACE)
        .then(|| path.as_str().to_owned())
}

fn is_already_registered(error: &glib::Error) -> bool {
    gio::DBusError::remote_error(error).as_deref() == Some(ALREADY_EXISTS_ERROR_NAME)
}

fn plan_registrations(
    registrations: &[String],
    refusals: &[String],
    adapters: &[String],
) -> RegistrationPlan {
    RegistrationPlan {
        adapters_to_register: adapters
            .iter()
            .filter(|adapter| !registrations.contains(adapter) && !refusals.contains(adapter))
            .cloned()
            .collect(),
        adapters_to_unregister: registrations
            .iter()
            .filter(|adapter| !adapters.contains(adapter))
            .cloned()
            .collect(),
    }
}

pub struct BluezPlayer {
    system: gio::DBusConnection,
    bluez: Bluez,
    mpris: Mpris,
    owner: RefCell<Option<String>>,
    registered_adapters: RefCell<Vec<String>>,
    refused_adapters: RefCell<Vec<String>>,
    removal_counts: RefCell<HashMap<String, u64>>,
    published_properties: RefCell<Properties>,
    is_syncing: Cell<bool>,
    is_sync_queued: Cell<bool>,
    retry_source: RefCell<Option<glib::SourceId>>,
    retry_interval: Cell<Duration>,
    _config_watch: watch::Watch,
}

impl BluezPlayer {
    pub fn new(
        system: Option<gio::DBusConnection>,
        bluez: &Bluez,
        mpris: &Mpris,
    ) -> Option<Rc<Self>> {
        let system = system?;
        let player = Rc::new_cyclic(|player: &Weak<Self>| BluezPlayer {
            system,
            bluez: bluez.clone(),
            mpris: mpris.clone(),
            owner: RefCell::new(None),
            registered_adapters: RefCell::default(),
            refused_adapters: RefCell::default(),
            removal_counts: RefCell::default(),
            published_properties: RefCell::default(),
            is_syncing: Cell::new(false),
            is_sync_queued: Cell::new(false),
            retry_source: RefCell::new(None),
            retry_interval: Cell::new(REGISTER_RETRY_INTERVAL),
            _config_watch: watch::config(BLUETOOTH_MEDIA_BUTTONS, {
                let player = player.clone();
                move || {
                    if let Some(player) = player.upgrade() {
                        player.publish();
                        player.queue_sync();
                    }
                }
            }),
        });
        if !player.export() {
            eprintln!("bluezplayer: {PATH} could not be exported");
            return None;
        }
        player.publish();
        player.follow_owner();
        player.follow_removals();
        bluez
            .subscribe({
                let player = Rc::downgrade(&player);
                move || {
                    if let Some(player) = player.upgrade() {
                        player.queue_sync();
                    }
                }
            })
            .detach();
        mpris
            .subscribe({
                let player = Rc::downgrade(&player);
                move || {
                    if let Some(player) = player.upgrade() {
                        player.publish();
                    }
                }
            })
            .detach();
        Some(player)
    }

    fn export(self: &Rc<Self>) -> bool {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return false;
        };
        let Some(interface) = node.lookup_interface(INTERFACE) else {
            return false;
        };
        let player = Rc::downgrade(self);
        self.system
            .register_object(PATH, &interface)
            .method_call(move |_, sender, _, _, method, _, invocation| {
                let Some(player) = player.upgrade() else {
                    invocation.return_dbus_error(NOT_SUPPORTED_ERROR_NAME, "The player is gone");
                    return;
                };
                if !is_from_owner(sender, player.owner.borrow().as_deref()) {
                    invocation.return_dbus_error(ACCESS_DENIED_ERROR_NAME, "Not from BlueZ");
                    return;
                }
                player.handle(method, invocation);
            })
            .build()
            .is_ok()
    }

    fn handle(&self, method: &str, invocation: gio::DBusMethodInvocation) {
        let target = self
            .active_track()
            .and_then(|track| Some((route(method, &track)?, track)));
        let Some((action, track)) = target else {
            invocation.return_dbus_error(NOT_SUPPORTED_ERROR_NAME, "NotSupported");
            return;
        };
        self.perform(action, &track);
        invocation.return_value(None);
    }

    fn perform(&self, action: Action, track: &Track) {
        match action {
            Action::Play => self.mpris.command(&track.bus, "Play"),
            Action::Pause => self.mpris.command(&track.bus, "Pause"),
            Action::Stop => self.mpris.command(&track.bus, "Stop"),
            Action::PlayPause => self.mpris.toggle_playing(track),
            Action::Next => self.mpris.skip_or_end(track),
            Action::Previous => self.mpris.previous(track),
        }
    }

    fn active_track(&self) -> Option<Track> {
        if !config::current().has_bluetooth_media_buttons {
            return None;
        }
        self.mpris.active_track()
    }

    fn current_properties(&self) -> Properties {
        let track = self.active_track();
        let length = track
            .as_ref()
            .map_or(0, |track| self.mpris.track_length(track));
        list_properties(track.as_ref(), length)
    }

    fn publish(&self) {
        let properties = self.current_properties();
        let changes = list_changes(&self.published_properties.borrow(), &properties);
        self.published_properties.replace(properties);
        if self.registered_adapters.borrow().is_empty() {
            return;
        }
        let owner = self.owner.borrow().clone();
        if let Some(owner) = owner {
            self.emit(&owner, &changes);
        }
    }

    fn emit(&self, owner: &str, changes: &[(&'static str, Variant)]) {
        if changes.is_empty() {
            return;
        }
        let parameters = Variant::tuple_from_iter([
            INTERFACE.to_variant(),
            build_dict(changes),
            Vec::<String>::new().to_variant(),
        ]);
        if let Err(error) = self.system.emit_signal(
            Some(owner),
            PATH,
            PROPERTIES_INTERFACE,
            "PropertiesChanged",
            Some(&parameters),
        ) {
            eprintln!("bluezplayer: PropertiesChanged failed: {error}");
        }
    }

    fn queue_sync(self: &Rc<Self>) {
        self.is_sync_queued.set(true);
        if self.is_syncing.replace(true) {
            return;
        }
        let player = self.clone();
        glib::spawn_future_local(async move {
            while player.is_sync_queued.replace(false) {
                player.sync().await;
            }
            player.is_syncing.set(false);
            if player.refused_adapters.borrow().is_empty() {
                player.cancel_retry();
            } else {
                player.schedule_retry();
            }
        });
    }

    async fn sync(&self) {
        let Some(owner) = self.owner.borrow().clone() else {
            return;
        };
        let Ok(path) = glib::variant::ObjectPath::try_from(PATH.to_owned()) else {
            return;
        };
        let adapters = if config::current().has_bluetooth_media_buttons {
            self.bluez.media_adapters.borrow().clone()
        } else {
            Vec::new()
        };
        self.refused_adapters
            .borrow_mut()
            .retain(|adapter| adapters.contains(adapter));
        let plan = plan_registrations(
            &self.registered_adapters.borrow(),
            &self.refused_adapters.borrow(),
            &adapters,
        );
        for adapter in plan.adapters_to_unregister {
            let result = self
                .call_media(&adapter, "UnregisterPlayer", (path.clone(),).to_variant())
                .await;
            if !self.is_owned_by(&owner) {
                return;
            }
            self.registered_adapters
                .borrow_mut()
                .retain(|path| *path != adapter);
            if let Err(error) = result {
                eprintln!("bluezplayer: UnregisterPlayer on {adapter} failed: {error}");
            }
        }
        for adapter in plan.adapters_to_register {
            let properties = self.published_properties.borrow().clone();
            let parameters = Variant::tuple_from_iter([path.to_variant(), build_dict(&properties)]);
            let removal_count = self.count_removals(&adapter);
            let result = self
                .call_media(&adapter, "RegisterPlayer", parameters)
                .await;
            if !self.is_owned_by(&owner) {
                return;
            }
            if self.count_removals(&adapter) != removal_count {
                continue;
            }
            let baseline = match result {
                Ok(_) => properties,
                Err(error) if is_already_registered(&error) => Properties::new(),
                Err(error) => {
                    eprintln!("bluezplayer: RegisterPlayer on {adapter} failed: {error}");
                    self.refused_adapters.borrow_mut().push(adapter);
                    continue;
                }
            };
            self.registered_adapters.borrow_mut().push(adapter);
            let changes = list_changes(&baseline, &self.published_properties.borrow());
            self.emit(&owner, &changes);
        }
    }

    fn schedule_retry(self: &Rc<Self>) {
        if self.retry_source.borrow().is_some() {
            return;
        }
        let interval = self.retry_interval.get();
        self.retry_interval
            .set((interval * 2).min(REGISTER_RETRY_INTERVAL_MAX));
        let player = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(interval, move || {
            let Some(player) = player.upgrade() else {
                return;
            };
            player.retry_source.take();
            player.refused_adapters.borrow_mut().clear();
            player.queue_sync();
        });
        self.retry_source.replace(Some(source));
    }

    fn cancel_retry(&self) {
        self.retry_interval.set(REGISTER_RETRY_INTERVAL);
        if let Some(source) = self.retry_source.take() {
            source.remove();
        }
    }

    async fn call_media(
        &self,
        adapter: &str,
        method: &str,
        parameters: Variant,
    ) -> Result<Variant, glib::Error> {
        self.system
            .call_future(
                Some(BUS),
                adapter,
                MEDIA_INTERFACE,
                method,
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                REGISTER_TIMEOUT_MILLIS,
            )
            .await
    }

    fn count_removals(&self, adapter: &str) -> u64 {
        self.removal_counts
            .borrow()
            .get(adapter)
            .copied()
            .unwrap_or_default()
    }

    fn is_owned_by(&self, owner: &str) -> bool {
        self.owner.borrow().as_deref() == Some(owner)
    }

    fn forget_registrations(&self) {
        self.registered_adapters.borrow_mut().clear();
        self.refused_adapters.borrow_mut().clear();
        self.cancel_retry();
    }

    fn follow_removals(self: &Rc<Self>) {
        let player = Rc::downgrade(self);
        std::mem::forget(self.system.subscribe_to_signal(
            Some(BUS),
            Some(OBJECT_MANAGER_INTERFACE),
            Some("InterfacesRemoved"),
            None,
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let Some(player) = player.upgrade() else {
                    return;
                };
                let Some(adapter) = read_removed_media_adapter(signal.parameters) else {
                    return;
                };
                for adapters in [&player.registered_adapters, &player.refused_adapters] {
                    adapters.borrow_mut().retain(|path| *path != adapter);
                }
                *player
                    .removal_counts
                    .borrow_mut()
                    .entry(adapter)
                    .or_default() += 1;
            },
        ));
    }

    fn follow_owner(self: &Rc<Self>) {
        gio::bus_watch_name_on_connection(
            &self.system,
            BUS,
            gio::BusNameWatcherFlags::NONE,
            {
                let player = Rc::downgrade(self);
                move |_, _, owner| {
                    let Some(player) = player.upgrade() else {
                        return;
                    };
                    player.owner.replace(Some(owner.to_owned()));
                    player.forget_registrations();
                    player.bluez.refresh();
                }
            },
            {
                let player = Rc::downgrade(self);
                move |_, _| {
                    let Some(player) = player.upgrade() else {
                        return;
                    };
                    player.owner.take();
                    player.forget_registrations();
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_removed_media_interface_drops_an_adapter() {
        let build_signal = |interfaces: &[&str]| {
            let path = glib::variant::ObjectPath::try_from("/org/bluez/hci0".to_owned()).unwrap();
            (path, interfaces.to_vec()).to_variant()
        };
        let cases = [
            (
                build_signal(&[MEDIA_INTERFACE, "org.bluez.Adapter1"]),
                Some("/org/bluez/hci0"),
            ),
            (build_signal(&["org.bluez.Device1"]), None),
            (
                ("/org/bluez/hci0", vec![MEDIA_INTERFACE]).to_variant(),
                None,
            ),
        ];
        for (parameters, adapter) in cases {
            assert_eq!(
                read_removed_media_adapter(&parameters).as_deref(),
                adapter,
                "{parameters}"
            );
        }
    }

    #[test]
    fn only_the_bluez_owner_is_obeyed() {
        let cases = [
            (Some(":1.5"), Some(":1.5"), true),
            (Some(":1.6"), Some(":1.5"), false),
            (Some(":1.5"), None, false),
            (None, None, false),
            (None, Some(":1.5"), false),
        ];
        for (sender, owner, is_obeyed) in cases {
            assert_eq!(
                is_from_owner(sender, owner),
                is_obeyed,
                "{sender:?} {owner:?}"
            );
        }
    }

    #[test]
    fn a_player_bluez_already_has_counts_as_registered() {
        let cases = [
            (ALREADY_EXISTS_ERROR_NAME, true),
            ("org.bluez.Error.InvalidArguments", false),
            ("org.freedesktop.DBus.Error.NoReply", false),
        ];
        for (name, is_registered) in cases {
            let error = gio::DBusError::new_for_dbus_error(name, "");
            assert_eq!(is_already_registered(&error), is_registered, "{name}");
        }
    }

    #[test]
    fn registrations_follow_the_adapters_and_skip_the_refused_ones() {
        let to_paths = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .map(|name| format!("/org/bluez/{name}"))
                .collect()
        };
        let cases = [
            (
                ["hci0", "hci3"].as_slice(),
                ["hci2"].as_slice(),
                ["hci1", "hci2", "hci3"].as_slice(),
                RegistrationPlan {
                    adapters_to_register: to_paths(&["hci1"]),
                    adapters_to_unregister: to_paths(&["hci0"]),
                },
            ),
            (&["hci0"], &[], &["hci0"], RegistrationPlan::default()),
            (
                &["hci0", "hci1"],
                &[],
                &[],
                RegistrationPlan {
                    adapters_to_register: Vec::new(),
                    adapters_to_unregister: to_paths(&["hci0", "hci1"]),
                },
            ),
        ];
        for (registrations, refusals, adapters, plan) in cases {
            assert_eq!(
                plan_registrations(
                    &to_paths(registrations),
                    &to_paths(refusals),
                    &to_paths(adapters)
                ),
                plan,
                "{registrations:?} {refusals:?} {adapters:?}"
            );
        }
    }
}

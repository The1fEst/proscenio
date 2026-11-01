pub mod calls;
pub mod model;
mod store;
mod throttle;

use gtk4::gio;
use gtk4::glib::{self, Variant};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::dbus;
use crate::platform::network::nmprofile::Profile;
use model::{BUS, Objects, SETTINGS_CONNECTION, SettingsConnection, Snapshot, WIRELESS_DEVICE};
use throttle::Throttle;

const OBJECT_MANAGER_PATH: &str = "/org/freedesktop";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const SETTLE_DELAY: Duration = Duration::from_millis(50);
const FLUSH_INTERVAL: Duration = Duration::from_millis(250);
const RETRY_DELAY: Duration = Duration::from_secs(2);

thread_local! {
    static INSTANCE: Rc<NetworkManager> = NetworkManager::new();
}

pub struct NetworkManager {
    system: Option<gio::DBusConnection>,
    objects: RefCell<Objects>,
    settings_connections: RefCell<HashMap<String, SettingsConnection>>,
    pending_paths: RefCell<HashSet<String>>,
    missing_paths: RefCell<Vec<String>>,
    is_resyncing: Cell<bool>,
    snapshot: RefCell<Rc<Snapshot>>,
    generation: Cell<u64>,
    throttle: RefCell<Throttle>,
    listeners: Listeners,
    signal_subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl NetworkManager {
    pub fn instance() -> Rc<Self> {
        INSTANCE.with(Rc::clone)
    }

    fn new() -> Rc<Self> {
        let system = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok();
        let manager = Rc::new(NetworkManager {
            system: system.clone(),
            objects: RefCell::default(),
            settings_connections: RefCell::default(),
            pending_paths: RefCell::default(),
            missing_paths: RefCell::default(),
            is_resyncing: Cell::new(false),
            snapshot: RefCell::default(),
            generation: Cell::new(0),
            throttle: RefCell::new(Throttle::new(SETTLE_DELAY, FLUSH_INTERVAL)),
            listeners: Listeners::default(),
            signal_subscription: RefCell::new(None),
        });
        let Some(system) = system else {
            return manager;
        };
        let weak = Rc::downgrade(&manager);
        manager
            .signal_subscription
            .replace(Some(system.subscribe_to_signal(
                Some(BUS),
                None,
                None,
                None,
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some(manager) = weak.upgrade() {
                        manager.receive(
                            signal.object_path,
                            signal.interface_name,
                            signal.signal_name,
                            signal.parameters,
                        );
                    }
                },
            )));
        let weak_manager = Rc::downgrade(&manager);
        gio::bus_watch_name_on_connection(
            &system,
            BUS,
            gio::BusNameWatcherFlags::NONE,
            {
                let weak_manager = weak_manager.clone();
                move |_, _, _| {
                    if let Some(manager) = weak_manager.upgrade() {
                        manager.generation.set(manager.generation.get() + 1);
                        manager.load(manager.generation.get());
                    }
                }
            },
            move |_, _| {
                if let Some(manager) = weak_manager.upgrade() {
                    manager.forget();
                }
            },
        );
        manager
    }

    pub fn snapshot(&self) -> Rc<Snapshot> {
        self.snapshot.borrow().clone()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    fn load(self: &Rc<Self>, generation: u64) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let reply = dbus::managed_objects(&system, BUS, OBJECT_MANAGER_PATH).await;
            let Some(manager) = weak.upgrade() else {
                return;
            };
            if manager.generation.get() != generation {
                return;
            }
            let Some(reply) = reply else {
                manager.retry_load(generation);
                return;
            };
            let objects = store::read_managed_objects(&reply.child_value(0));
            let settings_paths: HashSet<String> = objects
                .iter()
                .filter(|(_, interfaces)| interfaces.contains_key(SETTINGS_CONNECTION))
                .map(|(path, _)| path.clone())
                .collect();
            manager.objects.replace(objects);
            manager.pending_paths.replace(settings_paths.clone());
            for path in settings_paths {
                manager.fetch_settings(path);
            }
            manager.schedule_flush();
        });
    }

    fn retry_load(self: &Rc<Self>, generation: u64) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(RETRY_DELAY, move || {
            if let Some(manager) = weak.upgrade()
                && manager.generation.get() == generation
            {
                manager.load(generation);
            }
        });
    }

    fn forget(self: &Rc<Self>) {
        self.generation.set(self.generation.get() + 1);
        self.objects.borrow_mut().clear();
        self.settings_connections.borrow_mut().clear();
        self.pending_paths.borrow_mut().clear();
        self.missing_paths.borrow_mut().clear();
        self.schedule_flush();
    }

    fn fetch_settings(self: &Rc<Self>, path: String) {
        let generation = self.generation.get();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let connection = read_settings_connection(&path).await;
            let Some(manager) = weak.upgrade() else {
                return;
            };
            if manager.generation.get() != generation {
                return;
            }
            let is_known = manager
                .objects
                .borrow()
                .get(&path)
                .is_some_and(|interfaces| interfaces.contains_key(SETTINGS_CONNECTION));
            manager.pending_paths.borrow_mut().remove(&path);
            if let (true, Some(connection)) = (is_known, connection) {
                manager
                    .settings_connections
                    .borrow_mut()
                    .insert(path, connection);
            }
            manager.schedule_flush();
        });
    }

    fn receive(self: &Rc<Self>, path: &str, interface: &str, member: &str, parameters: &Variant) {
        let mut objects = self.objects.borrow_mut();
        let membership_change = match (interface, member) {
            (PROPERTIES, "PropertiesChanged") => {
                let has_changed = store::apply_properties_changed(&mut objects, path, parameters);
                drop(objects);
                if has_changed {
                    self.schedule_flush();
                }
                return;
            }
            (OBJECT_MANAGER, "InterfacesAdded") => {
                store::apply_interfaces_added(&mut objects, parameters)
            }
            (OBJECT_MANAGER, "InterfacesRemoved") => {
                store::apply_interfaces_removed(&mut objects, parameters)
            }
            (SETTINGS_CONNECTION, "Updated") => {
                drop(objects);
                self.fetch_settings(path.to_owned());
                return;
            }
            (WIRELESS_DEVICE, "AccessPointAdded" | "AccessPointRemoved") => {
                let has_changed = if member == "AccessPointAdded" {
                    store::apply_access_point_added(&mut objects, path, parameters)
                } else {
                    store::apply_access_point_removed(&mut objects, path, parameters)
                };
                drop(objects);
                if has_changed {
                    self.schedule_flush();
                }
                return;
            }
            _ => None,
        };
        let Some(changed_path) = membership_change else {
            return;
        };
        let has_settings = objects
            .get(&changed_path)
            .is_some_and(|interfaces| interfaces.contains_key(SETTINGS_CONNECTION));
        drop(objects);
        let is_fetched = self
            .settings_connections
            .borrow()
            .contains_key(&changed_path);
        if has_settings && !is_fetched {
            self.fetch_settings(changed_path);
        } else if !has_settings && is_fetched {
            self.settings_connections.borrow_mut().remove(&changed_path);
        }
        self.schedule_flush();
    }

    fn schedule_flush(self: &Rc<Self>) {
        if !self.pending_paths.borrow().is_empty() {
            return;
        }
        let Some(delay) = self.throttle.borrow_mut().request(Instant::now()) else {
            return;
        };
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(delay, move || {
            if let Some(manager) = weak.upgrade() {
                manager.flush();
            }
        });
    }

    fn try_resync(self: &Rc<Self>) -> bool {
        let Some(system) = self.system.clone() else {
            return false;
        };
        if self.is_resyncing.replace(true) {
            return false;
        }
        let generation = self.generation.get();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let reply = dbus::managed_objects(&system, BUS, OBJECT_MANAGER_PATH).await;
            let Some(manager) = weak.upgrade() else {
                return;
            };
            manager.is_resyncing.set(false);
            let Some(reply) = reply.filter(|_| manager.generation.get() == generation) else {
                return;
            };
            let objects = store::read_managed_objects(&reply.child_value(0));
            manager
                .settings_connections
                .borrow_mut()
                .retain(|path, _| objects.contains_key(path));
            let new_settings_paths: Vec<String> = objects
                .iter()
                .filter(|(path, interfaces)| {
                    interfaces.contains_key(SETTINGS_CONNECTION)
                        && !manager.settings_connections.borrow().contains_key(*path)
                })
                .map(|(path, _)| path.clone())
                .collect();
            manager.objects.replace(objects);
            for path in new_settings_paths {
                manager.fetch_settings(path);
            }
            manager.schedule_flush();
        });
        true
    }

    fn flush(self: &Rc<Self>) {
        self.throttle.borrow_mut().finish(Instant::now());
        if !self.pending_paths.borrow().is_empty() {
            return;
        }
        let missing_paths = store::list_missing_objects(&self.objects.borrow());
        let is_new_miss = *self.missing_paths.borrow() != missing_paths;
        if missing_paths.is_empty() || (is_new_miss && self.try_resync()) {
            self.missing_paths.replace(missing_paths);
        }
        let snapshot = Snapshot::new(&self.objects.borrow(), &self.settings_connections.borrow());
        if *self.snapshot.borrow().as_ref() == snapshot {
            return;
        }
        self.snapshot.replace(Rc::new(snapshot));
        self.listeners.notify();
    }
}

async fn read_settings_connection(path: &str) -> Option<SettingsConnection> {
    let reply = calls::call_raw(
        path,
        SETTINGS_CONNECTION,
        "GetSettings",
        None,
        "(a{sa{sv}})",
        false,
    )
    .await
    .ok()?;
    let profile = Profile::from_variant(&reply.child_value(0))?;
    Some(SettingsConnection::from_profile(path, &profile))
}

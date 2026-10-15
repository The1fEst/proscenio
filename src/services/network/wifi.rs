use gtk4::gio;
use gtk4::glib::{self, Variant, variant::ObjectPath};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use crate::core::i18n::tr;
use crate::platform::dbus;
use crate::services::network::net::{BUS, nmcli, split_escaped};
use crate::services::network::nmsettings;

const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const WIFI_DEVICE: u32 = 2;
const ACTIVATED: u32 = 2;
const DEACTIVATED: u32 = 4;
const SETTLE_DELAY: Duration = Duration::from_millis(200);
const ACTIVATION_POLL: Duration = Duration::from_millis(500);
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(90);
const CALL_TIMEOUT_MILLIS: i32 = 30_000;
const SECRETS_REQUIRED: &str = "Secrets were required";
const WIRELESS: &str = "802-11-wireless";

#[derive(Clone, Debug, PartialEq)]
pub struct AccessPoint {
    pub ssid: String,
    pub bssid: String,
    pub strength: i32,
    pub frequency: i32,
    pub is_active: bool,
    pub security: String,
}

impl AccessPoint {
    pub fn is_secure(&self) -> bool {
        !self.security.is_empty()
    }

    pub fn is_enterprise(&self) -> bool {
        self.security.contains("802.1X")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedNetwork {
    pub name: String,
    pub uuid: String,
    pub should_autoconnect: bool,
    pub is_active: bool,
}

#[derive(Default)]
pub struct State {
    pub is_available: bool,
    pub is_enabled: bool,
    pub is_scanning: bool,
    pub networks: Vec<AccessPoint>,
    pub saved_networks: Vec<SavedNetwork>,
    pub password_prompt_ssid: Option<String>,
    pub target_ssid: Option<String>,
    pub is_hidden_connecting: bool,
    pub hidden_status: Option<Result<(), String>>,
    is_scan_pending: bool,
    was_device_up: Option<bool>,
}

impl State {
    pub fn is_saved(&self, ssid: &str) -> bool {
        self.saved_networks.iter().any(|saved| saved.name == ssid)
    }

    pub fn saved_network(&self, ssid: &str) -> Option<&SavedNetwork> {
        let named = || {
            self.saved_networks
                .iter()
                .filter(move |saved| saved.name == ssid)
        };
        named()
            .find(|saved| saved.is_active)
            .or_else(|| named().next())
    }
}

pub struct Wifi {
    pub state: RefCell<State>,
    system: Option<gio::DBusConnection>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    is_refresh_queued: Cell<bool>,
    subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl Wifi {
    pub fn new() -> Rc<Self> {
        let system = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok();
        let wifi = Rc::new(Wifi {
            state: RefCell::new(State::default()),
            system: system.clone(),
            listeners: RefCell::new(Vec::new()),
            is_refresh_queued: Cell::new(false),
            subscription: RefCell::new(None),
        });
        if let Some(system) = &system {
            let weak = Rc::downgrade(&wifi);
            wifi.subscription
                .replace(Some(dbus::subscribe_properties_changed(
                    system,
                    BUS,
                    move || {
                        if let Some(wifi) = weak.upgrade() {
                            wifi.queue_refresh();
                        }
                    },
                )));
        }
        wifi.refresh();
        wifi
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    fn announce(&self) {
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    fn queue_refresh(self: &Rc<Self>) {
        if self.is_refresh_queued.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(SETTLE_DELAY, move || {
            if let Some(wifi) = weak.upgrade() {
                wifi.is_refresh_queued.set(false);
                wifi.refresh();
            }
        });
    }

    pub fn refresh(self: &Rc<Self>) {
        self.refresh_networks();
        self.refresh_saved();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let radio = nmcli(&["radio", "wifi"]).await.stdout;
            let devices = nmcli(&["-t", "-f", "TYPE,STATE", "device", "status"])
                .await
                .stdout;
            let Some(wifi) = weak.upgrade() else {
                return;
            };
            let radio = radio.trim();
            let is_device_up = devices
                .lines()
                .any(|line| line.starts_with("wifi:") && !line.contains("unavailable"));
            let should_rescan = {
                let mut state = wifi.state.borrow_mut();
                state.is_available = devices.lines().any(|line| line.starts_with("wifi:"));
                state.is_enabled = radio == "enabled";
                let has_come_up = state.was_device_up == Some(false) && is_device_up;
                state.was_device_up = Some(is_device_up);
                let should_rescan = state.is_scan_pending && has_come_up;
                if should_rescan {
                    state.is_scan_pending = false;
                }
                should_rescan
            };
            wifi.announce();
            if should_rescan {
                wifi.rescan();
            }
        });
    }

    fn refresh_networks(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let networks = nmcli(&[
                "-g",
                "ACTIVE,SIGNAL,FREQ,SSID,BSSID,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
            ])
            .await
            .stdout;
            if let Some(wifi) = weak.upgrade() {
                wifi.state.borrow_mut().networks = parse_networks(&networks);
                wifi.announce();
            }
        });
    }

    fn refresh_saved(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let output = nmcli(&[
                "-t",
                "-f",
                "NAME,UUID,TYPE,AUTOCONNECT,ACTIVE",
                "connection",
                "show",
            ])
            .await
            .stdout;
            if let Some(wifi) = weak.upgrade() {
                wifi.state.borrow_mut().saved_networks = parse_saved_networks(&output);
                wifi.announce();
            }
        });
    }

    pub fn set_enabled(self: &Rc<Self>, is_enabled: bool) {
        self.state.borrow_mut().is_scan_pending = is_enabled;
        self.run_then_refresh(&["radio", "wifi", if is_enabled { "on" } else { "off" }]);
    }

    pub fn rescan(self: &Rc<Self>) {
        self.state.borrow_mut().is_scanning = true;
        self.announce();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            nmcli(&["device", "wifi", "list", "--rescan", "yes"]).await;
            if let Some(wifi) = weak.upgrade() {
                wifi.state.borrow_mut().is_scanning = false;
                wifi.refresh();
            }
        });
    }

    pub fn connect(self: &Rc<Self>, point: &AccessPoint) {
        {
            let mut state = self.state.borrow_mut();
            if point.is_secure() && !state.is_saved(&point.ssid) {
                state.password_prompt_ssid = Some(point.ssid.clone());
                drop(state);
                self.announce();
                return;
            }
            state.password_prompt_ssid = None;
            state.target_ssid = Some(point.ssid.clone());
        }
        self.announce();
        let weak = Rc::downgrade(self);
        let ssid = point.ssid.clone();
        glib::spawn_future_local(async move {
            let output = nmcli(&["device", "wifi", "connect", &ssid]).await;
            let is_refused = !output.is_success || output.stderr.contains(SECRETS_REQUIRED);
            finish_connecting(&weak, &ssid, is_refused);
        });
    }

    pub fn connect_with_password(self: &Rc<Self>, point: &AccessPoint, password: &str) {
        if password.is_empty() {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.password_prompt_ssid = None;
            state.target_ssid = Some(point.ssid.clone());
        }
        self.announce();
        let weak = Rc::downgrade(self);
        let point = point.clone();
        let settings = wireless_settings(&point.ssid, &point.security, password, false);
        let old_uuid = self
            .state
            .borrow()
            .saved_network(&point.ssid)
            .map(|saved| saved.uuid.clone());
        let system = self.system.clone();
        glib::spawn_future_local(async move {
            let is_refused = match system {
                Some(system) => {
                    let has_joined = add_and_activate(&system, settings).await.is_ok();
                    if has_joined && let Some(old_uuid) = old_uuid {
                        nmcli(&["connection", "delete", "uuid", &old_uuid]).await;
                    }
                    !has_joined
                }
                None => true,
            };
            finish_connecting(&weak, &point.ssid, is_refused);
        });
    }

    pub fn cancel_password(&self) {
        self.state.borrow_mut().password_prompt_ssid = None;
        self.announce();
    }

    pub fn disconnect(self: &Rc<Self>) {
        let state = self.state.borrow();
        let Some(uuid) = state
            .saved_networks
            .iter()
            .find(|saved| saved.is_active)
            .map(|saved| saved.uuid.clone())
        else {
            return;
        };
        drop(state);
        self.run_then_refresh(&["connection", "down", "uuid", &uuid]);
    }

    pub fn forget(self: &Rc<Self>, uuid: &str) {
        self.run_then_refresh(&["connection", "delete", "uuid", uuid]);
    }

    pub fn set_autoconnect(self: &Rc<Self>, uuid: &str, should_autoconnect: bool) {
        let weak = Rc::downgrade(self);
        let uuid = uuid.to_owned();
        glib::spawn_future_local(async move {
            let _ = nmsettings::set_autoconnect(&uuid, should_autoconnect).await;
            if let Some(wifi) = weak.upgrade() {
                wifi.refresh();
            }
        });
    }

    pub fn connect_hidden(self: &Rc<Self>, ssid: &str, password: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.is_hidden_connecting = true;
            state.hidden_status = None;
        }
        self.announce();
        let weak = Rc::downgrade(self);
        let settings = wireless_settings(ssid, "", password, true);
        let old_uuid = self
            .state
            .borrow()
            .saved_network(ssid)
            .map(|saved| saved.uuid.clone());
        let reuses_profile = password.is_empty();
        let system = self.system.clone();
        glib::spawn_future_local(async move {
            let status = match (system, old_uuid) {
                (_, Some(old_uuid)) if reuses_profile => {
                    let output = nmcli(&["connection", "up", "uuid", &old_uuid]).await;
                    if output.is_success {
                        Ok(())
                    } else {
                        Err(tr("Could not connect"))
                    }
                }
                (Some(system), old_uuid) => {
                    let status = add_and_activate(&system, settings).await;
                    if status.is_ok()
                        && let Some(old_uuid) = old_uuid
                    {
                        nmcli(&["connection", "delete", "uuid", &old_uuid]).await;
                    }
                    status
                }
                (None, _) => Err(tr("Could not connect")),
            };
            if let Some(wifi) = weak.upgrade() {
                {
                    let mut state = wifi.state.borrow_mut();
                    state.is_hidden_connecting = false;
                    state.hidden_status = Some(status);
                }
                wifi.refresh();
            }
        });
    }

    fn run_then_refresh(self: &Rc<Self>, arguments: &[&str]) {
        let weak = Rc::downgrade(self);
        let arguments: Vec<String> = arguments
            .iter()
            .map(|&argument| argument.to_owned())
            .collect();
        glib::spawn_future_local(async move {
            let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
            nmcli(&arguments).await;
            if let Some(wifi) = weak.upgrade() {
                wifi.refresh();
            }
        });
    }
}

fn finish_connecting(weak: &Weak<Wifi>, ssid: &str, is_refused: bool) {
    let Some(wifi) = weak.upgrade() else {
        return;
    };
    {
        let mut state = wifi.state.borrow_mut();
        state.target_ssid = None;
        state.password_prompt_ssid = is_refused.then(|| ssid.to_owned());
    }
    wifi.refresh();
}

fn wireless_settings(ssid: &str, security: &str, password: &str, is_hidden: bool) -> Variant {
    let mut connection: HashMap<String, Variant> = HashMap::new();
    connection.insert("id".into(), ssid.to_variant());
    connection.insert("type".into(), WIRELESS.to_variant());
    let mut wireless: HashMap<String, Variant> = HashMap::new();
    wireless.insert("ssid".into(), ssid.as_bytes().to_variant());
    if is_hidden {
        wireless.insert("hidden".into(), true.to_variant());
    }
    let mut settings: HashMap<String, HashMap<String, Variant>> = HashMap::new();
    settings.insert("connection".into(), connection);
    settings.insert(WIRELESS.into(), wireless);
    if !password.is_empty() {
        let mut security_settings: HashMap<String, Variant> = HashMap::new();
        security_settings.insert("key-mgmt".into(), key_management(security).to_variant());
        security_settings.insert("psk".into(), password.to_variant());
        settings.insert("802-11-wireless-security".into(), security_settings);
    }
    settings.to_variant()
}

fn key_management(security: &str) -> &'static str {
    if security.contains("WPA3") && !security.contains("WPA2") && !security.contains("WPA1") {
        "sae"
    } else {
        "wpa-psk"
    }
}

pub async fn wireless_device(system: &gio::DBusConnection) -> Option<String> {
    let reply = system
        .call_future(
            Some(BUS),
            ROOT,
            BUS,
            "GetDevices",
            None,
            Some(glib::VariantTy::new("(ao)").unwrap()),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    let paths: Vec<ObjectPath> = reply.child_value(0).get()?;
    for path in paths {
        let kind = dbus::u32_property(system, BUS, path.as_str(), DEVICE, "DeviceType").await;
        if kind == Some(WIFI_DEVICE) {
            return Some(path.as_str().to_owned());
        }
    }
    None
}

async fn add_and_activate(system: &gio::DBusConnection, settings: Variant) -> Result<(), String> {
    let make_connect_error = || tr("Could not connect");
    let device = wireless_device(system)
        .await
        .ok_or_else(make_connect_error)?;
    let device = ObjectPath::try_from(device).map_err(|_| make_connect_error())?;
    let specific_object = ObjectPath::try_from("/".to_owned()).map_err(|_| make_connect_error())?;
    let arguments =
        Variant::tuple_from_iter([settings, device.to_variant(), specific_object.to_variant()]);
    let reply = system
        .call_future(
            Some(BUS),
            ROOT,
            BUS,
            "AddAndActivateConnection",
            Some(&arguments),
            Some(glib::VariantTy::new("(oo)").unwrap()),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MILLIS,
        )
        .await
        .map_err(|error| remote_message(&error))?;
    let has_joined = match reply.child_value(1).get::<ObjectPath>() {
        Some(active_connection) => wait_for_activation(system, active_connection.as_str()).await,
        None => false,
    };
    if has_joined {
        return Ok(());
    }
    if let Some(connection) = reply.child_value(0).get::<ObjectPath>() {
        let _ = system
            .call_future(
                Some(BUS),
                connection.as_str(),
                "org.freedesktop.NetworkManager.Settings.Connection",
                "Delete",
                None,
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MILLIS,
            )
            .await;
    }
    Err(make_connect_error())
}

async fn wait_for_activation(system: &gio::DBusConnection, active_connection: &str) -> bool {
    let start = Instant::now();
    while start.elapsed() < ACTIVATION_TIMEOUT {
        match dbus::u32_property(system, BUS, active_connection, ACTIVE, "State").await {
            Some(ACTIVATED) => return true,
            Some(DEACTIVATED) | None => return false,
            Some(_) => glib::timeout_future(ACTIVATION_POLL).await,
        }
    }
    false
}

pub fn remote_message(error: &glib::Error) -> String {
    let message = error.message();
    message
        .strip_prefix("GDBus.Error:")
        .and_then(|rest| rest.split_once(": "))
        .map_or(message, |(_, text)| text)
        .to_owned()
}

fn parse_networks(text: &str) -> Vec<AccessPoint> {
    let mut networks: Vec<AccessPoint> = Vec::new();
    for line in text.lines() {
        let fields = split_escaped(line);
        let [active, strength, frequency, ssid, bssid, security, ..] = fields.as_slice() else {
            continue;
        };
        if ssid.is_empty() {
            continue;
        }
        let point = AccessPoint {
            ssid: ssid.clone(),
            bssid: bssid.clone(),
            strength: strength.parse().unwrap_or(0),
            frequency: frequency
                .split_whitespace()
                .next()
                .and_then(|number| number.parse().ok())
                .unwrap_or(0),
            is_active: active == "yes",
            security: security.clone(),
        };
        match networks.iter_mut().find(|known| known.ssid == point.ssid) {
            None => networks.push(point),
            Some(known) => {
                let is_better = (point.is_active && !known.is_active)
                    || (!point.is_active && !known.is_active && point.strength > known.strength);
                if is_better {
                    *known = point;
                }
            }
        }
    }
    networks.sort_by_key(|point| (!point.is_active, std::cmp::Reverse(point.strength)));
    networks
}

fn parse_saved_networks(text: &str) -> Vec<SavedNetwork> {
    text.lines()
        .filter_map(|line| {
            let fields = split_escaped(line);
            let [name, uuid, kind, autoconnect, active, ..] = fields.as_slice() else {
                return None;
            };
            (kind == WIRELESS).then(|| SavedNetwork {
                name: name.clone(),
                uuid: uuid.clone(),
                should_autoconnect: autoconnect == "yes",
                is_active: active == "yes",
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn networks_keep_one_entry_per_name_and_put_the_joined_one_first() {
        let text = "no:40:2412 MHz:Cafe:AA\\:BB\\:CC\\:DD\\:EE\\:01:\n\
                    no:70:2437 MHz:Home:AA\\:BB\\:CC\\:DD\\:EE\\:02:WPA2\n\
                    yes:30:5180 MHz:Cafe:AA\\:BB\\:CC\\:DD\\:EE\\:03:\n\
                    no:90:2462 MHz:Home:AA\\:BB\\:CC\\:DD\\:EE\\:04:WPA2\n\
                    no:50:2412 MHz::AA\\:BB\\:CC\\:DD\\:EE\\:05:WPA2";
        let point =
            |ssid: &str, bssid: &str, strength, frequency, is_active, security: &str| AccessPoint {
                ssid: ssid.to_owned(),
                bssid: bssid.to_owned(),
                strength,
                frequency,
                is_active,
                security: security.to_owned(),
            };
        assert_eq!(
            parse_networks(text),
            [
                point("Cafe", "AA:BB:CC:DD:EE:03", 30, 5180, true, ""),
                point("Home", "AA:BB:CC:DD:EE:04", 90, 2462, false, "WPA2"),
            ]
        );
    }

    #[test]
    fn saved_networks_are_the_wireless_profiles() {
        let text = "Wired connection 1:u0:802-3-ethernet:yes:yes\nHome:u1:802-11-wireless:yes:no\nCafe\\: Two:u2:802-11-wireless:no:yes";
        assert_eq!(
            parse_saved_networks(text),
            [
                SavedNetwork {
                    name: "Home".to_owned(),
                    uuid: "u1".to_owned(),
                    should_autoconnect: true,
                    is_active: false
                },
                SavedNetwork {
                    name: "Cafe: Two".to_owned(),
                    uuid: "u2".to_owned(),
                    should_autoconnect: false,
                    is_active: true
                },
            ]
        );
    }

    #[test]
    fn a_network_named_twice_is_meant_by_its_active_profile_first() {
        let saved_network = |uuid: &str, is_active| SavedNetwork {
            name: "Cafe".to_owned(),
            uuid: uuid.to_owned(),
            should_autoconnect: true,
            is_active,
        };
        let state = State {
            saved_networks: vec![saved_network("old", false), saved_network("current", true)],
            ..State::default()
        };
        assert_eq!(
            state.saved_network("Cafe").map(|saved| saved.uuid.as_str()),
            Some("current")
        );
        assert_eq!(state.saved_network("Home"), None);
    }

    #[test]
    fn key_management_is_sae_only_for_networks_that_speak_nothing_older() {
        assert_eq!(key_management("WPA2"), "wpa-psk");
        assert_eq!(key_management("WPA2 WPA3"), "wpa-psk");
        assert_eq!(key_management("WPA3"), "sae");
    }
}

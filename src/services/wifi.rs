use gtk4::gio;
use gtk4::glib::{self, Variant, variant::ObjectPath};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use crate::core::i18n::tr;
use crate::platform::dbus;
use crate::services::net::{BUS, nmcli, split_escaped};

const ROOT: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const WIFI_DEVICE: u32 = 2;
const ACTIVATED: u32 = 2;
const DEACTIVATED: u32 = 4;
const SETTLE: Duration = Duration::from_millis(200);
const ACTIVATION_POLL: Duration = Duration::from_millis(500);
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(90);
const CALL_TIMEOUT: i32 = 30_000;
const SECRETS_REQUIRED: &str = "Secrets were required";
const WIRELESS: &str = "802-11-wireless";

#[derive(Clone, Debug, PartialEq)]
pub struct AccessPoint {
    pub ssid: String,
    pub bssid: String,
    pub strength: i32,
    pub frequency: i32,
    pub active: bool,
    pub security: String,
}

impl AccessPoint {
    pub fn secure(&self) -> bool {
        !self.security.is_empty()
    }

    pub fn enterprise(&self) -> bool {
        self.security.contains("802.1X")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    pub name: String,
    pub uuid: String,
    pub automatic: bool,
    pub active: bool,
}

#[derive(Default)]
pub struct State {
    pub available: bool,
    pub enabled: bool,
    pub scanning: bool,
    pub networks: Vec<AccessPoint>,
    pub saved: Vec<Saved>,
    pub asking: Option<String>,
    pub target: Option<String>,
    pub hidden_connecting: bool,
    pub hidden_status: Option<Result<(), String>>,
    scan_pending: bool,
    device_up: Option<bool>,
}

impl State {
    pub fn is_saved(&self, ssid: &str) -> bool {
        self.saved.iter().any(|saved| saved.name == ssid)
    }

    pub fn profile(&self, ssid: &str) -> Option<&Saved> {
        let named = || self.saved.iter().filter(move |saved| saved.name == ssid);
        named()
            .find(|saved| saved.active)
            .or_else(|| named().next())
    }
}

pub struct Wifi {
    pub state: RefCell<State>,
    system: Option<gio::DBusConnection>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    refresh_queued: Cell<bool>,
    subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl Wifi {
    pub fn new() -> Rc<Self> {
        let system = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok();
        let wifi = Rc::new(Wifi {
            state: RefCell::new(State::default()),
            system: system.clone(),
            listeners: RefCell::new(Vec::new()),
            refresh_queued: Cell::new(false),
            subscription: RefCell::new(None),
        });
        if let Some(system) = &system {
            let weak = Rc::downgrade(&wifi);
            wifi.subscription
                .replace(Some(dbus::on_properties_changed(system, BUS, move || {
                    if let Some(wifi) = weak.upgrade() {
                        wifi.queue_refresh();
                    }
                })));
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
        if self.refresh_queued.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(SETTLE, move || {
            if let Some(wifi) = weak.upgrade() {
                wifi.refresh_queued.set(false);
                wifi.refresh();
            }
        });
    }

    pub fn refresh(self: &Rc<Self>) {
        self.refresh_networks();
        self.refresh_saved();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let radio = nmcli(&["radio", "wifi"]).await.output;
            let devices = nmcli(&["-t", "-f", "TYPE,STATE", "device", "status"])
                .await
                .output;
            let Some(wifi) = weak.upgrade() else {
                return;
            };
            let radio = radio.trim();
            let device_up = devices
                .lines()
                .any(|line| line.starts_with("wifi:") && !line.contains("unavailable"));
            let rescan = {
                let mut state = wifi.state.borrow_mut();
                state.available = devices.lines().any(|line| line.starts_with("wifi:"));
                state.enabled = radio == "enabled";
                let came_up = state.device_up == Some(false) && device_up;
                state.device_up = Some(device_up);
                let rescan = state.scan_pending && came_up;
                if rescan {
                    state.scan_pending = false;
                }
                rescan
            };
            wifi.announce();
            if rescan {
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
            .output;
            if let Some(wifi) = weak.upgrade() {
                wifi.state.borrow_mut().networks = parse_networks(&networks);
                wifi.announce();
            }
        });
    }

    fn refresh_saved(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let saved = nmcli(&[
                "-t",
                "-f",
                "NAME,UUID,TYPE,AUTOCONNECT,ACTIVE",
                "connection",
                "show",
            ])
            .await
            .output;
            if let Some(wifi) = weak.upgrade() {
                wifi.state.borrow_mut().saved = parse_saved(&saved);
                wifi.announce();
            }
        });
    }

    pub fn enable(self: &Rc<Self>, on: bool) {
        self.state.borrow_mut().scan_pending = on;
        self.after(&["radio", "wifi", if on { "on" } else { "off" }]);
    }

    pub fn rescan(self: &Rc<Self>) {
        self.state.borrow_mut().scanning = true;
        self.announce();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            nmcli(&["device", "wifi", "list", "--rescan", "yes"]).await;
            if let Some(wifi) = weak.upgrade() {
                wifi.state.borrow_mut().scanning = false;
                wifi.refresh();
            }
        });
    }

    pub fn connect(self: &Rc<Self>, point: &AccessPoint) {
        {
            let mut state = self.state.borrow_mut();
            if point.secure() && !state.is_saved(&point.ssid) {
                state.asking = Some(point.ssid.clone());
                drop(state);
                self.announce();
                return;
            }
            state.asking = None;
            state.target = Some(point.ssid.clone());
        }
        self.announce();
        let weak = Rc::downgrade(self);
        let ssid = point.ssid.clone();
        glib::spawn_future_local(async move {
            let result = nmcli(&["device", "wifi", "connect", &ssid]).await;
            let refused = !result.success || result.errors.contains(SECRETS_REQUIRED);
            finish_connecting(&weak, &ssid, refused);
        });
    }

    pub fn connect_with_password(self: &Rc<Self>, point: &AccessPoint, password: &str) {
        if password.is_empty() {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.asking = None;
            state.target = Some(point.ssid.clone());
        }
        self.announce();
        let weak = Rc::downgrade(self);
        let (point, password) = (point.clone(), password.to_owned());
        let old = self
            .state
            .borrow()
            .profile(&point.ssid)
            .map(|saved| saved.uuid.clone());
        let system = self.system.clone();
        glib::spawn_future_local(async move {
            let settings = wireless_settings(&point.ssid, &point.security, &password, false);
            let refused = match system {
                Some(system) => {
                    let joined = add_and_activate(&system, settings).await.is_ok();
                    if joined && let Some(old) = old {
                        nmcli(&["connection", "delete", "uuid", &old]).await;
                    }
                    !joined
                }
                None => true,
            };
            finish_connecting(&weak, &point.ssid, refused);
        });
    }

    pub fn cancel_password(&self) {
        self.state.borrow_mut().asking = None;
        self.announce();
    }

    pub fn disconnect(self: &Rc<Self>) {
        let state = self.state.borrow();
        let Some(uuid) = state
            .saved
            .iter()
            .find(|saved| saved.active)
            .map(|saved| saved.uuid.clone())
        else {
            return;
        };
        drop(state);
        self.after(&["connection", "down", "uuid", &uuid]);
    }

    pub fn forget(self: &Rc<Self>, uuid: &str) {
        self.after(&["connection", "delete", "uuid", uuid]);
    }

    pub fn set_automatic(self: &Rc<Self>, uuid: &str, automatic: bool) {
        self.after(&[
            "connection",
            "modify",
            "uuid",
            uuid,
            "connection.autoconnect",
            if automatic { "yes" } else { "no" },
        ]);
    }

    pub fn connect_hidden(self: &Rc<Self>, ssid: &str, password: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.hidden_connecting = true;
            state.hidden_status = None;
        }
        self.announce();
        let weak = Rc::downgrade(self);
        let settings = wireless_settings(ssid, "", password, true);
        let old = self
            .state
            .borrow()
            .profile(ssid)
            .map(|saved| saved.uuid.clone());
        let reuse = password.is_empty();
        let system = self.system.clone();
        glib::spawn_future_local(async move {
            let status = match (system, old) {
                (_, Some(old)) if reuse => {
                    let result = nmcli(&["connection", "up", "uuid", &old]).await;
                    if result.success {
                        Ok(())
                    } else {
                        Err(tr("Could not connect"))
                    }
                }
                (Some(system), old) => {
                    let status = add_and_activate(&system, settings).await;
                    if status.is_ok()
                        && let Some(old) = old
                    {
                        nmcli(&["connection", "delete", "uuid", &old]).await;
                    }
                    status
                }
                (None, _) => Err(tr("Could not connect")),
            };
            if let Some(wifi) = weak.upgrade() {
                {
                    let mut state = wifi.state.borrow_mut();
                    state.hidden_connecting = false;
                    state.hidden_status = Some(status);
                }
                wifi.refresh();
            }
        });
    }

    fn after(self: &Rc<Self>, arguments: &[&str]) {
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

fn finish_connecting(weak: &Weak<Wifi>, ssid: &str, refused: bool) {
    let Some(wifi) = weak.upgrade() else {
        return;
    };
    {
        let mut state = wifi.state.borrow_mut();
        state.target = None;
        state.asking = refused.then(|| ssid.to_owned());
    }
    wifi.refresh();
}

fn wireless_settings(ssid: &str, security: &str, password: &str, hidden: bool) -> Variant {
    let mut connection: HashMap<String, Variant> = HashMap::new();
    connection.insert("id".into(), ssid.to_variant());
    connection.insert("type".into(), WIRELESS.to_variant());
    let mut wireless: HashMap<String, Variant> = HashMap::new();
    wireless.insert("ssid".into(), ssid.as_bytes().to_variant());
    if hidden {
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

async fn wireless_device(system: &gio::DBusConnection) -> Option<String> {
    let reply = system
        .call_future(
            Some(BUS),
            ROOT,
            BUS,
            "GetDevices",
            None,
            Some(glib::VariantTy::new("(ao)").unwrap()),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT,
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
    let failed = || tr("Could not connect");
    let device = wireless_device(system).await.ok_or_else(failed)?;
    let device = ObjectPath::try_from(device).map_err(|_| failed())?;
    let anywhere = ObjectPath::try_from("/".to_owned()).map_err(|_| failed())?;
    let arguments =
        Variant::tuple_from_iter([settings, device.to_variant(), anywhere.to_variant()]);
    let reply = system
        .call_future(
            Some(BUS),
            ROOT,
            BUS,
            "AddAndActivateConnection",
            Some(&arguments),
            Some(glib::VariantTy::new("(oo)").unwrap()),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT,
        )
        .await
        .map_err(|error| remote_message(&error))?;
    let joined = match reply.child_value(1).get::<ObjectPath>() {
        Some(active) => activated(system, active.as_str()).await,
        None => false,
    };
    if joined {
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
                CALL_TIMEOUT,
            )
            .await;
    }
    Err(failed())
}

async fn activated(system: &gio::DBusConnection, active: &str) -> bool {
    let started = Instant::now();
    while started.elapsed() < ACTIVATION_TIMEOUT {
        match dbus::u32_property(system, BUS, active, ACTIVE, "State").await {
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
            active: active == "yes",
            security: security.clone(),
        };
        match networks.iter_mut().find(|known| known.ssid == point.ssid) {
            None => networks.push(point),
            Some(known) => {
                let better = (point.active && !known.active)
                    || (!point.active && !known.active && point.strength > known.strength);
                if better {
                    *known = point;
                }
            }
        }
    }
    networks.sort_by_key(|point| (!point.active, std::cmp::Reverse(point.strength)));
    networks
}

fn parse_saved(text: &str) -> Vec<Saved> {
    text.lines()
        .filter_map(|line| {
            let fields = split_escaped(line);
            let [name, uuid, kind, automatic, active, ..] = fields.as_slice() else {
                return None;
            };
            (kind == WIRELESS).then(|| Saved {
                name: name.clone(),
                uuid: uuid.clone(),
                automatic: automatic == "yes",
                active: active == "yes",
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
            |ssid: &str, bssid: &str, strength, frequency, active, security: &str| AccessPoint {
                ssid: ssid.to_owned(),
                bssid: bssid.to_owned(),
                strength,
                frequency,
                active,
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
            parse_saved(text),
            [
                Saved {
                    name: "Home".to_owned(),
                    uuid: "u1".to_owned(),
                    automatic: true,
                    active: false
                },
                Saved {
                    name: "Cafe: Two".to_owned(),
                    uuid: "u2".to_owned(),
                    automatic: false,
                    active: true
                },
            ]
        );
    }

    #[test]
    fn a_network_named_twice_is_meant_by_its_active_profile_first() {
        let saved = |uuid: &str, active| Saved {
            name: "Cafe".to_owned(),
            uuid: uuid.to_owned(),
            automatic: true,
            active,
        };
        let state = State {
            saved: vec![saved("old", false), saved("current", true)],
            ..State::default()
        };
        assert_eq!(
            state.profile("Cafe").map(|saved| saved.uuid.as_str()),
            Some("current")
        );
        assert_eq!(state.profile("Home"), None);
    }

    #[test]
    fn key_management_is_sae_only_for_networks_that_speak_nothing_older() {
        assert_eq!(key_management("WPA2"), "wpa-psk");
        assert_eq!(key_management("WPA2 WPA3"), "wpa-psk");
        assert_eq!(key_management("WPA3"), "sae");
    }
}

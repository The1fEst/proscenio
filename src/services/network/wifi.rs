use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::i18n::tr;
use crate::core::listeners::Subscription;
use crate::platform::network::nmprofile::WIRELESS;
use crate::services::network::networkmanager::NetworkManager;
use crate::services::network::networkmanager::calls::{self, NO_OBJECT};
use crate::services::network::networkmanager::model::{AccessPointSecurity, Snapshot};
use crate::services::network::nmsettings;

pub use crate::services::network::networkmanager::model::AccessPoint;

const SCAN_TIMEOUT: Duration = Duration::from_secs(10);

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
    scan_start: Vec<(String, i64)>,
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
    manager: Rc<NetworkManager>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    scan_count: Cell<u64>,
    subscription: RefCell<Option<Subscription>>,
}

impl Wifi {
    pub fn new() -> Rc<Self> {
        let wifi = Rc::new(Wifi {
            state: RefCell::new(State::default()),
            manager: NetworkManager::instance(),
            listeners: RefCell::new(Vec::new()),
            scan_count: Cell::new(0),
            subscription: RefCell::new(None),
        });
        let weak = Rc::downgrade(&wifi);
        wifi.subscription
            .replace(Some(wifi.manager.subscribe(move || {
                if let Some(wifi) = weak.upgrade() {
                    wifi.show();
                }
            })));
        wifi.show();
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

    fn show(self: &Rc<Self>) {
        let snapshot = self.manager.snapshot();
        let is_device_up = snapshot
            .wifi_devices()
            .any(|device| !device.is_unavailable());
        let (has_changed, should_rescan) = {
            let mut state = self.state.borrow_mut();
            let is_available = snapshot.wifi_devices().next().is_some();
            let networks = list_networks(&snapshot);
            let saved_networks = list_saved_networks(&snapshot);
            let is_scan_done = state.is_scanning && has_scanned(&snapshot, &state.scan_start);
            let has_changed = is_scan_done
                || state.is_available != is_available
                || state.is_enabled != snapshot.is_wireless_enabled
                || state.networks != networks
                || state.saved_networks != saved_networks;
            state.is_available = is_available;
            state.is_enabled = snapshot.is_wireless_enabled;
            state.networks = networks;
            state.saved_networks = saved_networks;
            if is_scan_done {
                state.is_scanning = false;
            }
            let has_come_up = state.was_device_up == Some(false) && is_device_up;
            state.was_device_up = Some(is_device_up);
            let should_rescan = state.is_scan_pending && has_come_up;
            if should_rescan {
                state.is_scan_pending = false;
            }
            (has_changed, should_rescan)
        };
        if has_changed {
            self.announce();
        }
        if should_rescan {
            self.rescan();
        }
    }

    pub fn set_enabled(self: &Rc<Self>, is_enabled: bool) {
        self.state.borrow_mut().is_scan_pending = is_enabled;
        spawn(calls::set_wireless_enabled(is_enabled));
    }

    pub fn rescan(self: &Rc<Self>) {
        let snapshot = self.manager.snapshot();
        let devices: Vec<(String, i64)> = snapshot
            .wifi_devices()
            .filter(|device| !device.is_unavailable())
            .map(|device| (device.path.clone(), device.last_scan))
            .collect();
        let scan_count = self.scan_count.get() + 1;
        self.scan_count.set(scan_count);
        {
            let mut state = self.state.borrow_mut();
            state.is_scanning = !devices.is_empty();
            state.scan_start = devices.clone();
        }
        self.announce();
        let request_count = Rc::new(Cell::new(devices.len()));
        for (device, _) in devices {
            let weak = Rc::downgrade(self);
            let request_count = request_count.clone();
            glib::spawn_future_local(async move {
                let scan_result = calls::request_scan(&device).await;
                if scan_result.is_ok() {
                    return;
                }
                request_count.set(request_count.get() - 1);
                if let (0, Some(wifi)) = (request_count.get(), weak.upgrade()) {
                    wifi.finish_scan(scan_count);
                }
            });
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(SCAN_TIMEOUT, move || {
            if let Some(wifi) = weak.upgrade() {
                wifi.finish_scan(scan_count);
            }
        });
    }

    fn finish_scan(&self, scan_count: u64) {
        if self.scan_count.get() != scan_count || !self.state.borrow().is_scanning {
            return;
        }
        self.state.borrow_mut().is_scanning = false;
        self.announce();
    }

    pub fn connect(self: &Rc<Self>, point: &AccessPoint) {
        let profile_path = {
            let mut state = self.state.borrow_mut();
            if point.security.is_secure() && !state.is_saved(&point.ssid) {
                state.password_prompt_ssid = Some(point.ssid.clone());
                drop(state);
                self.announce();
                return;
            }
            state.password_prompt_ssid = None;
            state.target_ssid = Some(point.ssid.clone());
            let snapshot = self.manager.snapshot();
            state
                .saved_network(&point.ssid)
                .and_then(|saved| snapshot.find_settings_by_uuid(&saved.uuid))
                .map(|connection| connection.path.clone())
        };
        self.announce();
        let weak = Rc::downgrade(self);
        let point = point.clone();
        glib::spawn_future_local(async move {
            let has_joined = match profile_path {
                Some(profile_path) => {
                    match calls::activate_connection(&profile_path, &point.device, &point.path)
                        .await
                    {
                        Ok(active_connection) => {
                            calls::wait_for_activation(&active_connection).await
                        }
                        Err(_) => false,
                    }
                }
                None => {
                    let settings =
                        wireless_settings(&point.ssid, &point.raw_ssid, point.security, "", false);
                    join_new_network(settings, &point.device, &point.path)
                        .await
                        .is_ok()
                }
            };
            finish_connecting(&weak, &point.ssid, !has_joined);
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
        let settings = wireless_settings(
            &point.ssid,
            &point.raw_ssid,
            point.security,
            password,
            false,
        );
        let old_uuid = self
            .state
            .borrow()
            .saved_network(&point.ssid)
            .map(|saved| saved.uuid.clone());
        glib::spawn_future_local(async move {
            let has_joined = join_new_network(settings, &point.device, NO_OBJECT)
                .await
                .is_ok();
            if has_joined && let Some(old_uuid) = old_uuid {
                let _ = nmsettings::delete(&old_uuid).await;
            }
            finish_connecting(&weak, &point.ssid, !has_joined);
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
        let snapshot = self.manager.snapshot();
        let Some(active_connection) = snapshot.find_active_by_uuid(&uuid) else {
            return;
        };
        let path = active_connection.path.clone();
        spawn(async move { calls::deactivate_connection(&path).await });
    }

    pub fn forget(self: &Rc<Self>, uuid: &str) {
        let uuid = uuid.to_owned();
        spawn(async move { nmsettings::delete(&uuid).await });
    }

    pub fn set_autoconnect(self: &Rc<Self>, uuid: &str, should_autoconnect: bool) {
        let uuid = uuid.to_owned();
        spawn(async move { nmsettings::set_autoconnect(&uuid, should_autoconnect).await });
    }

    pub fn connect_hidden(self: &Rc<Self>, ssid: &str, password: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.is_hidden_connecting = true;
            state.hidden_status = None;
        }
        self.announce();
        let weak = Rc::downgrade(self);
        let settings = wireless_settings(
            ssid,
            ssid.as_bytes(),
            AccessPointSecurity::default(),
            password,
            true,
        );
        let snapshot = self.manager.snapshot();
        let old_profile = self
            .state
            .borrow()
            .saved_network(ssid)
            .and_then(|saved| snapshot.find_settings_by_uuid(&saved.uuid))
            .map(|connection| (connection.uuid.clone(), connection.path.clone()));
        let device = snapshot
            .wifi_devices()
            .next()
            .map(|device| device.path.clone());
        let reuses_profile = password.is_empty();
        glib::spawn_future_local(async move {
            let status = match (device, old_profile) {
                (_, Some((_, profile_path))) if reuses_profile => {
                    match calls::activate_connection(&profile_path, NO_OBJECT, NO_OBJECT).await {
                        Ok(active_connection)
                            if calls::wait_for_activation(&active_connection).await =>
                        {
                            Ok(())
                        }
                        _ => Err(tr("Could not connect")),
                    }
                }
                (Some(device), old_profile) => {
                    let status = join_new_network(settings, &device, NO_OBJECT).await;
                    if status.is_ok()
                        && let Some((old_uuid, _)) = old_profile
                    {
                        let _ = nmsettings::delete(&old_uuid).await;
                    }
                    status
                }
                (None, _) => Err(tr("Could not connect")),
            };
            if let Some(wifi) = weak.upgrade() {
                let mut state = wifi.state.borrow_mut();
                state.is_hidden_connecting = false;
                state.hidden_status = Some(status);
                drop(state);
                wifi.announce();
            }
        });
    }
}

fn spawn(call: impl Future<Output = Result<(), String>> + 'static) {
    glib::spawn_future_local(async move {
        if let Err(message) = call.await {
            eprintln!("wifi: {message}");
        }
    });
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
    wifi.announce();
}

fn wireless_settings(
    name: &str,
    ssid: &[u8],
    security: AccessPointSecurity,
    password: &str,
    is_hidden: bool,
) -> Variant {
    let mut connection: HashMap<String, Variant> = HashMap::new();
    connection.insert("id".into(), name.to_variant());
    connection.insert("type".into(), WIRELESS.to_variant());
    let mut wireless: HashMap<String, Variant> = HashMap::new();
    wireless.insert("ssid".into(), ssid.to_variant());
    if is_hidden {
        wireless.insert("hidden".into(), true.to_variant());
    }
    let mut settings: HashMap<String, HashMap<String, Variant>> = HashMap::new();
    settings.insert("connection".into(), connection);
    settings.insert(WIRELESS.into(), wireless);
    if !password.is_empty() {
        let mut security_settings: HashMap<String, Variant> = HashMap::new();
        security_settings.insert("key-mgmt".into(), security.key_management().to_variant());
        security_settings.insert("psk".into(), password.to_variant());
        settings.insert("802-11-wireless-security".into(), security_settings);
    }
    settings.to_variant()
}

async fn join_new_network(
    settings: Variant,
    device: &str,
    specific_object: &str,
) -> Result<(), String> {
    let (connection, active_connection) =
        calls::add_and_activate_connection(settings, device, specific_object).await?;
    if calls::wait_for_activation(&active_connection).await {
        return Ok(());
    }
    if !connection.is_empty() {
        let _ = calls::delete_connection(&connection).await;
    }
    Err(tr("Could not connect"))
}

pub fn remote_message(error: &glib::Error) -> String {
    let message = error.message();
    message
        .strip_prefix("GDBus.Error:")
        .and_then(|rest| rest.split_once(": "))
        .map_or(message, |(_, text)| text)
        .to_owned()
}

fn has_scanned(snapshot: &Snapshot, scan_start: &[(String, i64)]) -> bool {
    snapshot.wifi_devices().any(|device| {
        scan_start
            .iter()
            .any(|(path, last_scan)| *path == device.path && *last_scan != device.last_scan)
    })
}

fn list_networks(snapshot: &Snapshot) -> Vec<AccessPoint> {
    let mut networks: Vec<AccessPoint> = Vec::new();
    for point in snapshot
        .wifi_devices()
        .flat_map(|device| device.access_points.iter())
    {
        if point.ssid.is_empty() {
            continue;
        }
        match networks.iter_mut().find(|known| known.ssid == point.ssid) {
            None => networks.push(point.clone()),
            Some(known) => {
                let is_better = (point.is_active && !known.is_active)
                    || (!point.is_active && !known.is_active && point.strength > known.strength);
                if is_better {
                    *known = point.clone();
                }
            }
        }
    }
    networks.sort_by_key(|point| (!point.is_active, std::cmp::Reverse(point.strength)));
    networks
}

fn list_saved_networks(snapshot: &Snapshot) -> Vec<SavedNetwork> {
    snapshot
        .settings_connections
        .iter()
        .filter(|connection| connection.kind == WIRELESS)
        .map(|connection| SavedNetwork {
            name: connection.id.clone(),
            uuid: connection.uuid.clone(),
            should_autoconnect: connection.should_autoconnect,
            is_active: snapshot.find_active_by_uuid(&connection.uuid).is_some(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::network::networkmanager::model::{
        ActiveConnection, Device, DeviceKind, SettingsConnection,
    };

    fn new_point(ssid: &str, number: u32, strength: u8, is_active: bool) -> AccessPoint {
        AccessPoint {
            path: format!("/org/freedesktop/NetworkManager/AccessPoint/{number}"),
            device: "/org/freedesktop/NetworkManager/Devices/3".to_owned(),
            ssid: ssid.to_owned(),
            raw_ssid: ssid.as_bytes().to_vec(),
            strength,
            frequency: 2412,
            is_active,
            security: AccessPointSecurity::default(),
        }
    }

    fn new_wifi_device(access_points: Vec<AccessPoint>, last_scan: i64) -> Device {
        Device {
            path: "/org/freedesktop/NetworkManager/Devices/3".to_owned(),
            kind: DeviceKind::Wifi,
            state: 100,
            interface: "wlan0".to_owned(),
            active_connection: None,
            access_points,
            last_scan,
        }
    }

    #[test]
    fn networks_keep_one_entry_per_name_and_put_the_joined_one_first() {
        let snapshot = Snapshot {
            devices: vec![new_wifi_device(
                vec![
                    new_point("Cafe", 1, 40, false),
                    new_point("Home", 2, 70, false),
                    new_point("Cafe", 3, 30, true),
                    new_point("Home", 4, 90, false),
                    new_point("", 5, 50, false),
                ],
                0,
            )],
            ..Snapshot::default()
        };
        assert_eq!(
            list_networks(&snapshot),
            [
                new_point("Cafe", 3, 30, true),
                new_point("Home", 4, 90, false)
            ]
        );
    }

    #[test]
    fn saved_networks_are_the_wireless_profiles() {
        let new_profile =
            |number: u32, id: &str, kind: &str, should_autoconnect| SettingsConnection {
                path: format!("/org/freedesktop/NetworkManager/Settings/{number}"),
                id: id.to_owned(),
                uuid: format!("u{number}"),
                kind: kind.to_owned(),
                should_autoconnect,
            };
        let snapshot = Snapshot {
            settings_connections: vec![
                new_profile(0, "Wired connection 1", "802-3-ethernet", true),
                new_profile(1, "Home", WIRELESS, true),
                new_profile(2, "Cafe: Two", WIRELESS, false),
            ],
            active_connections: vec![ActiveConnection {
                path: "/org/freedesktop/NetworkManager/ActiveConnection/5".to_owned(),
                id: "Cafe: Two".to_owned(),
                uuid: "u2".to_owned(),
                kind: WIRELESS.to_owned(),
            }],
            ..Snapshot::default()
        };
        assert_eq!(
            list_saved_networks(&snapshot),
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
    fn a_scan_ends_when_the_device_reports_a_newer_one() {
        let start = [("/org/freedesktop/NetworkManager/Devices/3".to_owned(), 1000)];
        let new_snapshot = |last_scan| Snapshot {
            devices: vec![new_wifi_device(Vec::new(), last_scan)],
            ..Snapshot::default()
        };
        assert!(!has_scanned(&new_snapshot(1000), &start));
        assert!(has_scanned(&new_snapshot(4000), &start));
        assert!(!has_scanned(&Snapshot::default(), &start));
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
    fn a_new_network_gets_the_settings_its_security_needs() {
        let read_settings = |settings: Variant| {
            settings
                .get::<HashMap<String, HashMap<String, Variant>>>()
                .unwrap()
        };
        let wpa2 = AccessPointSecurity {
            has_privacy: true,
            wpa_flags: 0,
            rsn_flags: 0x188,
        };
        let wpa3 = AccessPointSecurity {
            rsn_flags: 0x488,
            ..wpa2
        };
        let open_settings = read_settings(wireless_settings(
            "Caf\u{fffd}",
            b"Caf\xe9",
            AccessPointSecurity::default(),
            "",
            false,
        ));
        assert_eq!(
            open_settings,
            HashMap::from([
                (
                    "connection".to_owned(),
                    HashMap::from([
                        ("id".to_owned(), "Caf\u{fffd}".to_variant()),
                        ("type".to_owned(), WIRELESS.to_variant()),
                    ])
                ),
                (
                    WIRELESS.to_owned(),
                    HashMap::from([("ssid".to_owned(), b"Caf\xe9".to_variant())])
                ),
            ])
        );
        for (security, key_management) in [(wpa2, "wpa-psk"), (wpa3, "sae")] {
            let settings = read_settings(wireless_settings(
                "Home", b"Home", security, "secret12", false,
            ));
            assert_eq!(
                settings["802-11-wireless-security"],
                HashMap::from([
                    ("key-mgmt".to_owned(), key_management.to_variant()),
                    ("psk".to_owned(), "secret12".to_variant()),
                ])
            );
            assert!(!settings[WIRELESS].contains_key("hidden"));
        }
        let hidden_settings = read_settings(wireless_settings(
            "Attic",
            b"Attic",
            AccessPointSecurity::default(),
            "secret12",
            true,
        ));
        assert_eq!(hidden_settings[WIRELESS]["hidden"], true.to_variant());
        assert_eq!(
            hidden_settings["802-11-wireless-security"]["key-mgmt"],
            "wpa-psk".to_variant()
        );
    }
}

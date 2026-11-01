use gtk4::glib::{Variant, variant::FromVariant, variant::ObjectPath};
use std::collections::HashMap;

use crate::platform::network::nmprofile::Profile;

pub const BUS: &str = "org.freedesktop.NetworkManager";
pub const ROOT: &str = "/org/freedesktop/NetworkManager";
pub const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
pub const WIRELESS_DEVICE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
pub const ACTIVE_CONNECTION: &str = "org.freedesktop.NetworkManager.Connection.Active";
pub const SETTINGS_CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
pub const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
pub const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
pub const INTERFACES: [&str; 7] = [
    BUS,
    DEVICE,
    WIRELESS_DEVICE,
    ACCESS_POINT,
    ACTIVE_CONNECTION,
    SETTINGS,
    SETTINGS_CONNECTION,
];

const ETHERNET_CODE: u32 = 1;
const WIFI_CODE: u32 = 2;
const UNAVAILABLE_STATE: u32 = 20;
const DISCONNECTED_STATE: u32 = 30;
const PREPARE_STATE: u32 = 40;
const SECONDARIES_STATE: u32 = 90;
const ACTIVATED_STATE: u32 = 100;
const LIMITED_CONNECTIVITY: u32 = 3;
const PRIVACY_FLAG: u32 = 0x1;
const ACCESS_POINT_CAPABILITY: u32 = 0x40;
const KEY_MANAGEMENT_PSK: u32 = 0x100;
const KEY_MANAGEMENT_802_1X: u32 = 0x200;
const KEY_MANAGEMENT_SAE: u32 = 0x400;
const KEY_MANAGEMENT_FLAGS: u32 = 0x3f00;
pub const LOOPBACK: &str = "loopback";

pub type Properties = HashMap<String, Variant>;
pub type Objects = HashMap<String, HashMap<String, Properties>>;

fn read<T: FromVariant>(properties: &Properties, name: &str) -> Option<T> {
    properties.get(name)?.get()
}

fn read_string(properties: &Properties, name: &str) -> String {
    read(properties, name).unwrap_or_default()
}

fn read_path(properties: &Properties, name: &str) -> Option<String> {
    read::<ObjectPath>(properties, name)
        .map(|path| path.as_str().to_owned())
        .filter(|path| path != "/")
}

fn read_paths(properties: &Properties, name: &str) -> Vec<String> {
    read::<Vec<ObjectPath>>(properties, name)
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.as_str().to_owned())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeviceKind {
    Ethernet,
    Wifi,
    Other,
}

impl DeviceKind {
    fn from_code(code: u32) -> Self {
        match code {
            ETHERNET_CODE => DeviceKind::Ethernet,
            WIFI_CODE => DeviceKind::Wifi,
            _ => DeviceKind::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AccessPointSecurity {
    pub has_privacy: bool,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}

impl AccessPointSecurity {
    pub fn is_secure(&self) -> bool {
        let is_wep = self.has_privacy && self.wpa_flags == 0 && self.rsn_flags == 0;
        is_wep || self.wpa_flags != 0 || self.rsn_flags & KEY_MANAGEMENT_FLAGS != 0
    }

    pub fn is_enterprise(&self) -> bool {
        (self.wpa_flags | self.rsn_flags) & KEY_MANAGEMENT_802_1X != 0
    }

    pub fn key_management(&self) -> &'static str {
        let is_wpa3_only = self.rsn_flags & KEY_MANAGEMENT_SAE != 0
            && self.rsn_flags & (KEY_MANAGEMENT_PSK | KEY_MANAGEMENT_802_1X) == 0
            && self.wpa_flags == 0;
        if is_wpa3_only { "sae" } else { "wpa-psk" }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccessPoint {
    pub path: String,
    pub device: String,
    pub ssid: String,
    pub raw_ssid: Vec<u8>,
    pub strength: u8,
    pub frequency: u32,
    pub is_active: bool,
    pub security: AccessPointSecurity,
}

impl AccessPoint {
    fn from_properties(path: &str, device: &str, is_active: bool, properties: &Properties) -> Self {
        let raw_ssid: Vec<u8> = read(properties, "Ssid").unwrap_or_default();
        AccessPoint {
            path: path.to_owned(),
            device: device.to_owned(),
            ssid: String::from_utf8_lossy(&raw_ssid).into_owned(),
            raw_ssid,
            strength: read(properties, "Strength").unwrap_or(0),
            frequency: read(properties, "Frequency").unwrap_or(0),
            is_active,
            security: AccessPointSecurity {
                has_privacy: read::<u32>(properties, "Flags").unwrap_or(0) & PRIVACY_FLAG != 0,
                wpa_flags: read(properties, "WpaFlags").unwrap_or(0),
                rsn_flags: read(properties, "RsnFlags").unwrap_or(0),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub path: String,
    pub kind: DeviceKind,
    pub state: u32,
    pub interface: String,
    pub active_connection: Option<String>,
    pub access_points: Vec<AccessPoint>,
    pub last_scan: i64,
    pub wireless_capabilities: u32,
}

impl Device {
    fn from_interfaces(
        path: &str,
        interfaces: &HashMap<String, Properties>,
        objects: &Objects,
    ) -> Option<Self> {
        let properties = interfaces.get(DEVICE)?;
        if read::<bool>(properties, "Real") == Some(false) {
            return None;
        }
        let no_properties = Properties::new();
        let wireless = interfaces.get(WIRELESS_DEVICE).unwrap_or(&no_properties);
        let active_access_point = read_path(wireless, "ActiveAccessPoint");
        let access_points = read_paths(wireless, "AccessPoints")
            .into_iter()
            .filter_map(|point_path| {
                let point = objects.get(&point_path)?.get(ACCESS_POINT)?;
                let is_active = active_access_point.as_deref() == Some(point_path.as_str());
                Some(AccessPoint::from_properties(
                    &point_path,
                    path,
                    is_active,
                    point,
                ))
            })
            .collect();
        Some(Device {
            path: path.to_owned(),
            kind: DeviceKind::from_code(read(properties, "DeviceType").unwrap_or(0)),
            state: read(properties, "State").unwrap_or(0),
            interface: read_string(properties, "Interface"),
            active_connection: read_path(properties, "ActiveConnection"),
            access_points,
            last_scan: read(wireless, "LastScan").unwrap_or(0),
            wireless_capabilities: read(wireless, "WirelessCapabilities").unwrap_or(0),
        })
    }

    pub fn can_host_access_point(&self) -> bool {
        self.wireless_capabilities & ACCESS_POINT_CAPABILITY != 0
    }

    pub fn active_point(&self) -> Option<&AccessPoint> {
        self.access_points.iter().find(|point| point.is_active)
    }

    pub fn is_unavailable(&self) -> bool {
        self.state == UNAVAILABLE_STATE
    }

    pub fn is_disconnected(&self) -> bool {
        self.state == DISCONNECTED_STATE
    }

    pub fn is_connecting(&self) -> bool {
        (PREPARE_STATE..=SECONDARIES_STATE).contains(&self.state)
    }

    pub fn is_connected(&self) -> bool {
        self.state == ACTIVATED_STATE
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveConnection {
    pub path: String,
    pub id: String,
    pub uuid: String,
    pub kind: String,
    pub devices: Vec<String>,
}

impl ActiveConnection {
    fn from_properties(path: &str, properties: &Properties) -> Self {
        ActiveConnection {
            path: path.to_owned(),
            id: read_string(properties, "Id"),
            uuid: read_string(properties, "Uuid"),
            kind: read_string(properties, "Type"),
            devices: read_paths(properties, "Devices"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsConnection {
    pub path: String,
    pub id: String,
    pub uuid: String,
    pub kind: String,
    pub should_autoconnect: bool,
    pub is_port: bool,
}

impl SettingsConnection {
    pub fn from_profile(path: &str, profile: &Profile) -> Self {
        SettingsConnection {
            path: path.to_owned(),
            id: profile.id(),
            uuid: profile.uuid(),
            kind: profile.kind(),
            should_autoconnect: profile.is_autoconnect_enabled(),
            is_port: profile.is_port(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub is_wireless_enabled: bool,
    pub connectivity: u32,
    pub primary_connection: Option<String>,
    pub primary_connection_type: String,
    pub devices: Vec<Device>,
    pub active_connections: Vec<ActiveConnection>,
    pub settings_connections: Vec<SettingsConnection>,
}

impl Snapshot {
    pub fn new(
        objects: &Objects,
        settings_connections: &HashMap<String, SettingsConnection>,
    ) -> Self {
        let mut paths: Vec<&String> = objects.keys().collect();
        paths.sort_by_key(|path| parse_object_number(path));
        let devices = paths
            .iter()
            .filter_map(|path| Device::from_interfaces(path, &objects[*path], objects))
            .collect();
        let active_connections = paths
            .iter()
            .filter_map(|path| {
                let properties = objects[*path].get(ACTIVE_CONNECTION)?;
                Some(ActiveConnection::from_properties(path, properties))
            })
            .collect();
        let mut settings_connections: Vec<SettingsConnection> =
            settings_connections.values().cloned().collect();
        settings_connections.sort_by_key(|connection| parse_object_number(&connection.path));
        let root = objects.get(ROOT).and_then(|interfaces| interfaces.get(BUS));
        let read_root = |name| root.and_then(|properties| properties.get(name));
        Snapshot {
            is_wireless_enabled: read_root("WirelessEnabled")
                .and_then(Variant::get)
                .unwrap_or(false),
            connectivity: read_root("Connectivity")
                .and_then(Variant::get)
                .unwrap_or(0),
            primary_connection: root
                .and_then(|properties| read_path(properties, "PrimaryConnection")),
            primary_connection_type: read_root("PrimaryConnectionType")
                .and_then(Variant::get)
                .unwrap_or_default(),
            devices,
            active_connections,
            settings_connections,
        }
    }

    pub fn is_connectivity_limited(&self) -> bool {
        self.connectivity == LIMITED_CONNECTIVITY
    }

    pub fn wifi_devices(&self) -> impl Iterator<Item = &Device> {
        self.devices
            .iter()
            .filter(|device| device.kind == DeviceKind::Wifi)
    }

    pub fn active_connection(&self, path: &str) -> Option<&ActiveConnection> {
        self.active_connections
            .iter()
            .find(|connection| connection.path == path)
    }

    pub fn main_connection(&self) -> Option<&ActiveConnection> {
        self.primary_connection
            .as_deref()
            .and_then(|path| self.active_connection(path))
            .or_else(|| {
                self.active_connections
                    .iter()
                    .find(|connection| connection.kind != LOOPBACK)
            })
    }

    pub fn find_settings_connection(&self, id: &str) -> Option<&SettingsConnection> {
        self.settings_connections
            .iter()
            .find(|connection| connection.id == id)
    }

    pub fn find_active_by_id(&self, id: &str) -> Option<&ActiveConnection> {
        self.active_connections
            .iter()
            .find(|connection| connection.id == id)
    }

    pub fn find_settings_by_uuid(&self, uuid: &str) -> Option<&SettingsConnection> {
        self.settings_connections
            .iter()
            .find(|connection| connection.uuid == uuid)
    }

    pub fn find_active_by_uuid(&self, uuid: &str) -> Option<&ActiveConnection> {
        self.active_connections
            .iter()
            .find(|connection| connection.uuid == uuid)
    }
}

fn parse_object_number(path: &str) -> u64 {
    path.rsplit_once('/')
        .and_then(|(_, number)| number.parse().ok())
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::platform::network::nmprofile::{CONNECTION, WIRELESS};
    use gtk4::prelude::*;

    pub fn new_properties(entries: &[(&str, Variant)]) -> Properties {
        entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect()
    }

    pub fn new_path(text: &str) -> Variant {
        ObjectPath::try_from(text.to_owned()).unwrap().to_variant()
    }

    pub fn new_paths(texts: &[&str]) -> Variant {
        texts
            .iter()
            .map(|text| ObjectPath::try_from((*text).to_owned()).unwrap())
            .collect::<Vec<_>>()
            .to_variant()
    }

    pub fn insert(
        objects: &mut Objects,
        object: &str,
        interface: &str,
        entries: &[(&str, Variant)],
    ) {
        objects
            .entry(object.to_owned())
            .or_default()
            .insert(interface.to_owned(), new_properties(entries));
    }

    #[test]
    fn device_kinds_and_states_come_from_their_codes() {
        let mut objects = Objects::new();
        for (number, kind, state) in [
            (1, 1u32, 100u32),
            (2, 2, 20),
            (3, 29, 30),
            (4, 32, 50),
            (5, 13, 120),
        ] {
            insert(
                &mut objects,
                &format!("{ROOT}/Devices/{number}"),
                DEVICE,
                &[
                    ("DeviceType", kind.to_variant()),
                    ("State", state.to_variant()),
                    ("Interface", format!("if{number}").to_variant()),
                    ("ActiveConnection", new_path("/")),
                ],
            );
        }
        insert(
            &mut objects,
            &format!("{ROOT}/Devices/6"),
            DEVICE,
            &[
                ("DeviceType", 10u32.to_variant()),
                ("Real", false.to_variant()),
            ],
        );
        let snapshot = Snapshot::new(&objects, &HashMap::new());
        let summary: Vec<_> = snapshot
            .devices
            .iter()
            .map(|device| {
                (
                    device.interface.as_str(),
                    device.kind,
                    device.is_connected(),
                    device.is_connecting(),
                    device.is_disconnected(),
                    device.is_unavailable(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("if1", DeviceKind::Ethernet, true, false, false, false),
                ("if2", DeviceKind::Wifi, false, false, false, true),
                ("if3", DeviceKind::Other, false, false, true, false),
                ("if4", DeviceKind::Other, false, true, false, false),
                ("if5", DeviceKind::Other, false, false, false, false),
            ]
        );
        assert!(
            snapshot
                .devices
                .iter()
                .all(|device| device.active_connection.is_none())
        );
    }

    #[test]
    fn access_points_belong_to_their_device_with_text_names() {
        let device = format!("{ROOT}/Devices/3");
        let mut objects = Objects::new();
        insert(
            &mut objects,
            &device,
            DEVICE,
            &[("DeviceType", 2u32.to_variant())],
        );
        insert(
            &mut objects,
            &device,
            WIRELESS_DEVICE,
            &[
                (
                    "AccessPoints",
                    new_paths(&[
                        "/org/freedesktop/NetworkManager/AccessPoint/7",
                        "/org/freedesktop/NetworkManager/AccessPoint/8",
                        "/org/freedesktop/NetworkManager/AccessPoint/9",
                    ]),
                ),
                (
                    "ActiveAccessPoint",
                    new_path("/org/freedesktop/NetworkManager/AccessPoint/8"),
                ),
                ("WirelessCapabilities", 0x70u32.to_variant()),
                ("LastScan", 4200i64.to_variant()),
            ],
        );
        let new_point = |number: u32, ssid: &[u8], strength: u8| {
            (
                format!("/org/freedesktop/NetworkManager/AccessPoint/{number}"),
                [
                    ("Ssid", ssid.to_variant()),
                    ("Strength", strength.to_variant()),
                    ("Frequency", 5180u32.to_variant()),
                    ("Flags", 1u32.to_variant()),
                    ("RsnFlags", 0x188u32.to_variant()),
                ],
            )
        };
        for (point_path, entries) in [
            new_point(7, "Café".as_bytes(), 40),
            new_point(8, b"Caf\xe9 \xff", 87),
        ] {
            insert(&mut objects, &point_path, ACCESS_POINT, &entries);
        }
        let snapshot = Snapshot::new(&objects, &HashMap::new());
        let device = &snapshot.devices[0];
        let names: Vec<_> = device
            .access_points
            .iter()
            .map(|point| (point.ssid.as_str(), point.strength, point.is_active))
            .collect();
        assert_eq!(
            names,
            [("Café", 40, false), ("Caf\u{fffd} \u{fffd}", 87, true)]
        );
        assert!(device.can_host_access_point());
        assert_eq!(device.last_scan, 4200);
        assert_eq!(
            device.active_point(),
            Some(&AccessPoint {
                path: "/org/freedesktop/NetworkManager/AccessPoint/8".to_owned(),
                device: device.path.clone(),
                ssid: "Caf\u{fffd} \u{fffd}".to_owned(),
                raw_ssid: b"Caf\xe9 \xff".to_vec(),
                strength: 87,
                frequency: 5180,
                is_active: true,
                security: AccessPointSecurity {
                    has_privacy: true,
                    wpa_flags: 0,
                    rsn_flags: 0x188,
                },
            })
        );
    }

    #[test]
    fn access_point_flags_give_the_kind_of_security() {
        let new_security = |has_privacy, wpa_flags, rsn_flags| AccessPointSecurity {
            has_privacy,
            wpa_flags,
            rsn_flags,
        };
        let cases = [
            (new_security(false, 0, 0), false, false, "wpa-psk"),
            (new_security(true, 0, 0), true, false, "wpa-psk"),
            (new_security(true, 0x144, 0), true, false, "wpa-psk"),
            (new_security(true, 0, 0x188), true, false, "wpa-psk"),
            (new_security(true, 0, 0x588), true, false, "wpa-psk"),
            (new_security(true, 0, 0x488), true, false, "sae"),
            (new_security(true, 0x144, 0x488), true, false, "wpa-psk"),
            (new_security(true, 0, 0x288), true, true, "wpa-psk"),
            (new_security(true, 0x244, 0), true, true, "wpa-psk"),
            (new_security(true, 0, 0x88), false, false, "wpa-psk"),
        ];
        for (security, is_secure, is_enterprise, key_management) in cases {
            assert_eq!(
                (
                    security.is_secure(),
                    security.is_enterprise(),
                    security.key_management()
                ),
                (is_secure, is_enterprise, key_management),
                "{security:?}"
            );
        }
    }

    #[test]
    fn the_root_object_gives_the_radio_and_connectivity() {
        let mut objects = Objects::new();
        insert(
            &mut objects,
            ROOT,
            BUS,
            &[
                ("WirelessEnabled", true.to_variant()),
                ("Connectivity", 3u32.to_variant()),
                ("PrimaryConnectionType", "802-3-ethernet".to_variant()),
            ],
        );
        let snapshot = Snapshot::new(&objects, &HashMap::new());
        assert!(snapshot.is_wireless_enabled);
        assert!(snapshot.is_connectivity_limited());
        assert_eq!(snapshot.primary_connection_type, "802-3-ethernet");
        assert_eq!(
            Snapshot::new(&Objects::new(), &HashMap::new()),
            Snapshot::default()
        );
    }

    #[test]
    fn the_main_connection_is_the_primary_one_else_the_first_but_loopback() {
        let mut objects = Objects::new();
        for (number, id, kind) in [
            (10, "Office", "vpn"),
            (1, "lo", LOOPBACK),
            (4, "Wired connection 1", "802-3-ethernet"),
            (6, "Cafe", "802-11-wireless"),
        ] {
            insert(
                &mut objects,
                &format!("{ROOT}/ActiveConnection/{number}"),
                ACTIVE_CONNECTION,
                &[
                    ("Id", id.to_variant()),
                    ("Uuid", format!("u{number}").to_variant()),
                    ("Type", kind.to_variant()),
                    ("Devices", new_paths(&[&format!("{ROOT}/Devices/{number}")])),
                ],
            );
        }
        let find_main_id = |objects: &Objects| {
            Snapshot::new(objects, &HashMap::new())
                .main_connection()
                .map(|connection| connection.id.clone())
        };
        assert_eq!(
            find_main_id(&objects).as_deref(),
            Some("Wired connection 1")
        );
        insert(
            &mut objects,
            ROOT,
            BUS,
            &[(
                "PrimaryConnection",
                new_path("/org/freedesktop/NetworkManager/ActiveConnection/6"),
            )],
        );
        assert_eq!(find_main_id(&objects).as_deref(), Some("Cafe"));
        let snapshot = Snapshot::new(&objects, &HashMap::new());
        let office = snapshot.find_active_by_uuid("u10").unwrap();
        assert_eq!(office.id, "Office");
        assert_eq!(office.devices, [format!("{ROOT}/Devices/10")]);
    }

    #[test]
    fn a_settings_connection_takes_its_names_from_the_profile() {
        let profile = Profile::from_variant(
            &HashMap::from([(
                CONNECTION.to_owned(),
                new_properties(&[
                    ("id", "Cafe".to_variant()),
                    ("uuid", "u1".to_variant()),
                    ("type", WIRELESS.to_variant()),
                    ("autoconnect", false.to_variant()),
                ]),
            )])
            .to_variant(),
        )
        .unwrap();
        assert_eq!(
            SettingsConnection::from_profile(
                "/org/freedesktop/NetworkManager/Settings/1",
                &profile
            ),
            SettingsConnection {
                path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                id: "Cafe".to_owned(),
                uuid: "u1".to_owned(),
                kind: WIRELESS.to_owned(),
                should_autoconnect: false,
                is_port: false,
            }
        );
    }
}

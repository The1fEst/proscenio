use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::services::network::networkmanager::NetworkManager;
use crate::services::network::networkmanager::calls::{self, NO_OBJECT};
use crate::services::network::networkmanager::model::{DeviceKind, LOOPBACK, Snapshot};

const WIREGUARD: &str = "WireGuard";
const WIREGUARD_KIND: &str = "wireguard";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wired {
    pub path: String,
    pub device: String,
    pub connection: String,
    pub is_connected: bool,
}

#[derive(Clone)]
pub struct Net {
    pub symbol: Rc<RefCell<String>>,
    pub name: Rc<RefCell<String>>,
    pub connection: Rc<RefCell<String>>,
    pub wifi_status: Rc<RefCell<String>>,
    pub is_wifi_enabled: Rc<Cell<bool>>,
    pub wifi_symbol: Rc<RefCell<String>>,
    pub ssid: Rc<RefCell<String>>,
    pub wired: Rc<RefCell<Wired>>,
    pub is_wireguard_up: Rc<Cell<bool>>,
    pub tunnels: Rc<RefCell<Vec<(String, bool)>>>,
    manager: Rc<NetworkManager>,
    listeners: Rc<Listeners>,
}

impl Net {
    pub fn new() -> Self {
        let net = Net {
            symbol: Rc::new(RefCell::new("wifi_find".to_owned())),
            name: Rc::new(RefCell::new(String::new())),
            connection: Rc::new(RefCell::new(String::new())),
            wifi_status: Rc::new(RefCell::new("disconnected".to_owned())),
            is_wifi_enabled: Rc::new(Cell::new(false)),
            wifi_symbol: Rc::new(RefCell::new("wifi_find".to_owned())),
            ssid: Rc::new(RefCell::new(String::new())),
            wired: Rc::default(),
            is_wireguard_up: Rc::new(Cell::new(false)),
            tunnels: Rc::default(),
            manager: NetworkManager::instance(),
            listeners: Rc::default(),
        };
        net.show(&net.manager.snapshot());
        net.manager
            .subscribe({
                let net = net.clone();
                move || net.show(&net.manager.snapshot())
            })
            .detach();
        net
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn toggle_wifi(&self) {
        spawn(calls::set_wireless_enabled(!self.is_wifi_enabled.get()));
    }

    pub fn toggle_wired(&self) {
        let wired = self.wired.borrow().clone();
        if wired.path.is_empty() {
            return;
        }
        if wired.is_connected {
            spawn(async move { calls::disconnect_device(&wired.path).await });
        } else {
            spawn(async move {
                calls::activate_connection(NO_OBJECT, &wired.path, NO_OBJECT)
                    .await
                    .map(drop)
            });
        }
    }

    pub fn toggle_wireguard(&self) {
        self.set_tunnel_up(WIREGUARD, !self.is_wireguard_up.get());
    }

    pub fn set_tunnel_up(&self, name: &str, is_up: bool) {
        let snapshot = self.manager.snapshot();
        if is_up {
            let Some(connection) = snapshot.find_settings_connection(name) else {
                return;
            };
            let path = connection.path.clone();
            spawn(async move {
                calls::activate_connection(&path, NO_OBJECT, NO_OBJECT)
                    .await
                    .map(drop)
            });
        } else if let Some(active_connection) = snapshot.find_active_by_id(name) {
            let path = active_connection.path.clone();
            spawn(async move { calls::deactivate_connection(&path).await });
        }
    }

    fn show(&self, snapshot: &Snapshot) {
        let (status, wired) = read_devices(snapshot);
        let is_wifi_enabled = snapshot.is_wireless_enabled;
        let point = snapshot
            .wifi_devices()
            .next()
            .and_then(|device| device.active_point());
        let (wifi_symbol, ssid) = match point {
            _ if !is_wifi_enabled => ("signal_wifi_off".to_owned(), String::new()),
            Some(point) => (bars(point.strength).to_owned(), point.ssid.clone()),
            None => ("wifi_find".to_owned(), String::new()),
        };
        let (symbol, name) = if snapshot.primary_connection_type == WIRED {
            ("lan".to_owned(), "Ethernet".to_owned())
        } else if !is_wifi_enabled {
            (wifi_symbol.clone(), "Off".to_owned())
        } else if ssid.is_empty() {
            (wifi_symbol.clone(), "Disconnected".to_owned())
        } else {
            (wifi_symbol.clone(), ssid.clone())
        };
        let connection = snapshot
            .main_connection()
            .map(|main_connection| main_connection.id.clone())
            .unwrap_or_default();
        let is_wireguard_up = snapshot.find_active_by_id(WIREGUARD).is_some();
        let has_changed = [
            update(&self.connection, connection),
            update(&self.wifi_status, status.to_owned()),
            update(&self.wired, wired),
            update(&self.wifi_symbol, wifi_symbol),
            update(&self.ssid, ssid),
            update(&self.symbol, symbol),
            update(&self.name, name),
            update(&self.tunnels, list_tunnels(snapshot)),
            self.is_wifi_enabled.replace(is_wifi_enabled) != is_wifi_enabled,
            self.is_wireguard_up.replace(is_wireguard_up) != is_wireguard_up,
        ];
        if has_changed.contains(&true) {
            self.listeners.notify();
        }
    }
}

fn update<T: PartialEq>(cell: &RefCell<T>, value: T) -> bool {
    if *cell.borrow() == value {
        return false;
    }
    cell.replace(value);
    true
}

fn spawn(call: impl Future<Output = Result<(), String>> + 'static) {
    glib::spawn_future_local(async move {
        if let Err(message) = call.await {
            eprintln!("network: {message}");
        }
    });
}

fn list_tunnels(snapshot: &Snapshot) -> Vec<(String, bool)> {
    let mut tunnels: Vec<(String, bool)> = snapshot
        .settings_connections
        .iter()
        .filter(|connection| connection.kind == WIREGUARD_KIND)
        .map(|connection| {
            let is_active = snapshot.find_active_by_uuid(&connection.uuid).is_some();
            (connection.id.clone(), is_active)
        })
        .collect();
    tunnels.sort_by_key(|(name, _)| name.to_lowercase());
    tunnels
}

fn bars(strength: u8) -> &'static str {
    match strength {
        84.. => "signal_wifi_4_bar",
        68..=83 => "network_wifi",
        51..=67 => "network_wifi_3_bar",
        34..=50 => "network_wifi_2_bar",
        18..=33 => "network_wifi_1_bar",
        _ => "signal_wifi_0_bar",
    }
}

fn read_devices(snapshot: &Snapshot) -> (&'static str, Wired) {
    let mut status = "disconnected";
    let mut wired: Option<(u8, Wired)> = None;
    for device in &snapshot.devices {
        match device.kind {
            DeviceKind::Wifi => {
                status = if device.is_disconnected() {
                    "disconnected"
                } else if device.is_connected() {
                    if snapshot.is_connectivity_limited() {
                        "limited"
                    } else {
                        "connected"
                    }
                } else if device.is_connecting() {
                    "connecting"
                } else if device.is_unavailable() {
                    "disabled"
                } else {
                    status
                };
            }
            DeviceKind::Ethernet => {
                let is_connected = device.is_connected();
                let rank = if is_connected {
                    2
                } else if device.is_disconnected() {
                    1
                } else if device.is_unavailable() {
                    0
                } else {
                    continue;
                };
                if wired
                    .as_ref()
                    .is_some_and(|(best_rank, _)| *best_rank >= rank)
                {
                    continue;
                }
                let connection = device
                    .active_connection
                    .as_deref()
                    .filter(|_| is_connected)
                    .and_then(|path| snapshot.active_connection(path))
                    .map(|active_connection| active_connection.id.clone())
                    .unwrap_or_default();
                let candidate = Wired {
                    path: device.path.clone(),
                    device: device.interface.clone(),
                    connection,
                    is_connected,
                };
                wired = Some((rank, candidate));
            }
            _ => {}
        }
    }
    (status, wired.map(|(_, wired)| wired).unwrap_or_default())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    pub name: String,
    pub uuid: String,
    pub kind: String,
    pub is_active: bool,
    pub device: String,
    pub is_port: bool,
}

pub const VPN_KINDS: [&str; 4] = ["vpn", "wireguard", "tun", "ip-tunnel"];
pub const VIRTUAL_KINDS: [&str; 3] = ["vlan", "bridge", "bond"];
pub const WIRED: &str = "802-3-ethernet";

fn list_connections(snapshot: &Snapshot) -> Vec<Connection> {
    let mut connections: Vec<Connection> = snapshot
        .settings_connections
        .iter()
        .filter(|connection| connection.kind != LOOPBACK)
        .map(|connection| {
            let active_connection = snapshot.find_active_by_uuid(&connection.uuid);
            let device = active_connection
                .and_then(|active_connection| active_connection.devices.first())
                .and_then(|path| snapshot.devices.iter().find(|device| device.path == *path))
                .map(|device| device.interface.clone())
                .unwrap_or_default();
            Connection {
                name: connection.id.clone(),
                uuid: connection.uuid.clone(),
                kind: connection.kind.clone(),
                is_active: active_connection.is_some(),
                device,
                is_port: connection.is_port,
            }
        })
        .collect();
    connections.sort_by_key(|connection| connection.name.to_lowercase());
    connections
}

pub struct Connections {
    pub list: RefCell<Vec<Connection>>,
    manager: Rc<NetworkManager>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    subscription: RefCell<Option<Subscription>>,
}

impl Connections {
    pub fn new() -> Rc<Self> {
        let manager = NetworkManager::instance();
        let connections = Rc::new(Connections {
            list: RefCell::new(list_connections(&manager.snapshot())),
            manager,
            listeners: RefCell::new(Vec::new()),
            subscription: RefCell::new(None),
        });
        let weak = Rc::downgrade(&connections);
        connections
            .subscription
            .replace(Some(connections.manager.subscribe(move || {
                if let Some(connections) = weak.upgrade() {
                    connections.show();
                }
            })));
        connections
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn set_active(&self, uuid: &str, is_active: bool) {
        let snapshot = self.manager.snapshot();
        if is_active {
            let Some(connection) = snapshot.find_settings_by_uuid(uuid) else {
                return;
            };
            let path = connection.path.clone();
            spawn(async move {
                calls::activate_connection(&path, NO_OBJECT, NO_OBJECT)
                    .await
                    .map(drop)
            });
        } else if let Some(active_connection) = snapshot.find_active_by_uuid(uuid) {
            let path = active_connection.path.clone();
            spawn(async move { calls::deactivate_connection(&path).await });
        }
    }

    fn show(&self) {
        let list = list_connections(&self.manager.snapshot());
        if !update(&self.list, list) {
            return;
        }
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::network::networkmanager::model::{
        ActiveConnection, Device, SettingsConnection,
    };

    fn new_device(
        number: u32,
        kind: DeviceKind,
        state: u32,
        active_connection: Option<&str>,
    ) -> Device {
        Device {
            path: format!("/org/freedesktop/NetworkManager/Devices/{number}"),
            kind,
            state,
            interface: format!("if{number}"),
            active_connection: active_connection.map(str::to_owned),
            access_points: Vec::new(),
            last_scan: 0,
            wireless_capabilities: 0,
        }
    }

    fn new_active(path: &str, id: &str, uuid: &str) -> ActiveConnection {
        ActiveConnection {
            path: path.to_owned(),
            id: id.to_owned(),
            uuid: uuid.to_owned(),
            kind: String::new(),
            devices: vec!["/org/freedesktop/NetworkManager/Devices/2".to_owned()],
        }
    }

    fn new_profile(number: u32, id: &str, kind: &str, is_port: bool) -> SettingsConnection {
        SettingsConnection {
            path: format!("/org/freedesktop/NetworkManager/Settings/{number}"),
            id: id.to_owned(),
            uuid: format!("u{number}"),
            kind: kind.to_owned(),
            should_autoconnect: true,
            is_port,
        }
    }

    #[test]
    fn devices_give_the_wifi_status_and_the_best_wired_device() {
        let active_path = "/org/freedesktop/NetworkManager/ActiveConnection/3";
        let new_wired = |number: u32, connection: &str, is_connected| Wired {
            path: format!("/org/freedesktop/NetworkManager/Devices/{number}"),
            device: format!("if{number}"),
            connection: connection.to_owned(),
            is_connected,
        };
        let cases = [
            (
                vec![
                    new_device(1, DeviceKind::Wifi, 100, None),
                    new_device(2, DeviceKind::Ethernet, 30, None),
                    new_device(3, DeviceKind::Ethernet, 20, None),
                ],
                4,
                ("connected", new_wired(2, "", false)),
            ),
            (
                vec![
                    new_device(1, DeviceKind::Ethernet, 20, None),
                    new_device(2, DeviceKind::Ethernet, 100, Some(active_path)),
                    new_device(3, DeviceKind::Wifi, 20, None),
                ],
                3,
                ("disabled", new_wired(2, "Wired connection 1", true)),
            ),
            (
                vec![
                    new_device(1, DeviceKind::Other, 30, None),
                    new_device(2, DeviceKind::Wifi, 100, None),
                    new_device(3, DeviceKind::Ethernet, 10, None),
                ],
                3,
                ("limited", Wired::default()),
            ),
            (
                vec![new_device(1, DeviceKind::Wifi, 70, None)],
                1,
                ("connecting", Wired::default()),
            ),
        ];
        for (devices, connectivity, expected) in cases {
            let snapshot = Snapshot {
                devices,
                connectivity,
                active_connections: vec![new_active(active_path, "Wired connection 1", "u1")],
                ..Snapshot::default()
            };
            assert_eq!(read_devices(&snapshot), expected);
        }
    }

    #[test]
    fn tunnels_are_the_wireguard_profiles_sorted_by_name() {
        let snapshot = Snapshot {
            settings_connections: vec![
                new_profile(1, "work", WIREGUARD_KIND, false),
                new_profile(2, "Home", WIREGUARD_KIND, false),
                new_profile(3, "Ethernet", WIRED, false),
            ],
            active_connections: vec![new_active("/a/1", "work", "u1")],
            ..Snapshot::default()
        };
        assert_eq!(
            list_tunnels(&snapshot),
            [("Home".to_owned(), false), ("work".to_owned(), true)]
        );
    }

    #[test]
    fn connections_leave_out_loopback_and_come_sorted_by_name() {
        let snapshot = Snapshot {
            devices: vec![
                new_device(1, DeviceKind::Other, 100, None),
                new_device(2, DeviceKind::Ethernet, 100, None),
            ],
            settings_connections: vec![
                new_profile(0, "lo", LOOPBACK, false),
                new_profile(1, "Wired connection 1", WIRED, false),
                new_profile(2, "Office: VPN", "vpn", false),
                new_profile(3, "br0 port eth1", WIRED, true),
            ],
            active_connections: vec![
                new_active(
                    "/org/freedesktop/NetworkManager/ActiveConnection/1",
                    "lo",
                    "u0",
                ),
                new_active(
                    "/org/freedesktop/NetworkManager/ActiveConnection/2",
                    "Wired connection 1",
                    "u1",
                ),
            ],
            ..Snapshot::default()
        };
        assert_eq!(
            list_connections(&snapshot),
            [
                Connection {
                    name: "br0 port eth1".to_owned(),
                    uuid: "u3".to_owned(),
                    kind: WIRED.to_owned(),
                    is_active: false,
                    device: String::new(),
                    is_port: true,
                },
                Connection {
                    name: "Office: VPN".to_owned(),
                    uuid: "u2".to_owned(),
                    kind: "vpn".to_owned(),
                    is_active: false,
                    device: String::new(),
                    is_port: false,
                },
                Connection {
                    name: "Wired connection 1".to_owned(),
                    uuid: "u1".to_owned(),
                    kind: WIRED.to_owned(),
                    is_active: true,
                    device: "if2".to_owned(),
                    is_port: false,
                },
            ]
        );
    }
}

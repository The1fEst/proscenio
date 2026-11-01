use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::dbus;
use crate::platform::security::pam;
use crate::services::network::networkmanager::NetworkManager;
use crate::services::network::networkmanager::calls::{self, NO_OBJECT};
use crate::services::network::networkmanager::model::{BUS, DeviceKind, Snapshot};

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

pub struct NmcliOutput {
    pub is_success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl NmcliOutput {
    pub fn into_stdout(self) -> Result<String, String> {
        if self.is_success {
            return Ok(self.stdout);
        }
        Err(self.stderr.trim().trim_start_matches("Error: ").to_owned())
    }
}

pub async fn nmcli(arguments: &[&str]) -> NmcliOutput {
    let mut command = process::command(&["nmcli"]);
    command.args(arguments).env("LANG", "C").env("LC_ALL", "C");
    let Some(output) = process::capture(command).await else {
        return NmcliOutput {
            is_success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        };
    };
    NmcliOutput {
        is_success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8(output.stdout).unwrap_or_else(|error| {
            let mut bytes = error.into_bytes();
            let text = String::from_utf8_lossy(&bytes).into_owned();
            pam::wipe(&mut bytes);
            text
        }),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

pub fn split_escaped(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                if let Some(escaped) = characters.next() {
                    fields.last_mut().unwrap().push(escaped);
                }
            }
            ':' => fields.push(String::new()),
            other => fields.last_mut().unwrap().push(other),
        }
    }
    fields
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

pub fn parse_connections(text: &str) -> Vec<Connection> {
    let mut connections: Vec<Connection> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let fields = split_escaped(line);
            if fields.len() < 4 || fields[2] == "loopback" {
                return None;
            }
            Some(Connection {
                name: fields[0].clone(),
                uuid: fields[1].clone(),
                kind: fields[2].clone(),
                is_active: fields[3] == "yes",
                device: fields.get(4).cloned().unwrap_or_default(),
                is_port: fields
                    .get(5)
                    .is_some_and(|port| !port.is_empty() && port != "--"),
            })
        })
        .collect();
    connections.sort_by_key(|connection| connection.name.to_lowercase());
    connections
}

pub struct Connections {
    pub list: RefCell<Vec<Connection>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    refresh_timer: Cell<Option<glib::SourceId>>,
    subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl Connections {
    const SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

    pub fn new() -> Rc<Self> {
        let connections = Rc::new(Connections {
            list: RefCell::new(Vec::new()),
            listeners: RefCell::new(Vec::new()),
            refresh_timer: Cell::new(None),
            subscription: RefCell::new(None),
        });
        if let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
            let weak = Rc::downgrade(&connections);
            connections
                .subscription
                .replace(Some(dbus::subscribe_properties_changed(
                    &system,
                    BUS,
                    move || {
                        if let Some(connections) = weak.upgrade() {
                            connections.queue_refresh();
                        }
                    },
                )));
        }
        connections.refresh();
        connections
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn set_active(self: &Rc<Self>, uuid: &str, is_active: bool) {
        let weak = Rc::downgrade(self);
        let uuid = uuid.to_owned();
        glib::spawn_future_local(async move {
            nmcli(&[
                "connection",
                if is_active { "up" } else { "down" },
                "uuid",
                &uuid,
            ])
            .await;
            if let Some(connections) = weak.upgrade() {
                connections.queue_refresh();
            }
        });
    }

    fn queue_refresh(self: &Rc<Self>) {
        if let Some(source) = self.refresh_timer.take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        self.refresh_timer.set(Some(glib::timeout_add_local_once(
            Self::SETTLE_DELAY,
            move || {
                if let Some(connections) = weak.upgrade() {
                    connections.refresh_timer.set(None);
                    connections.refresh();
                }
            },
        )));
    }

    fn refresh(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let output = nmcli(&[
                "-t",
                "-f",
                "NAME,UUID,TYPE,ACTIVE,DEVICE,PORT",
                "connection",
                "show",
            ])
            .await
            .stdout;
            let Some(connections) = weak.upgrade() else {
                return;
            };
            connections.list.replace(parse_connections(&output));
            for listener in connections.listeners.borrow().iter() {
                listener();
            }
        });
    }
}

impl Drop for Connections {
    fn drop(&mut self) {
        if let Some(source) = self.refresh_timer.take() {
            source.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::network::networkmanager::model::{
        ActiveConnection, Device, SettingsConnection,
    };

    #[test]
    fn a_failed_nmcli_call_is_an_error_and_not_empty_output() {
        let new_output = |is_success| NmcliOutput {
            is_success,
            exit_code: Some(if is_success { 0 } else { 8 }),
            stdout: String::new(),
            stderr: "Error: NetworkManager is not running.\n".to_owned(),
        };
        assert_eq!(new_output(true).into_stdout(), Ok(String::new()));
        assert_eq!(
            new_output(false).into_stdout(),
            Err("NetworkManager is not running.".to_owned())
        );
    }

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
        }
    }

    fn new_active(path: &str, id: &str, uuid: &str) -> ActiveConnection {
        ActiveConnection {
            path: path.to_owned(),
            id: id.to_owned(),
            uuid: uuid.to_owned(),
            kind: String::new(),
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
        let new_profile = |number: u32, id: &str, kind: &str| SettingsConnection {
            path: format!("/org/freedesktop/NetworkManager/Settings/{number}"),
            id: id.to_owned(),
            uuid: format!("u{number}"),
            kind: kind.to_owned(),
            should_autoconnect: true,
        };
        let snapshot = Snapshot {
            settings_connections: vec![
                new_profile(1, "work", WIREGUARD_KIND),
                new_profile(2, "Home", WIREGUARD_KIND),
                new_profile(3, "Ethernet", WIRED),
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
        let text = "lo:u0:loopback:yes:lo:\nWired connection 1:u1:802-3-ethernet:yes:eth0:\nOffice\\: VPN:u2:vpn:no::\nbr0 port eth1:u3:802-3-ethernet:no::bridge\n";
        assert_eq!(
            parse_connections(text),
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
                    device: "eth0".to_owned(),
                    is_port: false,
                },
            ]
        );
    }
}

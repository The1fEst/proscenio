use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::dbus;
use crate::platform::security::pam;

pub const BUS: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const WIREGUARD: &str = "WireGuard";
const SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wired {
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
    system: Option<gio::DBusConnection>,
    generation: Rc<Cell<u64>>,
    is_refresh_queued: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
}

impl Net {
    pub fn new(system: Option<gio::DBusConnection>) -> Self {
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
            system,
            generation: Rc::new(Cell::new(0)),
            is_refresh_queued: Rc::new(Cell::new(false)),
            listeners: Rc::default(),
        };
        net.refresh();
        if let Some(system) = &net.system {
            std::mem::forget(dbus::subscribe_properties_changed(system, BUS, {
                let net = net.clone();
                move || net.queue_refresh()
            }));
        }
        net
    }

    fn queue_refresh(&self) {
        if self.is_refresh_queued.replace(true) {
            return;
        }
        let net = self.clone();
        glib::timeout_add_local_once(SETTLE_DELAY, move || {
            net.is_refresh_queued.set(false);
            net.refresh();
        });
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn toggle_wifi(&self) {
        let should_enable = !self.is_wifi_enabled.get();
        let state = if should_enable { "on" } else { "off" };
        process::start(process::quiet(&["nmcli", "radio", "wifi", state]));
    }

    pub fn toggle_wired(&self) {
        let wired = self.wired.borrow();
        if wired.device.is_empty() {
            return;
        }
        let action = if wired.is_connected {
            "disconnect"
        } else {
            "connect"
        };
        process::start(process::quiet(&["nmcli", "device", action, &wired.device]));
    }

    pub fn toggle_wireguard(&self) {
        set_tunnel_up(WIREGUARD, !self.is_wireguard_up.get());
    }

    pub fn refresh(&self) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        let net = self.clone();
        process::read(
            &["sh", "-c", "nmcli -t -f NAME c show --active | head -1"],
            move |output| {
                if net.generation.get() != generation {
                    return;
                }
                net.connection.replace(output.trim_end().to_owned());
                net.announce();
            },
        );
        let net = self.clone();
        let command =
            "nmcli -t -f TYPE,STATE,DEVICE,CONNECTION d status && nmcli -t -f CONNECTIVITY g";
        process::read(&["sh", "-c", command], move |output| {
            if net.generation.get() != generation {
                return;
            }
            let (status, wired) = parse_devices(&output);
            net.wifi_status.replace(status.to_owned());
            net.wired.replace(wired);
            net.announce();
        });
        let net = self.clone();
        glib::spawn_future_local(async move {
            let kind = dbus::string_property(&system, BUS, ROOT, BUS, "PrimaryConnectionType")
                .await
                .unwrap_or_default();
            let is_wifi_enabled = dbus::bool_property(&system, BUS, ROOT, BUS, "WirelessEnabled")
                .await
                .unwrap_or(false);
            let is_wireguard_up = active_names(&system)
                .await
                .iter()
                .any(|name| name == WIREGUARD);
            let (wifi_symbol, ssid) = if !is_wifi_enabled {
                ("signal_wifi_off".to_owned(), String::new())
            } else {
                match access_point(&system).await {
                    Some((strength, ssid)) => (bars(strength).to_owned(), ssid),
                    None => ("wifi_find".to_owned(), String::new()),
                }
            };
            let (symbol, name) = if kind == WIRED {
                ("lan".to_owned(), "Ethernet".to_owned())
            } else if !is_wifi_enabled {
                (wifi_symbol.clone(), "Off".to_owned())
            } else if ssid.is_empty() {
                (wifi_symbol.clone(), "Disconnected".to_owned())
            } else {
                (wifi_symbol.clone(), ssid.clone())
            };
            if net.generation.get() != generation {
                return;
            }
            net.is_wifi_enabled.set(is_wifi_enabled);
            net.is_wireguard_up.set(is_wireguard_up);
            net.wifi_symbol.replace(wifi_symbol);
            net.ssid.replace(ssid);
            net.symbol.replace(symbol);
            net.name.replace(name);
            net.announce();
        });
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

pub fn read_tunnels(on_tunnels: impl Fn(Vec<(String, bool)>) + 'static) {
    process::read(
        &[
            "nmcli",
            "-t",
            "-f",
            "NAME,TYPE,ACTIVE",
            "connection",
            "show",
        ],
        move |output| {
            let list = output
                .lines()
                .filter_map(|line| {
                    let fields: Vec<&str> = line.split(':').collect();
                    let [name, kind, active, ..] = fields.as_slice() else {
                        return None;
                    };
                    (*kind == "wireguard").then(|| ((*name).to_owned(), *active == "yes"))
                })
                .collect();
            on_tunnels(list);
        },
    );
}

pub fn set_tunnel_up(name: &str, is_up: bool) {
    let state = if is_up { "up" } else { "down" };
    process::start(process::quiet(&["nmcli", "connection", state, name]));
}

fn bars(strength: u32) -> &'static str {
    match strength {
        84.. => "signal_wifi_4_bar",
        68..=83 => "network_wifi",
        51..=67 => "network_wifi_3_bar",
        34..=50 => "network_wifi_2_bar",
        18..=33 => "network_wifi_1_bar",
        _ => "signal_wifi_0_bar",
    }
}

fn parse_devices(output: &str) -> (&'static str, Wired) {
    let mut lines: Vec<&str> = output.trim().lines().collect();
    let connectivity = lines.pop().unwrap_or_default();
    let mut status = "disconnected";
    let mut wired: Option<(u8, Wired)> = None;
    for line in lines {
        let fields = split_escaped(line);
        let [kind, state, device, connection, ..] = fields.as_slice() else {
            continue;
        };
        match kind.as_str() {
            "wifi" => {
                status = if state == "disconnected" {
                    "disconnected"
                } else if state.starts_with("connected") {
                    if connectivity == "limited" {
                        "limited"
                    } else {
                        "connected"
                    }
                } else if state.starts_with("connecting") {
                    "connecting"
                } else if state == "unavailable" {
                    "disabled"
                } else {
                    status
                };
            }
            "ethernet" => {
                let is_connected = state.starts_with("connected");
                let rank = match state.as_str() {
                    _ if is_connected => 2,
                    "disconnected" => 1,
                    "unavailable" => 0,
                    _ => continue,
                };
                if wired
                    .as_ref()
                    .is_some_and(|(best_rank, _)| *best_rank >= rank)
                {
                    continue;
                }
                let candidate = Wired {
                    device: device.clone(),
                    connection: if is_connected {
                        connection.clone()
                    } else {
                        String::new()
                    },
                    is_connected,
                };
                wired = Some((rank, candidate));
            }
            _ => {}
        }
    }
    (status, wired.map(|(_, wired)| wired).unwrap_or_default())
}

async fn active_names(system: &gio::DBusConnection) -> Vec<String> {
    let Some(paths) = dbus::property(system, BUS, ROOT, BUS, "ActiveConnections").await else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for path in paths.iter() {
        let Some(path) = path.get::<glib::variant::ObjectPath>() else {
            continue;
        };
        if let Some(name) = dbus::string_property(system, BUS, path.as_str(), ACTIVE, "Id").await {
            names.push(name);
        }
    }
    names
}

fn strength(value: &glib::Variant) -> Option<u32> {
    value.get::<u8>().map(u32::from)
}

async fn access_point(system: &gio::DBusConnection) -> Option<(u32, String)> {
    let device = crate::services::network::wifi::wireless_device(system).await?;
    let point = dbus::path_property(
        system,
        BUS,
        &device,
        "org.freedesktop.NetworkManager.Device.Wireless",
        "ActiveAccessPoint",
    )
    .await?;
    if point == "/" {
        return None;
    }
    let interface = "org.freedesktop.NetworkManager.AccessPoint";
    let strength = strength(&dbus::property(system, BUS, &point, interface, "Strength").await?)?;
    let name = dbus::property(system, BUS, &point, interface, "Ssid")
        .await
        .map(|raw| {
            String::from_utf8_lossy(
                &raw.iter()
                    .filter_map(|byte| byte.get::<u8>())
                    .collect::<Vec<u8>>(),
            )
            .into_owned()
        })
        .unwrap_or_default();
    Some((strength, name))
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

    #[test]
    fn access_point_strength_is_a_byte() {
        use gtk4::glib::prelude::ToVariant;
        assert_eq!(strength(&87u8.to_variant()), Some(87));
    }

    #[test]
    fn devices_give_the_wifi_status_and_the_best_wired_device() {
        let cases = [
            (
                "wifi:connected:wlan0:Home\nethernet:disconnected:eno1:\nethernet:unavailable:usb0:\nfull\n",
                (
                    "connected",
                    Wired {
                        device: "eno1".to_owned(),
                        connection: String::new(),
                        is_connected: false,
                    },
                ),
            ),
            (
                "ethernet:unavailable:usb0:\nethernet:connected:eno1:Wired connection 1\nwifi:unavailable:wlan0:\nlimited\n",
                (
                    "disabled",
                    Wired {
                        device: "eno1".to_owned(),
                        connection: "Wired connection 1".to_owned(),
                        is_connected: true,
                    },
                ),
            ),
            (
                "bt:disconnected:F0\\:CD\\:31:\nwifi:connected:wlan0:Cafe\nlimited\n",
                ("limited", Wired::default()),
            ),
        ];
        for (output, expected) in cases {
            assert_eq!(parse_devices(output), expected, "{output}");
        }
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

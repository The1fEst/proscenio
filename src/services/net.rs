use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::dbus;

pub const BUS: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";

#[derive(Clone)]
pub struct Net {
    pub symbol: Rc<RefCell<String>>,
    pub name: Rc<RefCell<String>>,
    pub connection: Rc<RefCell<String>>,
    pub wifi_status: Rc<RefCell<String>>,
    pub wifi_enabled: Rc<Cell<bool>>,
    pub wireguard: Rc<Cell<bool>>,
    system: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
}

impl Net {
    pub fn new(system: Option<gio::DBusConnection>) -> Self {
        let net = Net {
            symbol: Rc::new(RefCell::new("wifi_find".to_owned())),
            name: Rc::new(RefCell::new(String::new())),
            connection: Rc::new(RefCell::new(String::new())),
            wifi_status: Rc::new(RefCell::new("disconnected".to_owned())),
            wifi_enabled: Rc::new(Cell::new(false)),
            wireguard: Rc::new(Cell::new(false)),
            system,
            listeners: Rc::default(),
        };
        net.refresh();
        if let Some(system) = &net.system {
            let again = net.clone();
            std::mem::forget(dbus::on_properties_changed(system, BUS, move || {
                again.refresh()
            }));
        }
        net
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn toggle_wifi(&self) {
        let on = !self.wifi_enabled.get();
        let state = if on { "on" } else { "off" };
        process::start(process::quiet(&["nmcli", "radio", "wifi", state]));
    }

    pub fn toggle_wireguard(&self) {
        let up = !self.wireguard.get();
        tunnel("WireGuard", up);
    }

    pub fn refresh(&self) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let net = self.clone();
        process::read(
            &["sh", "-c", "nmcli -t -f NAME c show --active | head -1"],
            move |output| {
                net.connection.replace(output.trim_end().to_owned());
                net.announce();
            },
        );
        let net = self.clone();
        let command = "nmcli -t -f TYPE,STATE d status && nmcli -t -f CONNECTIVITY g";
        process::read(&["sh", "-c", command], move |output| {
            let mut lines: Vec<&str> = output.trim().lines().collect();
            let connectivity = lines.pop().unwrap_or_default();
            let mut status = "disconnected";
            for line in lines {
                if line.contains("ethernet") && line.contains("connected") {
                    continue;
                }
                if !line.contains("wifi:") {
                    continue;
                }
                status = if line.contains("disconnected") {
                    "disconnected"
                } else if line.contains("connected") {
                    if connectivity == "limited" {
                        "limited"
                    } else {
                        "connected"
                    }
                } else if line.contains("connecting") {
                    "connecting"
                } else if line.contains("unavailable") {
                    "disabled"
                } else {
                    status
                };
            }
            net.wifi_status.replace(status.to_owned());
            net.announce();
        });
        let net = self.clone();
        glib::spawn_future_local(async move {
            let kind = dbus::string_property(&system, BUS, ROOT, BUS, "PrimaryConnectionType")
                .await
                .unwrap_or_default();
            let enabled = dbus::bool_property(&system, BUS, ROOT, BUS, "WirelessEnabled")
                .await
                .unwrap_or(false);
            net.wifi_enabled.set(enabled);
            net.wireguard.set(kind == "wireguard");

            if kind == "802-3-ethernet" {
                net.symbol.replace("lan".to_owned());
                net.name.replace("Ethernet".to_owned());
            } else if !enabled {
                net.symbol.replace("signal_wifi_off".to_owned());
                net.name.replace("Off".to_owned());
            } else {
                let point = access_point(&system).await;
                match point {
                    Some((strength, name)) => {
                        net.symbol.replace(bars(strength).to_owned());
                        net.name.replace(name);
                    }
                    None => {
                        net.symbol.replace("wifi_find".to_owned());
                        net.name.replace("Disconnected".to_owned());
                    }
                }
            }
            net.announce();
        });
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

pub fn tunnels(handler: impl Fn(Vec<(String, bool)>) + 'static) {
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
            handler(list);
        },
    );
}

pub fn tunnel(name: &str, up: bool) {
    let state = if up { "up" } else { "down" };
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

async fn access_point(system: &gio::DBusConnection) -> Option<(u32, String)> {
    let connection = dbus::path_property(system, BUS, ROOT, BUS, "PrimaryConnection").await?;
    if connection == "/" {
        return None;
    }
    let devices = dbus::property(
        system,
        BUS,
        &connection,
        "org.freedesktop.NetworkManager.Connection.Active",
        "Devices",
    )
    .await?;
    let device = devices
        .iter()
        .next()?
        .get::<glib::variant::ObjectPath>()?
        .as_str()
        .to_owned();
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
    let strength = dbus::u32_property(system, BUS, &point, interface, "Strength").await?;
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

pub struct Finished {
    pub success: bool,
    pub output: String,
    pub errors: String,
}

pub async fn nmcli(arguments: &[&str]) -> Finished {
    let mut command = process::command(&["nmcli"]);
    command.args(arguments).env("LANG", "C").env("LC_ALL", "C");
    let Some(finished) = process::capture(command).await else {
        return Finished {
            success: false,
            output: String::new(),
            errors: String::new(),
        };
    };
    Finished {
        success: finished.status.success(),
        output: String::from_utf8_lossy(&finished.stdout).into_owned(),
        errors: String::from_utf8_lossy(&finished.stderr).into_owned(),
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
    pub active: bool,
    pub device: String,
}

pub const VPN_KINDS: [&str; 4] = ["vpn", "wireguard", "tun", "ip-tunnel"];
pub const WIRED: &str = "802-3-ethernet";

pub fn parse_connections(text: &str) -> Vec<Connection> {
    let mut found: Vec<Connection> = text
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
                active: fields[3] == "yes",
                device: fields.get(4).cloned().unwrap_or_default(),
            })
        })
        .collect();
    found.sort_by_key(|connection| connection.name.to_lowercase());
    found
}

pub struct Connections {
    pub list: RefCell<Vec<Connection>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    refresh_queued: Cell<Option<glib::SourceId>>,
    subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl Connections {
    const SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

    pub fn new() -> Rc<Self> {
        let connections = Rc::new(Connections {
            list: RefCell::new(Vec::new()),
            listeners: RefCell::new(Vec::new()),
            refresh_queued: Cell::new(None),
            subscription: RefCell::new(None),
        });
        if let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
            let weak = Rc::downgrade(&connections);
            connections
                .subscription
                .replace(Some(dbus::on_properties_changed(&system, BUS, move || {
                    if let Some(connections) = weak.upgrade() {
                        connections.queue_refresh();
                    }
                })));
        }
        connections.refresh();
        connections
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn activate(self: &Rc<Self>, uuid: &str, up: bool) {
        let weak = Rc::downgrade(self);
        let uuid = uuid.to_owned();
        glib::spawn_future_local(async move {
            nmcli(&["connection", if up { "up" } else { "down" }, "uuid", &uuid]).await;
            if let Some(connections) = weak.upgrade() {
                connections.queue_refresh();
            }
        });
    }

    fn queue_refresh(self: &Rc<Self>) {
        if let Some(pending) = self.refresh_queued.take() {
            pending.remove();
        }
        let weak = Rc::downgrade(self);
        self.refresh_queued.set(Some(glib::timeout_add_local_once(
            Self::SETTLE,
            move || {
                if let Some(connections) = weak.upgrade() {
                    connections.refresh_queued.set(None);
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
                "NAME,UUID,TYPE,ACTIVE,DEVICE",
                "connection",
                "show",
            ])
            .await
            .output;
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
        if let Some(pending) = self.refresh_queued.take() {
            pending.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connections_leave_out_loopback_and_come_sorted_by_name() {
        let text = "lo:u0:loopback:yes:lo\nWired connection 1:u1:802-3-ethernet:yes:eth0\nOffice\\: VPN:u2:vpn:no:\n";
        assert_eq!(
            parse_connections(text),
            [
                Connection {
                    name: "Office: VPN".to_owned(),
                    uuid: "u2".to_owned(),
                    kind: "vpn".to_owned(),
                    active: false,
                    device: String::new(),
                },
                Connection {
                    name: "Wired connection 1".to_owned(),
                    uuid: "u1".to_owned(),
                    kind: WIRED.to_owned(),
                    active: true,
                    device: "eth0".to_owned(),
                },
            ]
        );
    }
}

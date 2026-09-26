use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::dbus;

pub const BUS: &str = "org.bluez";

#[derive(Clone, PartialEq)]
pub struct Device {
    pub path: String,
    pub address: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
    pub icon: String,
    pub battery: Option<f64>,
}

#[derive(Clone)]
pub struct Bluez {
    pub available: Rc<Cell<bool>>,
    pub powered: Rc<Cell<bool>>,
    pub discovering: Rc<Cell<bool>>,
    pub connected_name: Rc<RefCell<Option<String>>>,
    pub adapter: Rc<RefCell<Option<String>>>,
    pub devices: Rc<RefCell<Vec<Device>>>,
    system: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
}

impl Bluez {
    pub fn new(system: Option<gio::DBusConnection>) -> Self {
        let bluez = Bluez {
            available: Rc::new(Cell::new(false)),
            powered: Rc::new(Cell::new(false)),
            discovering: Rc::new(Cell::new(false)),
            connected_name: Rc::new(RefCell::new(None)),
            adapter: Rc::new(RefCell::new(None)),
            devices: Rc::new(RefCell::new(Vec::new())),
            system,
            listeners: Rc::new(Listeners::default()),
        };
        bluez.refresh();
        if let Some(system) = &bluez.system {
            let again = bluez.clone();
            std::mem::forget(dbus::on_properties_changed(system, BUS, move || {
                again.refresh()
            }));
            let again = bluez.clone();
            std::mem::forget(system.subscribe_to_signal(
                Some(BUS),
                Some("org.freedesktop.DBus.ObjectManager"),
                None,
                None,
                None,
                gio::DBusSignalFlags::NONE,
                move |_| again.refresh(),
            ));
        }
        bluez
    }

    fn call(
        &self,
        path: &str,
        interface: &'static str,
        method: &'static str,
        parameters: Option<Variant>,
    ) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let path = path.to_owned();
        glib::spawn_future_local(async move {
            let _ = system
                .call_future(
                    Some(BUS),
                    &path,
                    interface,
                    method,
                    parameters.as_ref(),
                    None,
                    gio::DBusCallFlags::NONE,
                    20000,
                )
                .await;
        });
    }

    pub fn set_powered(&self, on: bool) {
        let Some(adapter) = self.adapter.borrow().clone() else {
            return;
        };
        self.call(
            &adapter,
            "org.freedesktop.DBus.Properties",
            "Set",
            Some(
                (
                    "org.bluez.Adapter1",
                    "Powered",
                    Variant::from_variant(&on.to_variant()),
                )
                    .to_variant(),
            ),
        );
    }

    pub fn set_discovering(&self, on: bool) {
        let Some(adapter) = self.adapter.borrow().clone() else {
            return;
        };
        self.call(
            &adapter,
            "org.bluez.Adapter1",
            if on {
                "StartDiscovery"
            } else {
                "StopDiscovery"
            },
            None,
        );
    }

    pub fn pair(&self, path: &str) {
        self.call(path, "org.bluez.Device1", "Pair", None);
    }

    pub fn forget(&self, path: &str) {
        let Some(adapter) = self.adapter.borrow().clone() else {
            return;
        };
        let Ok(object) = glib::variant::ObjectPath::try_from(path.to_owned()) else {
            return;
        };
        self.call(
            &adapter,
            "org.bluez.Adapter1",
            "RemoveDevice",
            Some((object,).to_variant()),
        );
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn set_trusted(&self, path: &str) {
        self.call(
            path,
            "org.freedesktop.DBus.Properties",
            "Set",
            Some(
                (
                    "org.bluez.Device1",
                    "Trusted",
                    Variant::from_variant(&true.to_variant()),
                )
                    .to_variant(),
            ),
        );
    }

    pub async fn device_call(&self, path: &str, method: &str, timeout: i32) -> bool {
        let Some(system) = self.system.clone() else {
            return false;
        };
        system
            .call_future(
                Some(BUS),
                path,
                "org.bluez.Device1",
                method,
                None,
                None,
                gio::DBusCallFlags::NONE,
                timeout,
            )
            .await
            .is_ok()
    }

    pub fn toggle(&self) {
        let (Some(system), Some(adapter)) = (self.system.clone(), self.adapter.borrow().clone())
        else {
            return;
        };
        let on = !self.powered.get();
        glib::spawn_future_local(async move {
            let _ = system
                .call_future(
                    Some(BUS),
                    &adapter,
                    "org.freedesktop.DBus.Properties",
                    "Set",
                    Some(
                        &(
                            "org.bluez.Adapter1",
                            "Powered",
                            Variant::from_variant(&on.to_variant()),
                        )
                            .to_variant(),
                    ),
                    None,
                    gio::DBusCallFlags::NONE,
                    4000,
                )
                .await;
        });
    }

    pub fn connect_device(&self, path: &str, connect: bool) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let path = path.to_owned();
        glib::spawn_future_local(async move {
            let _ = system
                .call_future(
                    Some(BUS),
                    &path,
                    "org.bluez.Device1",
                    if connect { "Connect" } else { "Disconnect" },
                    None,
                    None,
                    gio::DBusCallFlags::NONE,
                    20000,
                )
                .await;
        });
    }

    pub fn refresh(&self) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let bluez = self.clone();
        glib::spawn_future_local(async move {
            let Some(objects) = dbus::managed_objects(&system, BUS, "/").await else {
                bluez.available.set(false);
                bluez.announce();
                return;
            };
            let objects = objects.child_value(0);

            let mut adapter = None;
            let mut powered = false;
            let mut discovering = false;
            let mut devices = Vec::new();
            for entry in objects.iter() {
                let path = entry
                    .child_value(0)
                    .get::<glib::variant::ObjectPath>()
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_default();
                let mut battery = None;
                let mut device = None;
                for interface in entry.child_value(1).iter() {
                    let key = interface.child_value(0);
                    let Some(name) = key.str() else {
                        continue;
                    };
                    let properties = interface.child_value(1);
                    match name {
                        "org.bluez.Adapter1" if adapter.is_none() => {
                            adapter = Some(path.clone());
                            powered = boolean(&properties, "Powered");
                            discovering = boolean(&properties, "Discovering");
                        }
                        "org.bluez.Battery1" => {
                            battery = lookup(&properties, "Percentage")
                                .and_then(|value| value.get::<u8>())
                                .map(|percent| percent as f64 / 100.0);
                        }
                        "org.bluez.Device1" => {
                            device = Some(Device {
                                path: path.clone(),
                                address: text(&properties, "Address").unwrap_or_default(),
                                name: text(&properties, "Alias")
                                    .or_else(|| text(&properties, "Name"))
                                    .unwrap_or_default(),
                                connected: boolean(&properties, "Connected"),
                                paired: boolean(&properties, "Paired"),
                                icon: text(&properties, "Icon").unwrap_or_default(),
                                battery: None,
                            })
                        }
                        _ => {}
                    }
                }
                if let Some(mut device) = device {
                    device.battery = battery;
                    devices.push(device);
                }
            }

            devices.sort_by_key(|device| (!device.connected, !device.paired));
            bluez
                .connected_name
                .replace(devices.iter().find(|d| d.connected).map(|d| d.name.clone()));
            bluez.available.set(adapter.is_some());
            bluez.powered.set(powered);
            bluez.discovering.set(discovering);
            bluez.adapter.replace(adapter);
            bluez.devices.replace(devices);
            bluez.announce();
        });
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

const DISCOVERY_POLL: std::time::Duration = std::time::Duration::from_millis(300);
const PAIRING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);
const CONNECTING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const BONDING_SETTLE: std::time::Duration = std::time::Duration::from_secs(5);
const RETRY_EVERY: std::time::Duration = std::time::Duration::from_millis(1500);
const CONNECT_ATTEMPTS: usize = 4;

pub struct Setup {
    bluez: Bluez,
    pub pairing: RefCell<Option<String>>,
    pub waiting: RefCell<Option<String>>,
    pub connecting: RefCell<Option<String>>,
    pub refused: RefCell<Vec<String>>,
    agent: RefCell<Option<gio::Subprocess>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Setup {
    pub fn new(bluez: &Bluez) -> Rc<Self> {
        Rc::new(Setup {
            bluez: bluez.clone(),
            pairing: RefCell::new(None),
            waiting: RefCell::new(None),
            connecting: RefCell::new(None),
            refused: RefCell::new(Vec::new()),
            agent: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
        })
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn busy(&self) -> bool {
        self.pairing.borrow().is_some()
            || self.waiting.borrow().is_some()
            || self.connecting.borrow().is_some()
    }

    fn announce(&self) {
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    fn hold_agent(&self, hold: bool) {
        if let Some(agent) = self.agent.take() {
            agent.force_exit();
        }
        if hold {
            let command = [
                "bluetoothctl",
                "--agent",
                "NoInputNoOutput",
                "--timeout",
                "86400",
                "devices",
            ];
            self.agent.replace(
                gio::Subprocess::newv(
                    &command.map(std::ffi::OsStr::new),
                    gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
                )
                .ok(),
            );
        }
    }

    async fn discovery_stopped(&self, patience: std::time::Duration) -> bool {
        let started = std::time::Instant::now();
        while self.bluez.discovering.get() {
            if started.elapsed() > patience {
                return false;
            }
            glib::timeout_future(DISCOVERY_POLL).await;
        }
        true
    }

    fn connected(&self, path: &str) -> bool {
        self.bluez
            .devices
            .borrow()
            .iter()
            .any(|device| device.path == path && device.connected)
    }

    fn paired(&self, path: &str) -> bool {
        self.bluez
            .devices
            .borrow()
            .iter()
            .any(|device| device.path == path && device.paired)
    }

    pub fn pair(self: &Rc<Self>, path: &str) {
        self.hold_agent(true);
        self.pairing.replace(Some(path.to_owned()));
        self.announce();
        self.bluez.set_discovering(false);
        let weak = Rc::downgrade(self);
        let path = path.to_owned();
        glib::spawn_future_local(async move {
            let Some(setup) = weak.upgrade() else {
                return;
            };
            let paired = setup.discovery_stopped(PAIRING_TIMEOUT).await
                && (setup
                    .bluez
                    .device_call(&path, "Pair", PAIRING_TIMEOUT.as_millis() as i32)
                    .await
                    || setup.paired(&path));
            setup.pairing.replace(None);
            if !paired {
                setup.hold_agent(false);
                setup.announce();
                return;
            }
            setup.bluez.set_trusted(&path);
            setup.waiting.replace(Some(path.clone()));
            setup.announce();
            glib::timeout_future(BONDING_SETTLE).await;
            for _ in 0..CONNECT_ATTEMPTS {
                if setup.connected(&path)
                    || setup
                        .bluez
                        .device_call(&path, "Connect", CONNECTING_TIMEOUT.as_millis() as i32)
                        .await
                {
                    break;
                }
                glib::timeout_future(RETRY_EVERY).await;
            }
            setup.waiting.replace(None);
            setup.hold_agent(false);
            setup.announce();
        });
    }

    pub fn connect(self: &Rc<Self>, path: &str) {
        self.connecting.replace(Some(path.to_owned()));
        self.refused.borrow_mut().retain(|refused| refused != path);
        self.announce();
        self.bluez.set_discovering(false);
        let weak = Rc::downgrade(self);
        let path = path.to_owned();
        glib::spawn_future_local(async move {
            let Some(setup) = weak.upgrade() else {
                return;
            };
            let connected = setup.discovery_stopped(CONNECTING_TIMEOUT).await
                && setup
                    .bluez
                    .device_call(&path, "Connect", CONNECTING_TIMEOUT.as_millis() as i32)
                    .await;
            setup.connecting.replace(None);
            if !connected {
                setup.refused.borrow_mut().push(path);
            }
            setup.announce();
        });
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        if let Some(agent) = self.agent.take() {
            agent.force_exit();
        }
    }
}

pub fn friendly_order(devices: &[Device]) -> Vec<Device> {
    let just_address = |name: &str| {
        let parts: Vec<&str> = name.split('-').collect();
        parts.len() == 6
            && parts
                .iter()
                .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
    };
    let mut sorted = devices.to_vec();
    sorted.sort_by(|a, b| {
        let group = |device: &Device| match (device.connected, device.paired) {
            (true, _) => 0,
            (false, true) => 1,
            (false, false) => 2,
        };
        group(a)
            .cmp(&group(b))
            .then(just_address(&a.name).cmp(&just_address(&b.name)))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    sorted
}

fn lookup(properties: &Variant, key: &str) -> Option<Variant> {
    properties
        .iter()
        .find(|entry| entry.child_value(0).str() == Some(key))?
        .child_value(1)
        .as_variant()
}

fn boolean(properties: &Variant, key: &str) -> bool {
    lookup(properties, key)
        .and_then(|value| value.get::<bool>())
        .unwrap_or(false)
}

fn text(properties: &Variant, key: &str) -> Option<String> {
    lookup(properties, key)?.str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, connected: bool, paired: bool) -> Device {
        Device {
            path: name.to_owned(),
            address: String::new(),
            name: name.to_owned(),
            connected,
            paired,
            icon: String::new(),
            battery: None,
        }
    }

    #[test]
    fn devices_come_connected_then_paired_then_new_with_bare_addresses_last() {
        let devices = [
            device("zebra", false, false),
            device("AA-BB-CC-DD-EE-FF", false, false),
            device("Keyboard", false, true),
            device("buds", true, true),
            device("apple", false, false),
        ];
        let names: Vec<String> = friendly_order(&devices)
            .into_iter()
            .map(|device| device.name)
            .collect();
        assert_eq!(
            names,
            ["buds", "Keyboard", "apple", "zebra", "AA-BB-CC-DD-EE-FF"]
        );
    }
}

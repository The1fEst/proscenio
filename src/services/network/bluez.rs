use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::dbus;

pub const BUS: &str = "org.bluez";
const SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(Clone, PartialEq)]
pub struct Device {
    pub path: String,
    pub address: String,
    pub name: String,
    pub is_connected: bool,
    pub is_paired: bool,
    pub icon: String,
    pub battery: Option<f64>,
}

#[derive(Clone)]
pub struct Bluez {
    pub is_available: Rc<Cell<bool>>,
    pub is_powered: Rc<Cell<bool>>,
    pub is_discovering: Rc<Cell<bool>>,
    pub connected_name: Rc<RefCell<Option<String>>>,
    pub adapter: Rc<RefCell<Option<String>>>,
    pub devices: Rc<RefCell<Vec<Device>>>,
    system: Option<gio::DBusConnection>,
    is_refresh_queued: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
}

impl Bluez {
    pub fn new(system: Option<gio::DBusConnection>) -> Self {
        let bluez = Bluez {
            is_available: Rc::new(Cell::new(false)),
            is_powered: Rc::new(Cell::new(false)),
            is_discovering: Rc::new(Cell::new(false)),
            connected_name: Rc::new(RefCell::new(None)),
            adapter: Rc::new(RefCell::new(None)),
            devices: Rc::new(RefCell::new(Vec::new())),
            system,
            is_refresh_queued: Rc::new(Cell::new(false)),
            listeners: Rc::new(Listeners::default()),
        };
        bluez.refresh();
        if let Some(system) = &bluez.system {
            std::mem::forget(dbus::subscribe_properties_changed(system, BUS, {
                let bluez = bluez.clone();
                move || bluez.queue_refresh()
            }));
            std::mem::forget(system.subscribe_to_signal(
                Some(BUS),
                Some("org.freedesktop.DBus.ObjectManager"),
                None,
                None,
                None,
                gio::DBusSignalFlags::NONE,
                {
                    let bluez = bluez.clone();
                    move |_| bluez.queue_refresh()
                },
            ));
        }
        bluez
    }

    fn queue_refresh(&self) {
        if self.is_refresh_queued.replace(true) {
            return;
        }
        let bluez = self.clone();
        glib::timeout_add_local_once(SETTLE_DELAY, move || {
            bluez.is_refresh_queued.set(false);
            bluez.refresh();
        });
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

    pub fn set_powered(&self, is_powered: bool) {
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
                    Variant::from_variant(&is_powered.to_variant()),
                )
                    .to_variant(),
            ),
        );
    }

    pub fn set_discovering(&self, is_discovering: bool) {
        let Some(adapter) = self.adapter.borrow().clone() else {
            return;
        };
        self.call(
            &adapter,
            "org.bluez.Adapter1",
            if is_discovering {
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

    pub async fn call_device(&self, path: &str, method: &str, timeout: i32) -> bool {
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
        let should_power_on = !self.is_powered.get();
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
                            Variant::from_variant(&should_power_on.to_variant()),
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

    pub fn set_device_connected(&self, path: &str, is_connected: bool) {
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
                    if is_connected {
                        "Connect"
                    } else {
                        "Disconnect"
                    },
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
                bluez.is_available.set(false);
                bluez.announce();
                return;
            };
            let objects = objects.child_value(0);

            let mut adapter = None;
            let mut is_powered = false;
            let mut is_discovering = false;
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
                            is_powered = boolean(&properties, "Powered");
                            is_discovering = boolean(&properties, "Discovering");
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
                                is_connected: boolean(&properties, "Connected"),
                                is_paired: boolean(&properties, "Paired"),
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

            devices.sort_by_key(|device| (!device.is_connected, !device.is_paired));
            bluez.connected_name.replace(
                devices
                    .iter()
                    .find(|d| d.is_connected)
                    .map(|d| d.name.clone()),
            );
            bluez.is_available.set(adapter.is_some());
            bluez.is_powered.set(is_powered);
            bluez.is_discovering.set(is_discovering);
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
    pub pairing_path: RefCell<Option<String>>,
    pub bonding_path: RefCell<Option<String>>,
    pub connecting_path: RefCell<Option<String>>,
    pub refused_paths: RefCell<Vec<String>>,
    agent: RefCell<Option<process::Running>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Setup {
    pub fn new(bluez: &Bluez) -> Rc<Self> {
        Rc::new(Setup {
            bluez: bluez.clone(),
            pairing_path: RefCell::new(None),
            bonding_path: RefCell::new(None),
            connecting_path: RefCell::new(None),
            refused_paths: RefCell::new(Vec::new()),
            agent: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
        })
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn is_busy(&self) -> bool {
        self.pairing_path.borrow().is_some()
            || self.bonding_path.borrow().is_some()
            || self.connecting_path.borrow().is_some()
    }

    fn announce(&self) {
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    fn hold_agent(&self, should_hold: bool) {
        if let Some(mut agent) = self.agent.take() {
            agent.stop();
        }
        if should_hold {
            let command = [
                "bluetoothctl",
                "--agent",
                "NoInputNoOutput",
                "--timeout",
                "86400",
                "devices",
            ];
            self.agent.replace(process::start(process::quiet(&command)));
        }
    }

    async fn wait_for_discovery_stop(&self, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while self.bluez.is_discovering.get() {
            if start.elapsed() > timeout {
                return false;
            }
            glib::timeout_future(DISCOVERY_POLL).await;
        }
        true
    }

    fn is_connected(&self, path: &str) -> bool {
        self.bluez
            .devices
            .borrow()
            .iter()
            .any(|device| device.path == path && device.is_connected)
    }

    fn is_paired(&self, path: &str) -> bool {
        self.bluez
            .devices
            .borrow()
            .iter()
            .any(|device| device.path == path && device.is_paired)
    }

    pub fn pair(self: &Rc<Self>, path: &str) {
        self.hold_agent(true);
        self.pairing_path.replace(Some(path.to_owned()));
        self.announce();
        self.bluez.set_discovering(false);
        let weak = Rc::downgrade(self);
        let path = path.to_owned();
        glib::spawn_future_local(async move {
            let Some(setup) = weak.upgrade() else {
                return;
            };
            let is_paired = setup.wait_for_discovery_stop(PAIRING_TIMEOUT).await
                && (setup
                    .bluez
                    .call_device(&path, "Pair", PAIRING_TIMEOUT.as_millis() as i32)
                    .await
                    || setup.is_paired(&path));
            setup.pairing_path.replace(None);
            if !is_paired {
                setup.hold_agent(false);
                setup.announce();
                return;
            }
            setup.bluez.set_trusted(&path);
            setup.bonding_path.replace(Some(path.clone()));
            setup.announce();
            glib::timeout_future(BONDING_SETTLE).await;
            for _ in 0..CONNECT_ATTEMPTS {
                if setup.is_connected(&path)
                    || setup
                        .bluez
                        .call_device(&path, "Connect", CONNECTING_TIMEOUT.as_millis() as i32)
                        .await
                {
                    break;
                }
                glib::timeout_future(RETRY_EVERY).await;
            }
            setup.bonding_path.replace(None);
            setup.hold_agent(false);
            setup.announce();
        });
    }

    pub fn connect(self: &Rc<Self>, path: &str) {
        self.connecting_path.replace(Some(path.to_owned()));
        self.refused_paths
            .borrow_mut()
            .retain(|refused_path| refused_path != path);
        self.announce();
        self.bluez.set_discovering(false);
        let weak = Rc::downgrade(self);
        let path = path.to_owned();
        glib::spawn_future_local(async move {
            let Some(setup) = weak.upgrade() else {
                return;
            };
            let is_connected = setup.wait_for_discovery_stop(CONNECTING_TIMEOUT).await
                && setup
                    .bluez
                    .call_device(&path, "Connect", CONNECTING_TIMEOUT.as_millis() as i32)
                    .await;
            setup.connecting_path.replace(None);
            if !is_connected {
                setup.refused_paths.borrow_mut().push(path);
            }
            setup.announce();
        });
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        if let Some(mut agent) = self.agent.take() {
            agent.stop();
        }
    }
}

pub fn friendly_order(devices: &[Device]) -> Vec<Device> {
    let is_bare_address = |name: &str| {
        let parts: Vec<&str> = name.split('-').collect();
        parts.len() == 6
            && parts
                .iter()
                .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
    };
    let mut sorted_devices = devices.to_vec();
    sorted_devices.sort_by(|a, b| {
        let group = |device: &Device| match (device.is_connected, device.is_paired) {
            (true, _) => 0,
            (false, true) => 1,
            (false, false) => 2,
        };
        group(a)
            .cmp(&group(b))
            .then(is_bare_address(&a.name).cmp(&is_bare_address(&b.name)))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    sorted_devices
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

    fn device(name: &str, is_connected: bool, is_paired: bool) -> Device {
        Device {
            path: name.to_owned(),
            address: String::new(),
            name: name.to_owned(),
            is_connected,
            is_paired,
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

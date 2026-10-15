use gtk4::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hyprland::devicesettings::{self, Change, Setting};
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprconfig::{self, Area};

const WRITE_DELAY: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct DeviceOptions {
    settings: RefCell<Vec<Setting>>,
    lists_mice: Cell<bool>,
    mice: RefCell<Vec<String>>,
    has_list: Cell<bool>,
    pending_changes: RefCell<Vec<Change>>,
    write_timer: Cell<Option<glib::SourceId>>,
    generation: Cell<u64>,
    listeners: Listeners,
}

fn collect_device_names(devices: &serde_json::Value, kind: &str) -> Vec<String> {
    devices
        .get(kind)
        .and_then(|list| list.as_array())
        .into_iter()
        .flatten()
        .filter_map(|device| device.get("name")?.as_str().map(str::to_owned))
        .collect()
}

impl DeviceOptions {
    pub fn new() -> Rc<Self> {
        let options = Rc::new(DeviceOptions::default());
        options.reload();
        options
    }

    pub fn with_mice() -> Rc<Self> {
        let options = Rc::new(DeviceOptions::default());
        options.lists_mice.set(true);
        options.reload();
        options
    }

    pub fn watch(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn reload(self: &Rc<Self>) {
        self.settings.replace(devicesettings::read());
        self.listeners.notify();
        if !self.lists_mice.get() {
            return;
        }
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        let options = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(mice) = gio::spawn_blocking(|| {
                collect_device_names(&hypr::json("devices").unwrap_or_default(), "mice")
            })
            .await
            else {
                return;
            };
            let Some(options) = options.upgrade() else {
                return;
            };
            if options.generation.get() != generation {
                return;
            }
            let is_first = !options.has_list.replace(true);
            if !is_first && *options.mice.borrow() == mice {
                return;
            }
            options.mice.replace(mice);
            options.listeners.notify();
        });
    }

    pub fn mice(&self) -> Vec<String> {
        self.mice.borrow().clone()
    }

    pub fn has_list(&self) -> bool {
        self.has_list.get()
    }

    pub fn value_of(&self, device: &str, key: &str) -> String {
        self.settings
            .borrow()
            .iter()
            .find(|setting| setting.device == device && setting.key == key)
            .map(|setting| setting.value.clone())
            .unwrap_or_default()
    }

    pub fn override_count(&self, device: &str, keys: &[&str]) -> usize {
        self.settings
            .borrow()
            .iter()
            .filter(|setting| setting.device == device && keys.contains(&setting.key.as_str()))
            .count()
    }

    pub fn set(self: &Rc<Self>, device: &str, key: &str, value: &str) {
        if device.is_empty() {
            return;
        }
        self.queue(Change::Set(
            device.to_owned(),
            key.to_owned(),
            value.to_owned(),
        ));
    }

    pub fn unset(self: &Rc<Self>, device: &str, key: &str) {
        self.queue(Change::Unset(device.to_owned(), key.to_owned()));
    }

    fn queue(self: &Rc<Self>, change: Change) {
        self.pending_changes.borrow_mut().push(change);
        if let Some(source) = self.write_timer.take() {
            source.remove();
        }
        let options = Rc::downgrade(self);
        self.write_timer
            .set(Some(glib::timeout_add_local_once(WRITE_DELAY, move || {
                if let Some(options) = options.upgrade() {
                    options.write_timer.set(None);
                    if options.persist() {
                        options.reload();
                    }
                }
            })));
    }

    fn persist(&self) -> bool {
        let changes = self.pending_changes.take();
        if changes.is_empty() {
            return false;
        }
        match devicesettings::apply(&changes) {
            Ok(()) => drop(gio::spawn_blocking(|| hypr::request("reload"))),
            Err(error) => eprintln!(
                "deviceoptions: {} could not be written: {error}",
                hyprconfig::path(Area::Devices).display()
            ),
        }
        true
    }
}

impl Drop for DeviceOptions {
    fn drop(&mut self) {
        if let Some(source) = self.write_timer.take() {
            source.remove();
        }
        self.persist();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_asked_keys_count_as_overrides() {
        let options = DeviceOptions::default();
        options.settings.replace(vec![Setting {
            device: "mouse".to_owned(),
            key: "enabled".to_owned(),
            value: "false".to_owned(),
        }]);
        assert_eq!(
            options.override_count("mouse", &["sensitivity", "natural_scroll"]),
            0
        );
        assert_eq!(options.override_count("mouse", &["enabled"]), 1);
    }
}

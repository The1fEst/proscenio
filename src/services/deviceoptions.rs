use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::devicesettings::{self, Change, Setting};
use crate::platform::hypr;

const WRITE_DELAY: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct DeviceOptions {
    settings: RefCell<Vec<Setting>>,
    mice: RefCell<Vec<String>>,
    pending: RefCell<Vec<Change>>,
    writing: Cell<Option<glib::SourceId>>,
    listeners: Listeners,
}

fn names(devices: &serde_json::Value, kind: &str) -> Vec<String> {
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

    pub fn watch(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn reload(&self) {
        self.settings.replace(devicesettings::read());
        let devices = hypr::json("devices").unwrap_or_default();
        self.mice.replace(names(&devices, "mice"));
        self.listeners.notify();
    }

    pub fn mice(&self) -> Vec<String> {
        self.mice.borrow().clone()
    }

    pub fn value_of(&self, device: &str, key: &str) -> String {
        self.settings
            .borrow()
            .iter()
            .find(|setting| setting.device == device && setting.key == key)
            .map(|setting| setting.value.clone())
            .unwrap_or_default()
    }

    pub fn overrides(&self, device: &str) -> usize {
        self.settings
            .borrow()
            .iter()
            .filter(|setting| setting.device == device)
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
        self.pending.borrow_mut().push(change);
        if let Some(source) = self.writing.take() {
            source.remove();
        }
        let options = Rc::downgrade(self);
        self.writing
            .set(Some(glib::timeout_add_local_once(WRITE_DELAY, move || {
                if let Some(options) = options.upgrade() {
                    options.writing.set(None);
                    if options.persist() {
                        options.reload();
                    }
                }
            })));
    }

    fn persist(&self) -> bool {
        let changes = self.pending.take();
        if changes.is_empty() {
            return false;
        }
        let _ = devicesettings::apply(&changes);
        hypr::request("reload");
        true
    }
}

impl Drop for DeviceOptions {
    fn drop(&mut self) {
        if let Some(source) = self.writing.take() {
            source.remove();
        }
        self.persist();
    }
}

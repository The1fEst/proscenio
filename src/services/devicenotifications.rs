use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config;
use crate::core::i18n::{tr, trf};
use crate::platform::notify::{self, Notification, Urgency};
use crate::platform::readable;
use crate::platform::udev::{Device, Monitor};
use crate::services::audio;

const QUIET: Duration = Duration::from_millis(500);
const SUBSYSTEM: &str = "usb";
const USB_DEVICE: &str = "usb_device";
const APP: &str = "Shell";
const ICON: &str = "drive-removable-media-usb";

pub struct DeviceNotifications {
    monitor: Monitor,
    display_names: RefCell<HashMap<String, String>>,
    removable: RefCell<Vec<String>>,
    added_quiet: Cell<bool>,
    removed_quiet: Cell<bool>,
    added: Cell<Option<u32>>,
    removed: Cell<Option<u32>>,
}

impl DeviceNotifications {
    pub fn start() -> Option<Rc<Self>> {
        let monitor = Monitor::subsystem(SUBSYSTEM)?;
        let fd = monitor.fd();
        let notifications = Rc::new(DeviceNotifications {
            monitor,
            display_names: RefCell::new(HashMap::new()),
            removable: RefCell::new(Vec::new()),
            added_quiet: Cell::new(false),
            removed_quiet: Cell::new(false),
            added: Cell::new(None),
            removed: Cell::new(None),
        });
        let weak = Rc::downgrade(&notifications);
        readable::when_readable(fd, move || {
            let Some(notifications) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            notifications.receive();
            glib::ControlFlow::Continue
        });
        Some(notifications)
    }

    fn receive(self: &Rc<Self>) {
        let Some(device) = self.monitor.receive() else {
            return;
        };
        match device.action().as_str() {
            "add" => self.device_added(&device),
            "remove" => self.device_removed(&device),
            _ => {}
        }
    }

    fn device_added(self: &Rc<Self>, device: &Device) {
        if device.device_type() != USB_DEVICE || !device.is_removable() {
            return;
        }
        let path = device.sysfs_path();
        self.removable.borrow_mut().push(path.clone());
        let display_name = device.display_name();
        if !display_name.is_empty() {
            self.display_names
                .borrow_mut()
                .insert(path, display_name.clone());
        }
        if self.added_quiet.get() {
            return;
        }
        close(&self.removed);
        close(&self.added);
        let text = if display_name.is_empty() {
            tr("A USB device has been connected.")
        } else {
            trf(
                "%1 has been connected.",
                &[&glib::markup_escape_text(&display_name)],
            )
        };
        self.show(
            |notifications| &notifications.added,
            &tr("USB Device Detected"),
            &text,
            "device-added",
        );
        self.quiet(|notifications| &notifications.added_quiet);
    }

    fn device_removed(self: &Rc<Self>, device: &Device) {
        if device.device_type() != USB_DEVICE {
            return;
        }
        let path = device.sysfs_path();
        let display_name = self
            .display_names
            .borrow_mut()
            .remove(&path)
            .unwrap_or_default();
        let known = {
            let mut removable = self.removable.borrow_mut();
            let at = removable.iter().position(|known| *known == path);
            at.map(|at| removable.remove(at)).is_some()
        };
        if !known && !device.is_removable() {
            return;
        }
        if self.removed_quiet.get() {
            return;
        }
        close(&self.added);
        close(&self.removed);
        let text = if display_name.is_empty() {
            tr("A USB device has been disconnected.")
        } else {
            trf(
                "%1 has been disconnected.",
                &[&glib::markup_escape_text(&display_name)],
            )
        };
        self.show(
            |notifications| &notifications.removed,
            &tr("USB Device Removed"),
            &text,
            "device-removed",
        );
        self.quiet(|notifications| &notifications.removed_quiet);
    }

    fn show(
        self: &Rc<Self>,
        slot: fn(&Self) -> &Cell<Option<u32>>,
        summary: &str,
        body: &str,
        sound: &str,
    ) {
        let weak = Rc::downgrade(self);
        notify::send_then(
            &Notification {
                app: APP,
                summary,
                body,
                icon: ICON,
                urgency: Urgency::Low,
                ..Default::default()
            },
            move |id| {
                if let Some(notifications) = weak.upgrade() {
                    slot(&notifications).set(Some(id));
                }
            },
        );
        let config = config::current();
        if config.sounds_devices {
            audio::play_system_sound(&config.sounds_theme, sound);
        }
    }

    fn quiet(self: &Rc<Self>, flag: fn(&Self) -> &Cell<bool>) {
        flag(self).set(true);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(QUIET, move || {
            if let Some(notifications) = weak.upgrade() {
                flag(&notifications).set(false);
            }
        });
    }
}

fn close(slot: &Cell<Option<u32>>) {
    if let Some(id) = slot.take() {
        notify::close(id);
    }
}

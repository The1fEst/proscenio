use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config;
use crate::core::i18n::{tr, trf};
use crate::platform::dbus::notify::{self, Notification, Urgency};
use crate::platform::system::udev::{Device, Monitor};
use crate::platform::wayland::readable;
use crate::services::media::audio;

const QUIET_PERIOD: Duration = Duration::from_millis(500);
const SUBSYSTEM: &str = "usb";
const USB_DEVICE: &str = "usb_device";
const APP: &str = "Shell";
const ICON: &str = "drive-removable-media-usb";

pub struct DeviceNotifications {
    monitor: Monitor,
    display_names: RefCell<HashMap<String, String>>,
    removable_paths: RefCell<Vec<String>>,
    is_added_quiet: Cell<bool>,
    is_removed_quiet: Cell<bool>,
    added_notification_id: Cell<Option<u32>>,
    removed_notification_id: Cell<Option<u32>>,
}

impl DeviceNotifications {
    pub fn start() -> Option<Rc<Self>> {
        let monitor = Monitor::for_subsystem(SUBSYSTEM)?;
        let fd = monitor.fd();
        let notifications = Rc::new(DeviceNotifications {
            monitor,
            display_names: RefCell::new(HashMap::new()),
            removable_paths: RefCell::new(Vec::new()),
            is_added_quiet: Cell::new(false),
            is_removed_quiet: Cell::new(false),
            added_notification_id: Cell::new(None),
            removed_notification_id: Cell::new(None),
        });
        let weak = Rc::downgrade(&notifications);
        readable::add_watch(fd, move || {
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
            "add" => self.on_device_added(&device),
            "remove" => self.on_device_removed(&device),
            _ => {}
        }
    }

    fn on_device_added(self: &Rc<Self>, device: &Device) {
        if device.device_type() != USB_DEVICE || !device.is_removable() {
            return;
        }
        let path = device.sysfs_path();
        self.removable_paths.borrow_mut().push(path.clone());
        let display_name = device.display_name();
        if !display_name.is_empty() {
            self.display_names
                .borrow_mut()
                .insert(path, display_name.clone());
        }
        if self.is_added_quiet.get() {
            return;
        }
        close(&self.removed_notification_id);
        close(&self.added_notification_id);
        let text = if display_name.is_empty() {
            tr("A USB device has been connected.")
        } else {
            trf(
                "%1 has been connected.",
                &[&glib::markup_escape_text(&display_name)],
            )
        };
        self.show(
            |notifications| &notifications.added_notification_id,
            &tr("USB Device Detected"),
            &text,
            "device-added",
        );
        self.quiet(|notifications| &notifications.is_added_quiet);
    }

    fn on_device_removed(self: &Rc<Self>, device: &Device) {
        if device.device_type() != USB_DEVICE {
            return;
        }
        let path = device.sysfs_path();
        let display_name = self
            .display_names
            .borrow_mut()
            .remove(&path)
            .unwrap_or_default();
        let was_known = {
            let mut removable_paths = self.removable_paths.borrow_mut();
            let index = removable_paths
                .iter()
                .position(|known_path| *known_path == path);
            index.map(|index| removable_paths.remove(index)).is_some()
        };
        if !was_known && !device.is_removable() {
            return;
        }
        if self.is_removed_quiet.get() {
            return;
        }
        close(&self.added_notification_id);
        close(&self.removed_notification_id);
        let text = if display_name.is_empty() {
            tr("A USB device has been disconnected.")
        } else {
            trf(
                "%1 has been disconnected.",
                &[&glib::markup_escape_text(&display_name)],
            )
        };
        self.show(
            |notifications| &notifications.removed_notification_id,
            &tr("USB Device Removed"),
            &text,
            "device-removed",
        );
        self.quiet(|notifications| &notifications.is_removed_quiet);
    }

    fn show(
        self: &Rc<Self>,
        slot: fn(&Self) -> &Cell<Option<u32>>,
        summary: &str,
        body: &str,
        sound_name: &str,
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
            audio::play_system_sound(&config.sounds_theme, sound_name);
        }
    }

    fn quiet(self: &Rc<Self>, flag: fn(&Self) -> &Cell<bool>) {
        flag(self).set(true);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(QUIET_PERIOD, move || {
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

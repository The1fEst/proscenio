use gtk4::gdk_pixbuf::{Colorspace, Pixbuf};
use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config;
use crate::core::listeners::{Listeners, Subscription};
use crate::platform::desktop;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

const INTROSPECTION: &str = "
<node>
  <interface name='org.freedesktop.Notifications'>
    <method name='GetCapabilities'>
      <arg type='as' name='capabilities' direction='out'/>
    </method>
    <method name='Notify'>
      <arg type='s' name='app_name' direction='in'/>
      <arg type='u' name='replaces_id' direction='in'/>
      <arg type='s' name='app_icon' direction='in'/>
      <arg type='s' name='summary' direction='in'/>
      <arg type='s' name='body' direction='in'/>
      <arg type='as' name='actions' direction='in'/>
      <arg type='a{sv}' name='hints' direction='in'/>
      <arg type='i' name='expire_timeout' direction='in'/>
      <arg type='u' name='id' direction='out'/>
    </method>
    <method name='CloseNotification'>
      <arg type='u' name='id' direction='in'/>
    </method>
    <method name='GetServerInformation'>
      <arg type='s' name='name' direction='out'/>
      <arg type='s' name='vendor' direction='out'/>
      <arg type='s' name='version' direction='out'/>
      <arg type='s' name='spec_version' direction='out'/>
    </method>
    <signal name='NotificationClosed'>
      <arg type='u' name='id'/>
      <arg type='u' name='reason'/>
    </signal>
    <signal name='ActionInvoked'>
      <arg type='u' name='id'/>
      <arg type='s' name='action_key'/>
    </signal>
  </interface>
</node>";

const CAPABILITIES: &[&str] = &[
    "actions",
    "body",
    "body-hyperlinks",
    "body-images",
    "body-markup",
    "icon-static",
    "inline-reply",
    "persistence",
];

#[derive(Clone)]
pub struct Notification {
    pub id: u32,
    pub actions: Vec<(String, String)>,
    pub app_icon: String,
    pub app_name: String,
    pub body: String,
    pub image: String,
    pub summary: String,
    pub time: i64,
    pub urgency: u8,
    pub transient: bool,
    pub popup: bool,
    pub timeout: i32,
}

pub struct Group {
    pub app_name: String,
    pub app_icon: String,
    pub time: i64,
    pub notifications: Vec<Notification>,
}

impl Group {
    pub fn urgent(&self) -> bool {
        self.notifications.iter().any(|entry| entry.urgency == 2)
    }

    pub fn ids(&self) -> Vec<u32> {
        self.notifications.iter().map(|entry| entry.id).collect()
    }
}

impl Notification {
    fn to_json(&self) -> Value {
        json!({
            "notificationId": self.id,
            "actions": self.actions.iter().map(|(identifier, text)| json!({
                "identifier": identifier,
                "text": text,
            })).collect::<Vec<_>>(),
            "appIcon": self.app_icon,
            "appName": self.app_name,
            "body": self.body,
            "image": self.image,
            "summary": self.summary,
            "time": self.time,
            "urgency": self.urgency.to_string(),
        })
    }

    fn from_json(entry: &Value) -> Option<Self> {
        let string = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Some(Notification {
            id: entry.get("notificationId").and_then(Value::as_u64)? as u32,
            actions: Vec::new(),
            app_icon: string("appIcon"),
            app_name: string("appName"),
            body: string("body"),
            image: string("image"),
            summary: string("summary"),
            time: entry.get("time").and_then(Value::as_i64).unwrap_or(0),
            urgency: string("urgency").parse().unwrap_or(1),
            transient: false,
            popup: false,
            timeout: 0,
        })
    }
}

#[derive(Clone)]
pub struct Notifications {
    pub list: Rc<RefCell<Vec<Notification>>>,
    pub unread: Rc<Cell<u32>>,
    pub silent: Rc<Cell<bool>>,
    next_id: Rc<Cell<u32>>,
    inhibited: Rc<Cell<bool>>,
    connection: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
    timers: Rc<RefCell<HashMap<u32, glib::SourceId>>>,
}

impl Notifications {
    pub fn new(config: &crate::core::config::Config, session: Option<gio::DBusConnection>) -> Self {
        let stored = read_store();
        let highest = stored.iter().map(|entry| entry.id).max().unwrap_or(0);
        let notifications = Notifications {
            list: Rc::new(RefCell::new(stored)),
            unread: Rc::new(Cell::new(0)),
            silent: Rc::new(Cell::new(config.notifications_silent)),
            next_id: Rc::new(Cell::new(highest + 1)),
            inhibited: Rc::new(Cell::new(false)),
            connection: session,
            listeners: Rc::default(),
            timers: Rc::new(RefCell::new(HashMap::new())),
        };
        notifications.serve();
        notifications
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn set_inhibited(&self, inhibited: bool) {
        self.inhibited.set(inhibited);
        if inhibited {
            self.timeout_all();
            self.mark_all_read();
        }
    }

    pub fn mark_all_read(&self) {
        if self.unread.replace(0) != 0 {
            self.announce();
        }
    }

    pub fn set_silent(&self, silent: bool) {
        self.silent.set(silent);
        crate::core::config::store_flag("notifications", "silent", silent);
        self.announce();
    }

    pub fn groups(&self, popup_only: bool) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();
        for notification in self.list.borrow().iter() {
            if popup_only && !notification.popup {
                continue;
            }
            match groups
                .iter_mut()
                .find(|group| group.app_name == notification.app_name)
            {
                Some(group) => {
                    group.time = group.time.max(notification.time);
                    if group.app_icon.is_empty() {
                        group.app_icon = notification.app_icon.clone();
                    }
                    group.notifications.push(notification.clone());
                }
                None => groups.push(Group {
                    app_name: notification.app_name.clone(),
                    app_icon: notification.app_icon.clone(),
                    time: notification.time,
                    notifications: vec![notification.clone()],
                }),
            }
        }
        groups.sort_by_key(|group| std::cmp::Reverse(group.time));
        groups
    }

    pub fn hold(&self, ids: &[u32]) {
        for id in ids {
            self.stop_timer(*id);
        }
    }

    pub fn release(&self, ids: &[u32]) {
        for id in ids {
            let interval = self
                .list
                .borrow()
                .iter()
                .find(|entry| entry.id == *id && entry.popup)
                .map(|entry| entry.timeout)
                .unwrap_or(0);
            if interval != 0 {
                self.arm_timer(*id, interval);
            }
        }
    }

    pub fn dismiss(&self, id: u32) {
        self.discard(id, 2);
    }

    pub fn discard_all(&self) {
        let ids: Vec<u32> = self.list.borrow().iter().map(|entry| entry.id).collect();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.stop_timer(*id);
            let _ = std::fs::remove_file(image_path(*id));
        }
        self.list.borrow_mut().clear();
        write_store(&self.list.borrow());
        for id in &ids {
            self.closed(*id, 2);
        }
        self.announce();
    }

    pub fn invoke(&self, id: u32, key: &str) {
        if let Some(connection) = &self.connection {
            let _ = connection.emit_signal(
                None,
                PATH,
                NAME,
                "ActionInvoked",
                Some(&(id, key).to_variant()),
            );
        }
        self.discard(id, 2);
    }

    pub fn timeout_all(&self) {
        let ids: Vec<u32> = self
            .list
            .borrow()
            .iter()
            .filter(|entry| entry.popup)
            .map(|entry| entry.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.stop_timer(*id);
        }
        for notification in self.list.borrow_mut().iter_mut() {
            notification.popup = false;
        }
        self.announce();
    }

    fn expire(&self, id: u32) {
        self.stop_timer(id);
        let transient = self
            .list
            .borrow()
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.transient);
        match transient {
            Some(true) => self.discard(id, 1),
            Some(false) => {
                let mut changed = false;
                for notification in self.list.borrow_mut().iter_mut() {
                    if notification.id == id && notification.popup {
                        notification.popup = false;
                        changed = true;
                    }
                }
                if changed {
                    self.announce();
                }
            }
            None => {}
        }
    }

    fn stop_timer(&self, id: u32) {
        if let Some(source) = self.timers.borrow_mut().remove(&id) {
            source.remove();
        }
    }

    fn arm_timer(&self, id: u32, millis: i32) {
        self.stop_timer(id);
        let notifications = self.clone();
        let source =
            glib::timeout_add_local_once(Duration::from_millis(millis as u64), move || {
                notifications.timers.borrow_mut().remove(&id);
                notifications.expire(id);
            });
        self.timers.borrow_mut().insert(id, source);
    }

    fn discard(&self, id: u32, reason: u32) {
        self.stop_timer(id);
        let removed = {
            let mut list = self.list.borrow_mut();
            let before = list.len();
            list.retain(|notification| notification.id != id);
            list.len() != before
        };
        if !removed {
            return;
        }
        let _ = std::fs::remove_file(image_path(id));
        write_store(&self.list.borrow());
        self.closed(id, reason);
        self.announce();
    }

    fn closed(&self, id: u32, reason: u32) {
        let Some(connection) = &self.connection else {
            return;
        };
        let _ = connection.emit_signal(
            None,
            PATH,
            NAME,
            "NotificationClosed",
            Some(&(id, reason).to_variant()),
        );
    }

    fn announce(&self) {
        self.listeners.notify();
    }

    fn receive(self: &Rc<Self>, call: &Variant) -> u32 {
        let text = |at: usize| {
            call.child_value(at)
                .str()
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let replaces = call.child_value(1).get::<u32>().unwrap_or(0);
        let hints = call.child_value(6);
        let expire = call.child_value(7).get::<i32>().unwrap_or(-1);

        let mut labels = Vec::new();
        let listed: Vec<String> = call
            .child_value(5)
            .iter()
            .filter_map(|entry| entry.str().map(str::to_owned))
            .collect();
        for pair in listed.chunks(2) {
            if let [identifier, label] = pair {
                labels.push((identifier.clone(), label.clone()));
            }
        }

        let id = if replaces != 0 {
            replaces
        } else {
            let id = self.next_id.get();
            self.next_id.set(id.wrapping_add(1).max(1));
            id
        };

        let notification = Notification {
            id,
            actions: labels,
            app_icon: app_icon(text(2), &hints),
            app_name: text(0),
            body: text(4),
            image: image(id, &hints),
            summary: text(3),
            time: glib::real_time() / 1000,
            urgency: hint_byte(&hints, "urgency").unwrap_or(1),
            transient: hint_bool(&hints, "transient").unwrap_or(false),
            popup: !self.inhibited.get() && !self.silent.get(),
            timeout: match expire {
                0 => 0,
                positive if positive > 0 => positive,
                _ => {
                    config::value_i64("/notifications/timeout", config::NOTIFICATION_TIMEOUT) as i32
                }
            },
        };
        let showing = notification.popup;
        let interval = notification.timeout;

        {
            let mut list = self.list.borrow_mut();
            list.retain(|held| held.id != id);
            list.push(notification);
        }
        write_store(&self.list.borrow());

        if showing {
            self.unread.set(self.unread.get() + 1);
            if interval != 0 {
                self.arm_timer(id, interval);
            }
        }
        self.announce();
        id
    }

    fn serve(&self) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        let Some(interface) = node.lookup_interface(NAME) else {
            return;
        };

        let notifications = Rc::new(self.clone());
        let registration = connection
            .register_object(PATH, &interface)
            .method_call(move |_, _, _, _, method, call, invocation| {
                match method {
                    "GetCapabilities" => {
                        invocation.return_value(Some(&(CAPABILITIES.to_vec(),).to_variant()));
                    }
                    "Notify" => {
                        let id = notifications.receive(&call);
                        invocation.return_value(Some(&(id,).to_variant()));
                    }
                    "CloseNotification" => {
                        if let Some(id) = call.child_value(0).get::<u32>() {
                            notifications.discard(id, 3);
                        }
                        invocation.return_value(None);
                    }
                    "GetServerInformation" => {
                        invocation
                            .return_value(Some(&("proscenio", "fEst", "0.1", "1.2").to_variant()));
                    }
                    _ => invocation.return_value(None),
                };
            })
            .build();
        if registration.is_err() {
            return;
        }

        gio::bus_own_name_on_connection(
            &connection,
            NAME,
            gio::BusNameOwnerFlags::NONE,
            |_, _| {},
            |_, _| {},
        );
        std::mem::forget(registration);
    }
}

fn hint(hints: &Variant, key: &str) -> Option<Variant> {
    hints
        .iter()
        .find(|pair| pair.child_value(0).str() == Some(key))
        .and_then(|pair| pair.child_value(1).as_variant())
}

fn app_icon(given: String, hints: &Variant) -> String {
    if !given.is_empty() {
        return given;
    }
    hint_string(hints, "desktop-entry")
        .and_then(|id| desktop::find(&id))
        .map(|entry| entry.icon())
        .unwrap_or_default()
}

fn image(id: u32, hints: &Variant) -> String {
    let pixels = ["image-data", "image_data", "icon_data"]
        .into_iter()
        .find_map(|key| hint(hints, key));
    if let Some(saved) = pixels.and_then(|pixels| save_pixels(id, &pixels)) {
        return saved;
    }
    let Some(path) = hint_string(hints, "image-path").or_else(|| hint_string(hints, "image_path"))
    else {
        return String::new();
    };
    if !path.starts_with("file:") {
        return path;
    }
    gio::File::for_uri(&path)
        .path()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn save_pixels(id: u32, pixels: &Variant) -> Option<String> {
    let (width, height, rowstride, alpha, bits, _, data) =
        pixels.get::<(i32, i32, i32, bool, i32, i32, Vec<u8>)>()?;
    let row = width.checked_mul(if alpha { 4 } else { 3 })?;
    let size = rowstride.checked_mul(height - 1)?.checked_add(row)?;
    if bits != 8 || width < 1 || height < 1 || rowstride < row || data.len() < size as usize {
        return None;
    }
    let pixbuf = Pixbuf::from_bytes(
        &glib::Bytes::from_owned(data),
        Colorspace::Rgb,
        alpha,
        bits,
        width,
        height,
        rowstride,
    );
    let path = image_path(id);
    std::fs::create_dir_all(path.parent()?).ok()?;
    pixbuf.savev(&path, "png", &[]).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn image_path(id: u32) -> PathBuf {
    crate::core::paths::cache()
        .join("notifications")
        .join(format!("{id}.png"))
}

fn hint_string(hints: &Variant, key: &str) -> Option<String> {
    hint(hints, key)?.str().map(str::to_owned)
}

fn hint_bool(hints: &Variant, key: &str) -> Option<bool> {
    hint(hints, key)?.get::<bool>()
}

fn hint_byte(hints: &Variant, key: &str) -> Option<u8> {
    hint(hints, key)?.get::<u8>()
}

fn store_path() -> PathBuf {
    crate::core::paths::cache().join("notifications.json")
}

fn read_store() -> Vec<Notification> {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_array().cloned())
        .map(|entries| entries.iter().filter_map(Notification::from_json).collect())
        .unwrap_or_default()
}

fn write_store(list: &[Notification]) {
    let path = store_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let entries: Vec<Value> = list.iter().map(Notification::to_json).collect();
    if let Ok(text) = serde_json::to_string_pretty(&Value::Array(entries)) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_path_hints_resolve_like_quickshell() {
        let cases = [
            ("image-path", "file:///tmp/a%20b.png", "/tmp/a b.png"),
            ("image_path", "file:///tmp/cover.png", "/tmp/cover.png"),
            ("image-path", "/tmp/cover.png", "/tmp/cover.png"),
            ("image-path", "audio-x-generic", "audio-x-generic"),
        ];
        for (key, given, expected) in cases {
            let hints = HashMap::from([(key.to_owned(), given.to_variant())]).to_variant();
            assert_eq!(image(0, &hints), expected);
        }
        let empty = HashMap::<String, Variant>::new().to_variant();
        assert_eq!(image(0, &empty), "");
    }
}

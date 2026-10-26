pub mod images;
pub mod server;
mod store;

use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use self::images::{named_icon, remove_discarded_images};
use self::store::{read_store, write_store};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::{config, persistent};
use crate::services::media::deviceusage::DeviceUsage;

pub const QUIET_APPS: &str = "/notifications/quietApps";
pub const FORGOTTEN_APPS: &str = "/notifications/forgottenApps";
pub const HIDE_WHILE_SHARING: &str = "/notifications/hideWhileSharing";
const KNOWN_APPS: &str = "notificationApps";
const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub label: String,
    pub placeholder: String,
}

#[derive(Clone)]
pub struct Notification {
    pub id: u32,
    pub actions: Vec<(String, String)>,
    pub reply: Option<Reply>,
    pub app_icon: String,
    pub app_name: String,
    pub body: String,
    pub image: String,
    pub summary: String,
    pub time_millis: i64,
    pub urgency: u8,
    pub is_transient: bool,
    pub is_popup: bool,
    pub timeout_millis: i32,
}

pub struct Group {
    pub app_name: String,
    pub app_icon: String,
    pub time_millis: i64,
    pub notifications: Vec<Notification>,
}

impl Group {
    pub fn is_urgent(&self) -> bool {
        self.notifications.iter().any(|entry| entry.urgency == 2)
    }

    pub fn ids(&self) -> Vec<u32> {
        self.notifications.iter().map(|entry| entry.id).collect()
    }

    pub fn is_shown_like(&self, notifications: &[Notification]) -> bool {
        self.notifications.len() == notifications.len()
            && self
                .notifications
                .iter()
                .zip(notifications)
                .all(|(entry, other)| entry.is_shown_like(other))
    }
}

impl Notification {
    fn is_shown_like(&self, other: &Notification) -> bool {
        let Notification {
            id,
            actions,
            reply,
            app_icon,
            app_name,
            body,
            image,
            summary,
            time_millis,
            urgency,
            is_transient: _,
            is_popup: _,
            timeout_millis: _,
        } = self;
        (
            *id,
            actions,
            reply,
            app_icon,
            app_name,
            body,
            image,
            summary,
            *time_millis,
            *urgency,
        ) == (
            other.id,
            &other.actions,
            &other.reply,
            &other.app_icon,
            &other.app_name,
            &other.body,
            &other.image,
            &other.summary,
            other.time_millis,
            other.urgency,
        )
    }
}

#[derive(Clone)]
pub struct Notifications {
    pub list: Rc<RefCell<Vec<Notification>>>,
    pub unread_count: Rc<Cell<u32>>,
    pub is_silent: Rc<Cell<bool>>,
    unread_ids: Rc<RefCell<HashSet<u32>>>,
    next_id: Rc<Cell<u32>>,
    is_inhibited: Rc<Cell<bool>>,
    is_sharing: Rc<Cell<bool>>,
    connection: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
    timers: Rc<RefCell<HashMap<u32, glib::SourceId>>>,
    drafts: Rc<RefCell<HashMap<u32, String>>>,
    applied_image_requests: Rc<RefCell<HashMap<u32, u64>>>,
    next_image_request: Rc<Cell<u64>>,
}

impl Notifications {
    pub fn new(
        config: &crate::core::config::Config,
        session: Option<gio::DBusConnection>,
        device_usage: &DeviceUsage,
    ) -> Self {
        let list = read_store();
        let highest_id = list.iter().map(|entry| entry.id).max().unwrap_or(0);
        let notifications = Notifications {
            list: Rc::new(RefCell::new(list)),
            unread_count: Rc::new(Cell::new(0)),
            is_silent: Rc::new(Cell::new(config.notifications_silent)),
            unread_ids: Rc::default(),
            next_id: Rc::new(Cell::new(highest_id + 1)),
            is_inhibited: Rc::new(Cell::new(false)),
            is_sharing: Rc::new(Cell::new(!device_usage.usage().screens.is_empty())),
            connection: session,
            listeners: Rc::default(),
            timers: Rc::new(RefCell::new(HashMap::new())),
            drafts: Rc::new(RefCell::new(HashMap::new())),
            applied_image_requests: Rc::default(),
            next_image_request: Rc::new(Cell::new(glib::real_time().unsigned_abs())),
        };
        notifications.serve();
        let watcher = notifications.clone();
        let usage = device_usage.clone();
        device_usage
            .subscribe(move || watcher.set_sharing(!usage.usage().screens.is_empty()))
            .detach();
        notifications
    }

    fn set_sharing(&self, is_sharing: bool) {
        if self.is_sharing.replace(is_sharing) != is_sharing
            && is_sharing
            && self.is_hidden_by_sharing()
        {
            self.timeout_all();
        }
    }

    fn is_hidden_by_sharing(&self) -> bool {
        self.is_sharing.get() && config::value_bool(HIDE_WHILE_SHARING, true)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn set_inhibited(&self, is_inhibited: bool) {
        self.is_inhibited.set(is_inhibited);
        if is_inhibited {
            self.timeout_all();
            self.mark_all_read();
        }
    }

    pub fn mark_all_read(&self) {
        self.unread_ids.borrow_mut().clear();
        if self.unread_count.replace(0) != 0 {
            self.announce();
        }
    }

    fn mark_unread(&self, id: u32) {
        if self.unread_ids.borrow_mut().insert(id) {
            self.unread_count.set(self.unread_count.get() + 1);
        }
    }

    fn forget_unread(&self, id: u32) {
        if self.unread_ids.borrow_mut().remove(&id) {
            self.unread_count
                .set(self.unread_count.get().saturating_sub(1));
        }
    }

    pub fn set_silent(&self, is_silent: bool) {
        self.is_silent.set(is_silent);
        crate::core::config::store_flag("notifications", "silent", is_silent);
        self.announce();
    }

    pub fn groups(&self, is_popup_only: bool) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();
        for notification in self.list.borrow().iter() {
            if is_popup_only && !notification.is_popup {
                continue;
            }
            match groups
                .iter_mut()
                .find(|group| group.app_name == notification.app_name)
            {
                Some(group) => {
                    group.time_millis = group.time_millis.max(notification.time_millis);
                    if group.app_icon.is_empty() {
                        group.app_icon = notification.app_icon.clone();
                    }
                    group.notifications.push(notification.clone());
                }
                None => groups.push(Group {
                    app_name: notification.app_name.clone(),
                    app_icon: notification.app_icon.clone(),
                    time_millis: notification.time_millis,
                    notifications: vec![notification.clone()],
                }),
            }
        }
        groups.sort_by_key(|group| std::cmp::Reverse(group.time_millis));
        groups
    }

    pub fn hold(&self, ids: &[u32]) {
        for id in ids {
            self.stop_timer(*id);
        }
    }

    pub fn release(&self, ids: &[u32]) {
        for id in ids {
            if self.drafts.borrow().contains_key(id) {
                continue;
            }
            let timeout_millis = self
                .list
                .borrow()
                .iter()
                .find(|entry| entry.id == *id && entry.is_popup)
                .map(|entry| entry.timeout_millis)
                .unwrap_or(0);
            if timeout_millis != 0 {
                self.arm_timer(*id, timeout_millis);
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
            self.forget_state(*id);
        }
        let removed: Vec<Notification> = self.list.borrow_mut().drain(..).collect();
        write_store(&self.list.borrow());
        for notification in removed {
            remove_discarded_images(notification.id, &notification.image);
        }
        for id in &ids {
            self.emit_closed(*id, 2);
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

    pub fn draft(&self, id: u32) -> Option<String> {
        self.drafts.borrow().get(&id).cloned()
    }

    pub fn set_draft(&self, id: u32, text: &str) {
        self.stop_timer(id);
        self.drafts.borrow_mut().insert(id, text.to_owned());
    }

    pub fn drop_draft(&self, id: u32) {
        if self.drafts.borrow_mut().remove(&id).is_some() {
            self.release(&[id]);
        }
    }

    pub fn reply(&self, id: u32, text: &str) {
        if let Some(connection) = &self.connection {
            let _ = connection.emit_signal(
                None,
                PATH,
                NAME,
                "NotificationReplied",
                Some(&(id, text).to_variant()),
            );
        }
        self.discard(id, 2);
    }

    pub fn timeout_all(&self) {
        let popups: Vec<(u32, bool)> = self
            .list
            .borrow()
            .iter()
            .filter(|entry| entry.is_popup)
            .map(|entry| (entry.id, entry.is_transient))
            .collect();
        if popups.is_empty() {
            return;
        }
        for (id, _) in &popups {
            self.stop_timer(*id);
        }
        for notification in self.list.borrow_mut().iter_mut() {
            notification.is_popup = false;
        }
        for (id, is_transient) in popups {
            if is_transient {
                self.discard(id, 1);
            }
        }
        self.announce();
    }

    fn expire(&self, id: u32) {
        self.stop_timer(id);
        let is_transient = self
            .list
            .borrow()
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.is_transient);
        match is_transient {
            Some(true) => self.discard(id, 1),
            Some(false) => {
                let mut has_changed = false;
                for notification in self.list.borrow_mut().iter_mut() {
                    if notification.id == id && notification.is_popup {
                        notification.is_popup = false;
                        has_changed = true;
                    }
                }
                if has_changed {
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

    fn arm_timer(&self, id: u32, timeout_millis: i32) {
        self.stop_timer(id);
        let notifications = self.clone();
        let source =
            glib::timeout_add_local_once(Duration::from_millis(timeout_millis as u64), move || {
                notifications.timers.borrow_mut().remove(&id);
                notifications.expire(id);
            });
        self.timers.borrow_mut().insert(id, source);
    }

    fn forget_state(&self, id: u32) {
        self.stop_timer(id);
        self.drafts.borrow_mut().remove(&id);
        self.forget_unread(id);
        self.applied_image_requests.borrow_mut().remove(&id);
    }

    fn discard(&self, id: u32, reason: u32) {
        self.forget_state(id);
        let removed = {
            let mut list = self.list.borrow_mut();
            list.iter()
                .position(|notification| notification.id == id)
                .map(|index| list.remove(index))
        };
        let Some(removed) = removed else {
            return;
        };
        write_store(&self.list.borrow());
        remove_discarded_images(id, &removed.image);
        self.emit_closed(id, reason);
        self.announce();
    }

    fn emit_closed(&self, id: u32, reason: u32) {
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

    fn take_id(&self, replaces_id: u32) -> u32 {
        if replaces_id != 0 && self.list.borrow().iter().any(|held| held.id == replaces_id) {
            return replaces_id;
        }
        let id = self.next_id.get();
        self.next_id.set(id.wrapping_add(1).max(1));
        id
    }
}

pub fn is_app_listed(pointer: &str, app_name: &str) -> bool {
    config::value(pointer)
        .and_then(|value| value.as_array().cloned())
        .is_some_and(|names| names.iter().any(|name| name.as_str() == Some(app_name)))
}

pub fn known_apps() -> Vec<String> {
    persistent::read(&[KNOWN_APPS])
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .collect()
}

fn remember_app(app_name: &str) {
    if app_name.is_empty() {
        return;
    }
    let mut app_names = known_apps();
    if app_names.iter().any(|name| name == app_name) {
        return;
    }
    app_names.push(app_name.to_owned());
    app_names.sort_by_key(|name| name.to_lowercase());
    persistent::write(&[KNOWN_APPS], Value::from(app_names));
}

pub fn app_icon_name(app_name: &str) -> String {
    named_icon(app_name)
}

fn hint(hints: &Variant, key: &str) -> Option<Variant> {
    hints
        .iter()
        .find(|pair| pair.child_value(0).str() == Some(key))
        .and_then(|pair| pair.child_value(1).as_variant())
}

fn hint_string(hints: &Variant, key: &str) -> Option<String> {
    hint(hints, key)?.str().map(str::to_owned)
}

#[cfg(test)]
pub fn sample_group(reply_flags: &[bool]) -> Group {
    let notifications = reply_flags
        .iter()
        .enumerate()
        .map(|(index, has_reply)| Notification {
            id: index as u32 + 1,
            actions: Vec::new(),
            reply: has_reply.then(|| Reply {
                label: String::new(),
                placeholder: String::new(),
            }),
            app_icon: String::new(),
            app_name: "Chat".to_owned(),
            body: String::new(),
            image: String::new(),
            summary: String::new(),
            time_millis: 0,
            urgency: 1,
            is_transient: false,
            is_popup: true,
            timeout_millis: 0,
        })
        .collect();
    Group {
        app_name: "Chat".to_owned(),
        app_icon: String::new(),
        time_millis: 0,
        notifications,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_detached() -> Notifications {
        Notifications {
            list: Rc::default(),
            unread_count: Rc::default(),
            is_silent: Rc::default(),
            unread_ids: Rc::default(),
            next_id: Rc::new(Cell::new(1)),
            is_inhibited: Rc::default(),
            is_sharing: Rc::default(),
            connection: None,
            listeners: Rc::default(),
            timers: Rc::default(),
            drafts: Rc::default(),
            applied_image_requests: Rc::default(),
            next_image_request: Rc::default(),
        }
    }

    #[test]
    fn sharing_follows_the_screen_state_at_once() {
        let notifications = new_detached();
        notifications.set_sharing(true);
        assert!(notifications.is_sharing.get());
        notifications.set_sharing(false);
        assert!(!notifications.is_sharing.get());
    }

    #[test]
    fn a_replaced_notification_counts_once_until_it_is_read() {
        let notifications = new_detached();
        for _ in 0..50 {
            notifications.mark_unread(42);
        }
        notifications.mark_unread(7);
        assert_eq!(notifications.unread_count.get(), 2);
        notifications.mark_all_read();
        assert_eq!(notifications.unread_count.get(), 0);
        notifications.mark_unread(42);
        assert_eq!(notifications.unread_count.get(), 1);
    }

    #[test]
    fn a_group_is_shown_alike_only_while_its_visible_content_is_the_same() {
        let group = sample_group(&[false, true]);
        let mut notifications = group.notifications.clone();
        notifications[0].is_popup = false;
        notifications[1].timeout_millis = 5000;
        notifications[1].is_transient = true;
        assert!(group.is_shown_like(&notifications));

        let changes: [fn(&mut Notification); 9] = [
            |entry| entry.summary = "Replaced".to_owned(),
            |entry| entry.image = "/cover.png".to_owned(),
            |entry| entry.body = "New text".to_owned(),
            |entry| entry.actions = vec![("open".to_owned(), "Open".to_owned())],
            |entry| entry.reply = None,
            |entry| entry.app_icon = "chat".to_owned(),
            |entry| entry.time_millis = 1,
            |entry| entry.urgency = 2,
            |entry| entry.id = 9,
        ];
        for change in changes {
            let mut changed = group.notifications.clone();
            change(&mut changed[1]);
            assert!(!group.is_shown_like(&changed));
        }
        assert!(!group.is_shown_like(&group.notifications[..1]));
    }

    #[test]
    fn an_image_request_applies_only_when_newer_than_the_last_applied_one() {
        let notifications = new_detached();
        notifications
            .list
            .replace(sample_group(&[false]).notifications);
        let id = notifications.list.borrow()[0].id;
        notifications.list.borrow_mut()[0].image = "/old.png".to_owned();
        let image = || notifications.list.borrow()[0].image.clone();

        let older_request = notifications.take_image_request();
        let newer_request = notifications.take_image_request();
        assert_eq!(
            notifications.apply_image(id, newer_request, "/new.png".to_owned()),
            (true, Some("/old.png".to_owned()))
        );
        assert_eq!(image(), "/new.png");
        assert_eq!(
            notifications.apply_image(id, older_request, "/older.png".to_owned()),
            (false, Some("/older.png".to_owned()))
        );
        assert_eq!(image(), "/new.png");

        let same_request = notifications.take_image_request();
        assert_eq!(
            notifications.apply_image(id, same_request, "/new.png".to_owned()),
            (false, None)
        );
        let gone_request = notifications.take_image_request();
        assert_eq!(
            notifications.apply_image(99, gone_request, "/gone.png".to_owned()),
            (false, Some("/gone.png".to_owned()))
        );
    }

    #[test]
    fn only_a_held_notification_can_be_replaced() {
        let notifications = new_detached();
        notifications
            .list
            .replace(sample_group(&[false, false]).notifications);
        notifications.next_id.set(3);
        assert_eq!(notifications.take_id(2), 2);
        assert_eq!(notifications.take_id(0), 3);
        assert_eq!(notifications.take_id(7), 4);
        assert_eq!(notifications.take_id(4), 5);
    }

    #[test]
    fn a_removed_unread_notification_leaves_the_count() {
        let notifications = new_detached();
        notifications.mark_unread(42);
        notifications.mark_unread(7);
        notifications.dismiss(42);
        assert_eq!(notifications.unread_count.get(), 1);
        notifications.dismiss(42);
        assert_eq!(notifications.unread_count.get(), 1);
        notifications.mark_unread(42);
        assert_eq!(notifications.unread_count.get(), 2);
    }

    #[test]
    fn a_forgotten_notification_leaves_no_draft_unread_mark_or_image_request() {
        let notifications = new_detached();
        notifications.set_draft(42, "half a reply");
        notifications.mark_unread(42);
        notifications
            .applied_image_requests
            .borrow_mut()
            .insert(42, 3);
        notifications.forget_state(42);
        assert_eq!(notifications.draft(42), None);
        assert_eq!(notifications.unread_count.get(), 0);
        assert!(notifications.applied_image_requests.borrow().is_empty());
    }
}

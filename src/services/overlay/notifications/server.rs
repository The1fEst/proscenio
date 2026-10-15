use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::rc::Rc;

use super::images::resolve_app_icon;
use super::store::write_store;
use super::{
    FORGOTTEN_APPS, NAME, Notification, Notifications, PATH, QUIET_APPS, Reply, hint, hint_string,
    is_app_listed, remember_app,
};
use crate::core::config;

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
    <signal name='NotificationReplied'>
      <arg type='u' name='id'/>
      <arg type='s' name='text'/>
    </signal>
  </interface>
</node>";

pub const CAPABILITIES: &[&str] = &[
    "actions",
    "body",
    "body-hyperlinks",
    "body-images",
    "body-markup",
    "icon-static",
    "inline-reply",
    "persistence",
];

const REPLY_ACTION: &str = "inline-reply";

impl Notifications {
    fn receive(self: &Rc<Self>, call: &Variant) -> u32 {
        let text = |at: usize| {
            call.child_value(at)
                .str()
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let replaces_id = call.child_value(1).get::<u32>().unwrap_or(0);
        let hints = call.child_value(6);
        let expire_timeout_millis = call.child_value(7).get::<i32>().unwrap_or(-1);

        let mut labels = Vec::new();
        let action_strings: Vec<String> = call
            .child_value(5)
            .iter()
            .filter_map(|entry| entry.str().map(str::to_owned))
            .collect();
        for pair in action_strings.chunks(2) {
            if let [identifier, label] = pair {
                labels.push((identifier.clone(), label.clone()));
            }
        }

        let id = self.take_id(replaces_id);

        let app_name = text(0);
        remember_app(&app_name);
        let is_quiet = is_app_listed(QUIET_APPS, &app_name);
        let is_forgotten = is_app_listed(FORGOTTEN_APPS, &app_name);
        let reply = take_reply(&mut labels, &hints);
        let should_alert = !self.is_inhibited.get() && !self.is_silent.get() && !is_quiet;
        let mut notification = Notification {
            id,
            actions: labels,
            reply,
            app_icon: resolve_app_icon(text(2), &hints, &app_name),
            app_name,
            body: text(4),
            image: String::new(),
            summary: text(3),
            time_millis: glib::real_time() / 1000,
            urgency: hint_byte(&hints, "urgency").unwrap_or(1),
            is_transient: is_forgotten || hint_bool(&hints, "transient").unwrap_or(false),
            is_popup: should_alert && !self.is_hidden_by_sharing(),
            timeout_millis: resolve_timeout_millis(
                expire_timeout_millis,
                config::value_i64(
                    "/notifications/timeout",
                    config::NOTIFICATION_TIMEOUT_MILLIS,
                ),
            ),
        };
        let is_popup = notification.is_popup;
        let timeout_millis = notification.timeout_millis;
        let is_transient = notification.is_transient;
        if !is_popup && is_transient {
            self.discard(id, 1);
            return id;
        }
        let app_icon = notification.app_icon.clone();
        {
            let mut list = self.list.borrow_mut();
            if let Some(index) = list.iter().position(|held| held.id == id) {
                notification.image = list.remove(index).image;
            }
            list.push(notification);
        }
        write_store(&self.list.borrow());
        self.request_image(id, hints, app_icon);

        if should_alert && !is_transient {
            self.mark_unread(id);
        }
        if is_popup && timeout_millis != 0 {
            self.arm_timer(id, timeout_millis);
        }
        self.announce();
        id
    }

    pub(super) fn serve(&self) {
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

fn resolve_timeout_millis(expire_timeout_millis: i32, configured_millis: i64) -> i32 {
    match expire_timeout_millis {
        0 => 0,
        positive if positive > 0 => positive,
        _ if configured_millis <= 0 => 0,
        _ => {
            let (min_millis, max_millis) = config::NOTIFICATION_TIMEOUT_RANGE_MILLIS;
            configured_millis.clamp(min_millis, max_millis) as i32
        }
    }
}

fn take_reply(actions: &mut Vec<(String, String)>, hints: &Variant) -> Option<Reply> {
    let index = actions
        .iter()
        .position(|(identifier, _)| identifier == REPLY_ACTION)?;
    let (_, label) = actions.remove(index);
    Some(Reply {
        label,
        placeholder: hint_string(hints, "x-kde-reply-placeholder-text").unwrap_or_default(),
    })
}

fn hint_bool(hints: &Variant, key: &str) -> Option<bool> {
    hint(hints, key)?.get::<bool>()
}

fn hint_byte(hints: &Variant, key: &str) -> Option<u8> {
    hint(hints, key)?.get::<u8>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn an_inline_reply_action_becomes_the_reply_field() {
        let mut actions = vec![
            ("default".to_owned(), "Open".to_owned()),
            ("inline-reply".to_owned(), "Answer".to_owned()),
        ];
        let hints = HashMap::from([(
            "x-kde-reply-placeholder-text".to_owned(),
            "Message".to_variant(),
        )])
        .to_variant();
        assert_eq!(
            take_reply(&mut actions, &hints),
            Some(Reply {
                label: "Answer".to_owned(),
                placeholder: "Message".to_owned(),
            })
        );
        assert_eq!(actions, vec![("default".to_owned(), "Open".to_owned())]);

        let empty = HashMap::<String, Variant>::new().to_variant();
        assert_eq!(take_reply(&mut actions, &empty), None);
    }

    #[test]
    fn inline_reply_is_advertised_only_with_its_signal() {
        assert_eq!(
            CAPABILITIES.contains(&"inline-reply"),
            INTROSPECTION.contains("NotificationReplied")
        );
    }

    #[test]
    fn a_popup_timeout_comes_from_the_sender_or_the_clamped_config_value() {
        let cases = [
            (-1, 9000, 9000),
            (-2, 9000, 9000),
            (-1, 0, 0),
            (-1, -5000, 0),
            (-1, 500, 1000),
            (-1, 60001, 60000),
            (-1, 5_000_000_000, 60000),
            (0, 9000, 0),
            (3000, -5000, 3000),
        ];
        for (expire_timeout_millis, configured_millis, expected) in cases {
            assert_eq!(
                resolve_timeout_millis(expire_timeout_millis, configured_millis),
                expected,
                "{expire_timeout_millis} {configured_millis}"
            );
        }
    }
}

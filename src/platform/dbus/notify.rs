use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

#[derive(Clone, Copy, Default)]
pub enum Urgency {
    Low = 0,
    #[default]
    Normal = 1,
    Critical = 2,
}

#[derive(Default)]
pub struct Notification<'a> {
    pub app: &'a str,
    pub summary: &'a str,
    pub body: &'a str,
    pub icon: &'a str,
    pub category: &'a str,
    pub urgency: Urgency,
    pub is_transient: bool,
    pub actions: &'a [(&'a str, &'a str)],
}

fn arguments(notification: &Notification) -> Variant {
    let actions: Vec<&str> = notification
        .actions
        .iter()
        .flat_map(|(key, label)| [*key, *label])
        .collect();
    let mut hints: HashMap<&str, Variant> = HashMap::new();
    hints.insert("urgency", (notification.urgency as u8).to_variant());
    if notification.is_transient {
        hints.insert("transient", true.to_variant());
    }
    if !notification.category.is_empty() {
        hints.insert("category", notification.category.to_variant());
    }
    (
        notification.app,
        0u32,
        notification.icon,
        notification.summary,
        notification.body,
        actions,
        hints,
        -1i32,
    )
        .to_variant()
}

pub fn send(notification: &Notification) {
    send_then(notification, |_| {});
}

pub fn send_then(notification: &Notification, on_sent: impl FnOnce(u32) + 'static) {
    let parameters = arguments(notification);
    glib::spawn_future_local(async move {
        let Ok(bus) = gio::bus_get_future(gio::BusType::Session).await else {
            return;
        };
        let reply = bus
            .call_future(
                Some(NAME),
                PATH,
                NAME,
                "Notify",
                Some(&parameters),
                Some(VariantTy::new("(u)").unwrap()),
                gio::DBusCallFlags::NONE,
                -1,
            )
            .await;
        if let Some((id,)) = reply.ok().and_then(|reply| reply.get::<(u32,)>()) {
            on_sent(id);
        }
    });
}

pub fn close(id: u32) {
    glib::spawn_future_local(async move {
        let Ok(bus) = gio::bus_get_future(gio::BusType::Session).await else {
            return;
        };
        let _ = bus
            .call_future(
                Some(NAME),
                PATH,
                NAME,
                "CloseNotification",
                Some(&(id,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                -1,
            )
            .await;
    });
}

pub fn send_blocking(notification: &Notification) -> Option<String> {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| send_and_wait(&context, notification))
        .ok()
        .flatten()
}

fn send_and_wait(context: &glib::MainContext, notification: &Notification) -> Option<String> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let main_loop = glib::MainLoop::new(Some(context), false);
    let id = Rc::new(RefCell::new(None::<u32>));
    let action_key = Rc::new(RefCell::new(None::<String>));
    let _subscription = bus.subscribe_to_signal(
        Some(NAME),
        Some(NAME),
        None,
        Some(PATH),
        None,
        gio::DBusSignalFlags::NONE,
        {
            let main_loop = main_loop.clone();
            let id = id.clone();
            let action_key = action_key.clone();
            move |signal| {
                let Some(expected_id) = *id.borrow() else {
                    return;
                };
                match signal.signal_name {
                    "ActionInvoked" => {
                        if let Some((notification_id, key)) =
                            signal.parameters.get::<(u32, String)>()
                            && notification_id == expected_id
                        {
                            action_key.replace(Some(key));
                            main_loop.quit();
                        }
                    }
                    "NotificationClosed" => {
                        if let Some((notification_id, _)) = signal.parameters.get::<(u32, u32)>()
                            && notification_id == expected_id
                        {
                            main_loop.quit();
                        }
                    }
                    _ => {}
                }
            }
        },
    );
    let reply = bus
        .call_sync(
            Some(NAME),
            PATH,
            NAME,
            "Notify",
            Some(&arguments(notification)),
            Some(VariantTy::new("(u)").ok()?),
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .ok()?;
    id.replace(reply.get::<(u32,)>().map(|(id,)| id));
    if notification.actions.is_empty() {
        return None;
    }
    main_loop.run();
    action_key.take()
}

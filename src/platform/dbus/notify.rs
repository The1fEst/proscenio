use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const BUS_NAME: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";

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
    pub image_path: &'a str,
    pub image: Option<&'a Pixbuf>,
    pub category: &'a str,
    pub urgency: Urgency,
    pub is_transient: bool,
    pub actions: &'a [(&'a str, &'a str)],
    pub replaces_id: u32,
}

fn encode_pixels(image: &Pixbuf) -> Variant {
    Variant::tuple_from_iter([
        image.width().to_variant(),
        image.height().to_variant(),
        image.rowstride().to_variant(),
        image.has_alpha().to_variant(),
        image.bits_per_sample().to_variant(),
        image.n_channels().to_variant(),
        Variant::array_from_fixed_array(&image.read_pixel_bytes()),
    ])
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
    if !notification.image_path.is_empty() {
        hints.insert("image-path", notification.image_path.to_variant());
    }
    if let Some(image) = notification.image {
        hints.insert("image-data", encode_pixels(image));
    }
    (
        notification.app,
        notification.replaces_id,
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
    let _owner_subscription = bus.subscribe_to_signal(
        Some(BUS_NAME),
        Some(BUS_NAME),
        Some("NameOwnerChanged"),
        Some(BUS_PATH),
        Some(NAME),
        gio::DBusSignalFlags::NONE,
        {
            let main_loop = main_loop.clone();
            move |signal| {
                if is_owner_lost(signal.parameters) {
                    main_loop.quit();
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

fn is_owner_lost(parameters: &Variant) -> bool {
    parameters
        .get::<(String, String, String)>()
        .is_some_and(|(_, _, new_owner)| new_owner.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::gdk_pixbuf::Colorspace;

    #[test]
    fn image_hints_carry_the_pixels_and_the_path() {
        let image = Pixbuf::new(Colorspace::Rgb, true, 8, 2, 3).unwrap();
        let parameters = arguments(&Notification {
            image_path: "/pictures/a.png",
            image: Some(&image),
            ..Default::default()
        });
        let hints = glib::VariantDict::new(Some(&parameters.child_value(6)));
        let pixel_hint = hints.lookup_value("image-data", None).unwrap();
        assert_eq!(pixel_hint.type_().as_str(), "(iiibiiay)");
        assert_eq!(pixel_hint.child_value(0).get::<i32>(), Some(2));
        assert_eq!(pixel_hint.child_value(1).get::<i32>(), Some(3));
        assert_eq!(
            hints
                .lookup_value("image-path", None)
                .and_then(|path| path.get::<String>()),
            Some("/pictures/a.png".to_owned())
        );
    }

    #[test]
    fn only_a_name_without_a_new_owner_ends_the_wait() {
        let cases = [
            ((NAME, ":1.5", ""), true),
            ((NAME, "", ""), true),
            ((NAME, "", ":1.5"), false),
            ((NAME, ":1.5", ":1.6"), false),
        ];
        for (parameters, expected) in cases {
            assert_eq!(is_owner_lost(&parameters.to_variant()), expected);
        }
        assert!(!is_owner_lost(&(1u32, 2u32).to_variant()));
    }

    #[test]
    fn a_notification_without_an_image_has_no_image_hints() {
        let parameters = arguments(&Notification::default());
        let hints = glib::VariantDict::new(Some(&parameters.child_value(6)));
        assert!(hints.lookup_value("image-data", None).is_none());
        assert!(hints.lookup_value("image-path", None).is_none());
    }
}

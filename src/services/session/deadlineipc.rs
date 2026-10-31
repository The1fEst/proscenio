use gtk4::glib;

use crate::platform::dbus::ipc::Ipc;
use crate::services::Services;
use crate::services::session::deadline::{Hold, Preset};

fn resolve_now(preset: Option<Preset>) -> Option<Hold> {
    preset?.resolve(&glib::DateTime::now_local().ok()?)
}

pub fn register(ipc: &Ipc, services: &Services) {
    let notifications = &services.notifications;
    ipc.add("notifications", "toggleDoNotDisturb", {
        let notifications = notifications.clone();
        move || notifications.set_silent(!notifications.is_silent.get())
    });
    ipc.add("notifications", "enableDoNotDisturb", {
        let notifications = notifications.clone();
        move || notifications.silence(Hold::Indefinite)
    });
    ipc.add("notifications", "disableDoNotDisturb", {
        let notifications = notifications.clone();
        move || notifications.set_silent(false)
    });
    ipc.add_with(
        "notifications",
        "enableDoNotDisturbFor",
        &["minutes: string"],
        {
            let notifications = notifications.clone();
            move |arguments| {
                if let Some(hold) = resolve_now(Preset::from_minutes_text(&arguments[0])) {
                    notifications.silence(hold);
                }
            }
        },
    );
    ipc.add_with(
        "notifications",
        "enableDoNotDisturbUntil",
        &["time: string"],
        {
            let notifications = notifications.clone();
            move |arguments| {
                if let Some(hold) = resolve_now(Preset::from_clock_text(&arguments[0])) {
                    notifications.silence(hold);
                }
            }
        },
    );

    let session = &services.session;
    ipc.add("inhibit", "toggle", {
        let session = session.clone();
        move || session.toggle_keep_awake()
    });
    ipc.add("inhibit", "enable", {
        let session = session.clone();
        move || session.keep_awake(Hold::Indefinite)
    });
    ipc.add("inhibit", "disable", {
        let session = session.clone();
        move || session.release_keep_awake()
    });
    ipc.add_with("inhibit", "enableFor", &["minutes: string"], {
        let session = session.clone();
        move |arguments| {
            if let Some(hold) = resolve_now(Preset::from_minutes_text(&arguments[0])) {
                session.keep_awake(hold);
            }
        }
    });
}

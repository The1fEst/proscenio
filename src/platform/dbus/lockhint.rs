use gtk4::gio;
use gtk4::glib::VariantTy;
use gtk4::prelude::*;

const LOGIN: &str = "org.freedesktop.login1";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
const SESSION: &str = "org.freedesktop.login1.Session";
const TIMEOUT_MILLIS: i32 = 2000;

pub fn is_set() -> bool {
    let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) else {
        return false;
    };
    system
        .call_sync(
            Some(LOGIN),
            SESSION_PATH,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&(SESSION, "LockedHint").to_variant()),
            Some(VariantTy::new("(v)").unwrap()),
            gio::DBusCallFlags::NONE,
            TIMEOUT_MILLIS,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|reply| reply.child_value(0).as_variant())
        .and_then(|value| value.get::<bool>())
        .unwrap_or(false)
}

pub fn set(is_locked: bool) {
    let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) else {
        return;
    };
    system.call(
        Some(LOGIN),
        SESSION_PATH,
        SESSION,
        "SetLockedHint",
        Some(&(is_locked,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        TIMEOUT_MILLIS,
        gio::Cancellable::NONE,
        move |result| {
            if let Err(error) = result {
                eprintln!("lock: logind did not take the locked hint {is_locked}: {error}");
            }
        },
    );
}

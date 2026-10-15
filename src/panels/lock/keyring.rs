use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::platform::security::pam;

const SECRETS: &str = "org.freedesktop.secrets";
const SECRETS_PATH: &str = "/org/freedesktop/secrets";
const SECRET_SERVICE: &str = "org.freedesktop.Secret.Service";
const KEYRING_INTERNAL: &str = "org.gnome.keyring.InternalUnsupportedGuiltRiddenInterface";
const LOGIN_KEYRING: &str = "/org/freedesktop/secrets/collection/login";

pub(super) fn unlock_keyring(password: String) {
    let mut secret = password.into_bytes();
    if let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        unlock_login_keyring(&bus, &secret);
    }
    pam::wipe(&mut secret);
}

fn unlock_login_keyring(bus: &gio::DBusConnection, secret: &[u8]) -> Option<()> {
    let call = |path: &str, interface: &str, method: &str, arguments: glib::Variant| {
        bus.call_sync(
            Some(SECRETS),
            path,
            interface,
            method,
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .ok()
    };
    let lock_state = call(
        LOGIN_KEYRING,
        "org.freedesktop.DBus.Properties",
        "Get",
        ("org.freedesktop.Secret.Collection", "Locked").to_variant(),
    )
    .and_then(|reply| reply.child_value(0).as_variant())
    .and_then(|value| value.get::<bool>());
    if lock_state == Some(false) {
        return Some(());
    }
    let session = call(
        SECRETS_PATH,
        SECRET_SERVICE,
        "OpenSession",
        ("plain", "".to_variant()).to_variant(),
    )?
    .child_value(1);
    let master = glib::Variant::tuple_from_iter([
        session.clone(),
        glib::Variant::array_from_fixed_array::<u8>(&[]),
        glib::Variant::array_from_fixed_array(secret),
        "text/plain".to_variant(),
    ]);
    if lock_state.is_some() {
        let login = glib::variant::ObjectPath::try_from(LOGIN_KEYRING.to_owned()).ok()?;
        call(
            SECRETS_PATH,
            KEYRING_INTERNAL,
            "UnlockWithMasterPassword",
            glib::Variant::tuple_from_iter([login.to_variant(), master]),
        );
    } else {
        let properties: std::collections::HashMap<&str, glib::Variant> = [(
            "org.freedesktop.Secret.Collection.Label",
            "login".to_variant(),
        )]
        .into();
        let collection_path = call(
            SECRETS_PATH,
            KEYRING_INTERNAL,
            "CreateWithMasterPassword",
            glib::Variant::tuple_from_iter([properties.to_variant(), master]),
        )
        .map(|reply| reply.child_value(0));
        call(
            LOGIN_KEYRING,
            "org.freedesktop.DBus.Properties",
            "Set",
            (
                "org.freedesktop.Secret.Collection",
                "Label",
                "Login".to_variant(),
            )
                .to_variant(),
        );
        let default_collection_path = call(
            SECRETS_PATH,
            SECRET_SERVICE,
            "ReadAlias",
            ("default",).to_variant(),
        )
        .and_then(|reply| reply.child_value(0).str().map(str::to_owned));
        if let Some(collection_path) = collection_path
            && default_collection_path.as_deref() == Some("/")
        {
            call(
                SECRETS_PATH,
                SECRET_SERVICE,
                "SetAlias",
                glib::Variant::tuple_from_iter(["default".to_variant(), collection_path]),
            );
        }
    }
    call(
        session.str()?,
        "org.freedesktop.Secret.Session",
        "Close",
        ().to_variant(),
    );
    Some(())
}

use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy};
use gtk4::prelude::*;

pub const WAIT_FOR_PASSWORD: i32 = i32::MAX;

pub async fn property(
    connection: &gio::DBusConnection,
    bus: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Option<Variant> {
    let reply = connection
        .call_future(
            Some(bus),
            path,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&(interface, name).to_variant()),
            Some(VariantTy::new("(v)").unwrap()),
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
        .ok()?;
    reply.child_value(0).as_variant()
}

pub async fn string_property(
    connection: &gio::DBusConnection,
    bus: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Option<String> {
    property(connection, bus, path, interface, name)
        .await?
        .str()
        .map(str::to_owned)
}

pub async fn bool_property(
    connection: &gio::DBusConnection,
    bus: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Option<bool> {
    property(connection, bus, path, interface, name)
        .await?
        .get::<bool>()
}

pub async fn u32_property(
    connection: &gio::DBusConnection,
    bus: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Option<u32> {
    property(connection, bus, path, interface, name)
        .await?
        .get::<u32>()
}

pub async fn path_property(
    connection: &gio::DBusConnection,
    bus: &str,
    path: &str,
    interface: &str,
    name: &str,
) -> Option<String> {
    let value = property(connection, bus, path, interface, name).await?;
    value
        .get::<glib::variant::ObjectPath>()
        .map(|object| object.as_str().to_owned())
}

pub async fn managed_objects(
    connection: &gio::DBusConnection,
    bus: &str,
    root: &str,
) -> Option<Variant> {
    connection
        .call_future(
            Some(bus),
            root,
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
            None,
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
        .ok()
}

pub fn on_properties_changed(
    connection: &gio::DBusConnection,
    bus: &str,
    handler: impl Fn() + 'static,
) -> gio::SignalSubscription {
    connection.subscribe_to_signal(
        Some(bus),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        None,
        None,
        gio::DBusSignalFlags::NONE,
        move |_| handler(),
    )
}

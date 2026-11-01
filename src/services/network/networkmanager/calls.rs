use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy, variant::ObjectPath};
use gtk4::prelude::*;

use super::model::{BUS, DEVICE, ROOT};
use crate::platform::dbus::NO_TIMEOUT;
use crate::services::network::wifi::remote_message;

pub const NO_OBJECT: &str = "/";
const CALL_TIMEOUT_MILLIS: i32 = 120_000;

pub async fn call(
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<Variant>,
    reply: &str,
    is_interactive: bool,
) -> Result<Variant, String> {
    call_raw(path, interface, method, arguments, reply, is_interactive)
        .await
        .map_err(|error| remote_message(&error))
}

pub async fn call_raw(
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<Variant>,
    reply: &str,
    is_interactive: bool,
) -> Result<Variant, glib::Error> {
    let system = gio::bus_get_future(gio::BusType::System).await?;
    let (flags, timeout) = if is_interactive {
        (
            gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
            NO_TIMEOUT,
        )
    } else {
        (gio::DBusCallFlags::NONE, CALL_TIMEOUT_MILLIS)
    };
    system
        .call_future(
            Some(BUS),
            path,
            interface,
            method,
            arguments.as_ref(),
            VariantTy::new(reply).ok(),
            flags,
            timeout,
        )
        .await
}

fn encode_object_path(path: &str) -> Result<Variant, String> {
    ObjectPath::try_from(path.to_owned())
        .map(|path| path.to_variant())
        .map_err(|_| format!("{path} is not an object path"))
}

pub async fn set_wireless_enabled(is_enabled: bool) -> Result<(), String> {
    let arguments = (BUS, "WirelessEnabled", is_enabled.to_variant()).to_variant();
    call(
        ROOT,
        "org.freedesktop.DBus.Properties",
        "Set",
        Some(arguments),
        "()",
        true,
    )
    .await?;
    Ok(())
}

pub async fn activate_connection(
    connection: &str,
    device: &str,
    specific_object: &str,
) -> Result<String, String> {
    let arguments = Variant::tuple_from_iter([
        encode_object_path(connection)?,
        encode_object_path(device)?,
        encode_object_path(specific_object)?,
    ]);
    let reply = call(
        ROOT,
        BUS,
        "ActivateConnection",
        Some(arguments),
        "(o)",
        true,
    )
    .await?;
    Ok(reply
        .child_value(0)
        .get::<ObjectPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_default())
}

pub async fn deactivate_connection(active_connection: &str) -> Result<(), String> {
    let arguments = Variant::tuple_from_iter([encode_object_path(active_connection)?]);
    call(
        ROOT,
        BUS,
        "DeactivateConnection",
        Some(arguments),
        "()",
        true,
    )
    .await?;
    Ok(())
}

pub async fn disconnect_device(device: &str) -> Result<(), String> {
    call(device, DEVICE, "Disconnect", None, "()", true).await?;
    Ok(())
}

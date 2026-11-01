use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy, variant::ObjectPath};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::Duration;

use super::model::{ACTIVE_CONNECTION, BUS, DEVICE, ROOT, SETTINGS_CONNECTION, WIRELESS_DEVICE};
use crate::platform::dbus::{self, NO_TIMEOUT};
use crate::services::network::wifi::remote_message;

pub const NO_OBJECT: &str = "/";
const CALL_TIMEOUT_MILLIS: i32 = 120_000;
const CONNECTION_ACTIVATED_STATE: u32 = 2;
const CONNECTION_DEACTIVATED_STATE: u32 = 4;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(90);

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

pub async fn add_and_activate_connection(
    settings: Variant,
    device: &str,
    specific_object: &str,
) -> Result<(String, String), String> {
    let arguments = Variant::tuple_from_iter([
        settings,
        encode_object_path(device)?,
        encode_object_path(specific_object)?,
    ]);
    let reply = call(
        ROOT,
        BUS,
        "AddAndActivateConnection",
        Some(arguments),
        "(oo)",
        true,
    )
    .await?;
    let read_path = |index| {
        reply
            .child_value(index)
            .get::<ObjectPath>()
            .map(|path| path.as_str().to_owned())
            .unwrap_or_default()
    };
    Ok((read_path(0), read_path(1)))
}

pub async fn delete_connection(connection: &str) -> Result<(), String> {
    call(connection, SETTINGS_CONNECTION, "Delete", None, "()", true).await?;
    Ok(())
}

pub async fn request_scan(device: &str) -> Result<(), String> {
    let arguments = (HashMap::<String, Variant>::new(),).to_variant();
    call(
        device,
        WIRELESS_DEVICE,
        "RequestScan",
        Some(arguments),
        "()",
        false,
    )
    .await?;
    Ok(())
}

#[derive(Default)]
struct Latch {
    value: Cell<Option<bool>>,
    waker: RefCell<Option<Waker>>,
}

impl Latch {
    fn set(&self, value: bool) {
        if self.value.get().is_some() {
            return;
        }
        self.value.set(Some(value));
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    async fn wait(&self) -> bool {
        std::future::poll_fn(|context| match self.value.get() {
            Some(value) => Poll::Ready(value),
            None => {
                self.waker.replace(Some(context.waker().clone()));
                Poll::Pending
            }
        })
        .await
    }
}

fn read_activation(state: u32) -> Option<bool> {
    match state {
        CONNECTION_ACTIVATED_STATE => Some(true),
        CONNECTION_DEACTIVATED_STATE => Some(false),
        _ => None,
    }
}

pub async fn wait_for_activation(active_connection: &str) -> bool {
    if active_connection.is_empty() {
        return false;
    }
    let Ok(system) = gio::bus_get_future(gio::BusType::System).await else {
        return false;
    };
    let latch = Rc::new(Latch::default());
    let _subscription = system.subscribe_to_signal(
        Some(BUS),
        Some(ACTIVE_CONNECTION),
        Some("StateChanged"),
        Some(active_connection),
        None,
        gio::DBusSignalFlags::NONE,
        {
            let latch = Rc::downgrade(&latch);
            move |signal| {
                let activation = signal
                    .parameters
                    .get::<(u32, u32)>()
                    .and_then(|(state, _)| read_activation(state));
                if let (Some(latch), Some(activation)) = (latch.upgrade(), activation) {
                    latch.set(activation);
                }
            }
        },
    );
    let state =
        dbus::u32_property(&system, BUS, active_connection, ACTIVE_CONNECTION, "State").await;
    match state {
        None => return false,
        Some(state) => {
            if let Some(activation) = read_activation(state) {
                latch.set(activation);
            }
        }
    }
    glib::future_with_timeout(ACTIVATION_TIMEOUT, latch.wait())
        .await
        .unwrap_or(false)
}

use gtk4::gio;
use gtk4::glib::{self, VariantTy};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::dbus;

const BUS: &str = "org.freedesktop.GeoClue2";
const MANAGER: &str = "/org/freedesktop/GeoClue2/Manager";
const MANAGER_INTERFACE: &str = "org.freedesktop.GeoClue2.Manager";
const CLIENT: &str = "org.freedesktop.GeoClue2.Client";
const LOCATION: &str = "org.freedesktop.GeoClue2.Location";
const DESKTOP_ID: &str = "proscenio";
const ACCURACY_EXACT: u32 = 8;
const TIMEOUT: i32 = 5000;

pub type Held = Rc<RefCell<Option<gio::SignalSubscription>>>;

pub fn follow(
    system: gio::DBusConnection,
    held: Held,
    found: impl Fn(f64, f64) + 'static,
    failed: impl FnOnce() + 'static,
) {
    glib::spawn_future_local(async move {
        if start(&system, &held, Rc::new(found)).await.is_none() {
            held.take();
            failed();
        }
    });
}

async fn start(
    system: &gio::DBusConnection,
    held: &Held,
    found: Rc<dyn Fn(f64, f64)>,
) -> Option<()> {
    let reply = system
        .call_future(
            Some(BUS),
            MANAGER,
            MANAGER_INTERFACE,
            "GetClient",
            None,
            Some(VariantTy::new("(o)").ok()?),
            gio::DBusCallFlags::NONE,
            TIMEOUT,
        )
        .await
        .ok()?;
    let client = reply.child_value(0).str()?.to_owned();
    set(system, &client, "DesktopId", DESKTOP_ID.to_variant()).await?;
    set(
        system,
        &client,
        "RequestedAccuracyLevel",
        ACCURACY_EXACT.to_variant(),
    )
    .await?;

    let connection = system.clone();
    held.replace(Some(system.subscribe_to_signal(
        Some(BUS),
        Some(CLIENT),
        Some("LocationUpdated"),
        Some(&client),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(path) = signal.parameters.child_value(1).str().map(str::to_owned) else {
                return;
            };
            let connection = connection.clone();
            let found = found.clone();
            glib::spawn_future_local(async move {
                let coordinate = |name: &'static str| {
                    let connection = connection.clone();
                    let path = path.clone();
                    async move {
                        dbus::property(&connection, BUS, &path, LOCATION, name)
                            .await?
                            .get::<f64>()
                    }
                };
                if let (Some(latitude), Some(longitude)) =
                    (coordinate("Latitude").await, coordinate("Longitude").await)
                {
                    found(latitude, longitude);
                }
            });
        },
    )));

    system
        .call_future(
            Some(BUS),
            &client,
            CLIENT,
            "Start",
            None,
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT,
        )
        .await
        .ok()?;
    Some(())
}

async fn set(
    system: &gio::DBusConnection,
    client: &str,
    name: &str,
    value: glib::Variant,
) -> Option<()> {
    system
        .call_future(
            Some(BUS),
            client,
            "org.freedesktop.DBus.Properties",
            "Set",
            Some(&(CLIENT, name, value).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT,
        )
        .await
        .ok()
        .map(|_| ())
}

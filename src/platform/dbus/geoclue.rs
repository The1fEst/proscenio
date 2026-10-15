use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::platform::dbus;

const BUS: &str = "org.freedesktop.GeoClue2";
const MANAGER: &str = "/org/freedesktop/GeoClue2/Manager";
const MANAGER_INTERFACE: &str = "org.freedesktop.GeoClue2.Manager";
const CLIENT: &str = "org.freedesktop.GeoClue2.Client";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const LOCATION: &str = "org.freedesktop.GeoClue2.Location";
const DESKTOP_ID: &str = "proscenio";
const ACCURACY_EXACT: u32 = 8;
const TIMEOUT_MILLIS: i32 = 5000;
const START_TIMEOUT_MILLIS: i32 = 120_000;

pub struct LocationClient {
    pub client_path: String,
    _subscription: gio::SignalSubscription,
}

pub type LocationClientSlot = Rc<RefCell<Option<LocationClient>>>;
pub type WeakLocationClientSlot = Weak<RefCell<Option<LocationClient>>>;

pub fn release(system: gio::DBusConnection, client_path: String) {
    glib::spawn_future_local(async move {
        let _ = call(&system, &client_path, CLIENT, "Stop", None).await;
        let Ok(object_path) = glib::variant::ObjectPath::try_from(client_path) else {
            return;
        };
        let parameters = (object_path,).to_variant();
        let _ = call(
            &system,
            MANAGER,
            MANAGER_INTERFACE,
            "DeleteClient",
            Some(parameters),
        )
        .await;
    });
}

pub fn follow(
    system: gio::DBusConnection,
    client_slot: LocationClientSlot,
    on_found: impl Fn(f64, f64) + 'static,
    on_failed: impl FnOnce() + 'static,
) {
    glib::spawn_future_local(async move {
        let Some(client_path) = create_client(&system).await else {
            if Rc::strong_count(&client_slot) > 1 {
                on_failed();
            }
            return;
        };
        if Rc::strong_count(&client_slot) == 1 {
            release(system, client_path);
            return;
        }
        client_slot.replace(Some(LocationClient {
            client_path: client_path.clone(),
            _subscription: subscribe(&system, &client_path, Rc::new(on_found)),
        }));
        let is_started = configure(&system, &client_path).await.is_some()
            && call_with_timeout(
                &system,
                &client_path,
                CLIENT,
                "Start",
                None,
                START_TIMEOUT_MILLIS,
            )
            .await
            .is_some();
        let is_owned = Rc::strong_count(&client_slot) > 1;
        if is_started && is_owned {
            return;
        }
        client_slot.take();
        release(system, client_path);
        if !is_started && is_owned {
            on_failed();
        }
    });
}

async fn create_client(system: &gio::DBusConnection) -> Option<String> {
    let reply = call(system, MANAGER, MANAGER_INTERFACE, "CreateClient", None).await?;
    Some(reply.try_child_value(0)?.str()?.to_owned())
}

async fn configure(system: &gio::DBusConnection, client_path: &str) -> Option<()> {
    let set = |name: &str, value: glib::Variant| {
        let parameters = (CLIENT, name, value).to_variant();
        call(system, client_path, PROPERTIES, "Set", Some(parameters))
    };
    set("DesktopId", DESKTOP_ID.to_variant()).await?;
    set("RequestedAccuracyLevel", ACCURACY_EXACT.to_variant())
        .await
        .map(|_| ())
}

fn subscribe(
    system: &gio::DBusConnection,
    client_path: &str,
    on_found: Rc<dyn Fn(f64, f64)>,
) -> gio::SignalSubscription {
    let connection = system.clone();
    system.subscribe_to_signal(
        Some(BUS),
        Some(CLIENT),
        Some("LocationUpdated"),
        Some(client_path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(path) = signal.parameters.child_value(1).str().map(str::to_owned) else {
                return;
            };
            let connection = connection.clone();
            let on_found = on_found.clone();
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
                    on_found(latitude, longitude);
                }
            });
        },
    )
}

async fn call(
    system: &gio::DBusConnection,
    object_path: &str,
    interface: &str,
    method: &str,
    parameters: Option<glib::Variant>,
) -> Option<glib::Variant> {
    call_with_timeout(
        system,
        object_path,
        interface,
        method,
        parameters,
        TIMEOUT_MILLIS,
    )
    .await
}

async fn call_with_timeout(
    system: &gio::DBusConnection,
    object_path: &str,
    interface: &str,
    method: &str,
    parameters: Option<glib::Variant>,
    timeout_millis: i32,
) -> Option<glib::Variant> {
    system
        .call_future(
            Some(BUS),
            object_path,
            interface,
            method,
            parameters.as_ref(),
            None,
            gio::DBusCallFlags::NONE,
            timeout_millis,
        )
        .await
        .ok()
}

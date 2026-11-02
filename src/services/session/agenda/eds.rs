use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy};
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;

use super::civil::format_date_time;
use super::sources::{self, Source, Target};

const REGISTRY_BUS: &str = "org.gnome.evolution.dataserver.Sources5";
const REGISTRY_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
const FACTORY_BUS: &str = "org.gnome.evolution.dataserver.Calendar8";
const FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
const FACTORY_INTERFACE: &str = "org.gnome.evolution.dataserver.CalendarFactory";
const CALENDAR_INTERFACE: &str = "org.gnome.evolution.dataserver.Calendar";
const VIEW_INTERFACE: &str = "org.gnome.evolution.dataserver.CalendarView";
const CALL_TIMEOUT_MILLIS: i32 = 10_000;
const QUERY_TIMEOUT_MILLIS: i32 = 30_000;

#[derive(Clone)]
pub struct Calendar {
    bus_name: String,
    path: String,
}

pub struct Watch {
    view: String,
    _subscription: gio::SignalSubscription,
}

async fn read_managed_objects(bus: &gio::DBusConnection) -> Option<Variant> {
    let reply = bus
        .call_future(
            Some(REGISTRY_BUS),
            REGISTRY_PATH,
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
            None,
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    Some(reply.child_value(0))
}

pub async fn read_sources(bus: &gio::DBusConnection) -> Option<Vec<Source>> {
    Some(sources::from_managed_objects(
        &read_managed_objects(bus).await?,
    ))
}

pub async fn read_targets(bus: &gio::DBusConnection) -> Option<Vec<Target>> {
    Some(sources::targets_from_managed_objects(
        &read_managed_objects(bus).await?,
    ))
}

pub async fn create_object(
    bus: &gio::DBusConnection,
    calendar_uid: &str,
    component: &str,
) -> Result<(), String> {
    let calendar = open(bus, calendar_uid)
        .await
        .ok_or_else(|| tr("No calendar can take events"))?;
    let result = call(
        bus,
        &calendar.bus_name,
        &calendar.path,
        CALENDAR_INTERFACE,
        "CreateObjects",
        Some(&(vec![component], 0u32).to_variant()),
    )
    .await;
    release(bus, &calendar, None);
    result.map(|_| ()).map_err(|mut error| {
        gio::DBusError::strip_remote_error(&mut error);
        error.message().to_owned()
    })
}

pub async fn open(bus: &gio::DBusConnection, uid: &str) -> Option<Calendar> {
    let reply = bus
        .call_future(
            Some(FACTORY_BUS),
            FACTORY_PATH,
            FACTORY_INTERFACE,
            "OpenCalendar",
            Some(&(uid,).to_variant()),
            VariantTy::new("(ss)").ok(),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    let (path, bus_name) = reply.get::<(String, String)>()?;
    let calendar = Calendar { bus_name, path };
    let open_result = call(
        bus,
        &calendar.bus_name,
        &calendar.path,
        CALENDAR_INTERFACE,
        "Open",
        None,
    )
    .await;
    if open_result.is_err() {
        release(bus, &calendar, None);
        return None;
    }
    Some(calendar)
}

pub async fn query(
    bus: &gio::DBusConnection,
    calendar: &Calendar,
    expression: &str,
) -> Option<Vec<String>> {
    let reply = bus
        .call_future(
            Some(&calendar.bus_name),
            &calendar.path,
            CALENDAR_INTERFACE,
            "GetObjectList",
            Some(&(expression,).to_variant()),
            VariantTy::new("(as)").ok(),
            gio::DBusCallFlags::NONE,
            QUERY_TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    reply.child_value(0).get::<Vec<String>>()
}

pub async fn watch(
    bus: &gio::DBusConnection,
    calendar: &Calendar,
    expression: &str,
    on_change: impl Fn() + 'static,
) -> Option<Watch> {
    let reply = bus
        .call_future(
            Some(&calendar.bus_name),
            &calendar.path,
            CALENDAR_INTERFACE,
            "GetView",
            Some(&(expression,).to_variant()),
            VariantTy::new("(o)").ok(),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    let view = reply.child_value(0).str()?.to_owned();
    let is_loaded = Rc::new(Cell::new(false));
    let subscription = bus.subscribe_to_signal(
        Some(&calendar.bus_name),
        Some(VIEW_INTERFACE),
        None,
        Some(&view),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| match signal.signal_name {
            "Complete" if !is_loaded.replace(true) => on_change(),
            "ObjectsAdded" | "ObjectsModified" | "ObjectsRemoved" if is_loaded.get() => on_change(),
            _ => {}
        },
    );
    let watch = Watch {
        view,
        _subscription: subscription,
    };
    let start_result = call(
        bus,
        &calendar.bus_name,
        &watch.view,
        VIEW_INTERFACE,
        "Start",
        None,
    )
    .await;
    if start_result.is_err() {
        dispose_view(bus, calendar, watch);
        return None;
    }
    Some(watch)
}

pub fn dispose_view(bus: &gio::DBusConnection, calendar: &Calendar, watch: Watch) {
    call_and_forget(
        bus,
        &calendar.bus_name,
        &watch.view,
        VIEW_INTERFACE,
        "Dispose",
    );
}

pub fn release(bus: &gio::DBusConnection, calendar: &Calendar, watch: Option<Watch>) {
    if let Some(watch) = watch {
        dispose_view(bus, calendar, watch);
    }
    call_and_forget(
        bus,
        &calendar.bus_name,
        &calendar.path,
        CALENDAR_INTERFACE,
        "Close",
    );
}

async fn call(
    bus: &gio::DBusConnection,
    name: &str,
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<&Variant>,
) -> Result<Variant, glib::Error> {
    bus.call_future(
        Some(name),
        path,
        interface,
        method,
        arguments,
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT_MILLIS,
    )
    .await
}

fn call_and_forget(
    bus: &gio::DBusConnection,
    name: &str,
    path: &str,
    interface: &str,
    method: &str,
) {
    bus.call(
        Some(name),
        path,
        interface,
        method,
        None,
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT_MILLIS,
        gio::Cancellable::NONE,
        |_| {},
    );
}

pub fn range_query(first_unix: i64, end_unix: i64, zone: &str) -> String {
    format!(
        "(occur-in-time-range? (make-time \"{}\") (make-time \"{}\") \"{zone}\")",
        iso_utc(first_unix),
        iso_utc(end_unix)
    )
}

fn iso_utc(unix: i64) -> String {
    format!("{}Z", format_date_time(unix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_query_names_both_ends_in_utc_and_the_zone_for_floating_times() {
        assert_eq!(
            range_query(1_793_491_200, 1_796_169_599, "Asia/Yekaterinburg"),
            "(occur-in-time-range? (make-time \"20261101T000000Z\") \
             (make-time \"20261201T235959Z\") \"Asia/Yekaterinburg\")"
        );
    }
}

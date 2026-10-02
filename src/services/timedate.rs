use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy};
use gtk4::prelude::*;

use crate::platform::dbus::{self, WAIT_FOR_PASSWORD};
use crate::services::wifi::remote_message;

pub const BUS: &str = "org.freedesktop.timedate1";
const PATH: &str = "/org/freedesktop/timedate1";
const CALL_TIMEOUT: i32 = 120_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub timezone: String,
    pub ntp: bool,
    pub can_ntp: bool,
    pub synchronized: bool,
    pub local_rtc: bool,
}

async fn system() -> Result<gio::DBusConnection, String> {
    gio::bus_get_future(gio::BusType::System)
        .await
        .map_err(|error| error.message().to_owned())
}

pub async fn state() -> Option<State> {
    let system = system().await.ok()?;
    let flag = |name: &'static str| {
        let system = system.clone();
        async move {
            dbus::bool_property(&system, BUS, PATH, BUS, name)
                .await
                .unwrap_or(false)
        }
    };
    Some(State {
        timezone: dbus::string_property(&system, BUS, PATH, BUS, "Timezone").await?,
        ntp: flag("NTP").await,
        can_ntp: flag("CanNTP").await,
        synchronized: flag("NTPSynchronized").await,
        local_rtc: flag("LocalRTC").await,
    })
}

async fn call(method: &str, arguments: Variant, reply: &str) -> Result<Variant, String> {
    system()
        .await?
        .call_future(
            Some(BUS),
            PATH,
            BUS,
            method,
            Some(&arguments),
            VariantTy::new(reply).ok(),
            gio::DBusCallFlags::ALLOW_INTERACTIVE_AUTHORIZATION,
            WAIT_FOR_PASSWORD,
        )
        .await
        .map_err(|error| remote_message(&error))
}

pub async fn set_timezone(zone: &str) -> Result<(), String> {
    call("SetTimezone", (zone, true).to_variant(), "()").await?;
    Ok(())
}

pub async fn set_ntp(on: bool) -> Result<(), String> {
    call("SetNTP", (on, true).to_variant(), "()").await?;
    Ok(())
}

pub async fn set_local_rtc(local: bool) -> Result<(), String> {
    call("SetLocalRTC", (local, false, true).to_variant(), "()").await?;
    Ok(())
}

pub async fn set_time(microseconds: i64) -> Result<(), String> {
    call("SetTime", (microseconds, false, true).to_variant(), "()").await?;
    Ok(())
}

pub async fn zones() -> Vec<String> {
    let Ok(system) = system().await else {
        return Vec::new();
    };
    system
        .call_future(
            Some(BUS),
            PATH,
            BUS,
            "ListTimezones",
            None,
            VariantTy::new("(as)").ok(),
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT,
        )
        .await
        .ok()
        .and_then(|reply| reply.child_value(0).get::<Vec<String>>())
        .unwrap_or_default()
}

pub fn offset_name(zone: &str) -> String {
    let now = glib::DateTime::now(&glib::TimeZone::new(Some(zone))).ok();
    let seconds = now.map(|now| now.utc_offset().as_seconds()).unwrap_or(0);
    format_offset(seconds)
}

pub fn format_offset(seconds: i64) -> String {
    let sign = if seconds < 0 { '−' } else { '+' };
    let minutes = seconds.abs() / 60;
    format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

pub fn parse_time(date: &str, time: &str, zone: &str) -> Option<i64> {
    let mut date_parts = date.trim().split('-').map(|part| part.parse::<i32>().ok());
    let (year, month, day) = (
        date_parts.next()??,
        date_parts.next()??,
        date_parts.next()??,
    );
    if date_parts.next().is_some() {
        return None;
    }
    let mut time_parts = time.trim().split(':').map(|part| part.parse::<f64>().ok());
    let (hour, minute) = (time_parts.next()??, time_parts.next()??);
    let second = time_parts.next().flatten().unwrap_or(0.0);
    let zone = glib::TimeZone::new(Some(zone));
    let moment =
        glib::DateTime::new(&zone, year, month, day, hour as i32, minute as i32, second).ok()?;
    Some(moment.to_unix() * 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_read_like_utc_plus_hours_and_minutes() {
        assert_eq!(format_offset(5 * 3600), "UTC+05:00");
        assert_eq!(format_offset(-(3 * 3600 + 1800)), "UTC−03:30");
        assert_eq!(format_offset(0), "UTC+00:00");
        assert_eq!(offset_name("UTC"), "UTC+00:00");
    }

    #[test]
    fn a_typed_date_and_time_become_microseconds_in_that_zone() {
        assert_eq!(
            parse_time("2026-10-02", "12:30", "UTC"),
            Some(1_790_944_200_000_000)
        );
        assert_eq!(
            parse_time("2026-10-02", "17:30:00", "Asia/Yekaterinburg"),
            Some(1_790_944_200_000_000)
        );
        assert_eq!(parse_time("2026-13-02", "12:30", "UTC"), None);
        assert_eq!(parse_time("yesterday", "12:30", "UTC"), None);
    }
}

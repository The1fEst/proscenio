use gtk4::gio;
use gtk4::glib;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::config;
use crate::core::i18n::{tr, trf, trn};
use crate::core::persistent;
use crate::panels::bar::clock::strftime_from_qt;
use crate::platform::dbus::sleep::subscribe_to_resume;
use crate::services::session::parse_clock_time;

const MINUTE_SECS: i64 = 60;
const HOUR_MINUTES: u32 = 60;
const MICROS_PER_SEC: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Indefinite,
    Until(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Minutes(u32),
    Clock { hour: u32, minute: u32 },
    Indefinite,
}

pub const MORNING: Preset = Preset::Clock { hour: 8, minute: 0 };

pub const DO_NOT_DISTURB_PRESETS: [Preset; 7] = [
    Preset::Minutes(15),
    Preset::Minutes(30),
    Preset::Minutes(60),
    Preset::Minutes(180),
    Preset::Minutes(480),
    MORNING,
    Preset::Indefinite,
];

impl Preset {
    pub fn from_clock_text(text: &str) -> Option<Preset> {
        let (hour, minute) = parse_clock_time(text)?
            .split_once(':')
            .and_then(|(hour, minute)| Some((hour.parse().ok()?, minute.parse().ok()?)))?;
        Some(Preset::Clock { hour, minute })
    }

    pub fn from_minutes_text(text: &str) -> Option<Preset> {
        text.trim()
            .parse()
            .ok()
            .filter(|minutes| *minutes > 0)
            .map(Preset::Minutes)
    }

    pub fn resolve(self, now: &glib::DateTime) -> Option<Hold> {
        match self {
            Preset::Indefinite => Some(Hold::Indefinite),
            Preset::Minutes(minutes) => Some(Hold::Until(
                now.to_unix() + i64::from(minutes) * MINUTE_SECS,
            )),
            Preset::Clock { hour, minute } => {
                find_next_clock_secs(now, hour, minute).map(Hold::Until)
            }
        }
    }

    pub fn label(self) -> String {
        match self {
            Preset::Indefinite => tr("Until I turn it off"),
            Preset::Minutes(minutes) if minutes % HOUR_MINUTES == 0 => trn(
                "For %1 hour",
                "For %1 hours",
                u64::from(minutes / HOUR_MINUTES),
            ),
            Preset::Minutes(minutes) => trn("For %1 minute", "For %1 minutes", u64::from(minutes)),
            Preset::Clock { hour, minute } => {
                let time_format = config::current().time_format.clone();
                glib::DateTime::new(
                    &glib::TimeZone::local(),
                    2000,
                    1,
                    1,
                    hour as i32,
                    minute as i32,
                    0.0,
                )
                .ok()
                .and_then(|time| format_status(&time, &time_format))
                .unwrap_or_default()
            }
        }
    }
}

pub fn find_next_clock_secs(now: &glib::DateTime, hour: u32, minute: u32) -> Option<i64> {
    let today = glib::DateTime::new(
        &now.timezone(),
        now.year(),
        now.month(),
        now.day_of_month(),
        hour as i32,
        minute as i32,
        0.0,
    )
    .ok()?;
    let next = if today.to_unix() > now.to_unix() {
        today
    } else {
        today.add_days(1).ok()?
    };
    Some(next.to_unix())
}

pub fn restore_hold(was_on: bool, saved_end_secs: Option<i64>, now_secs: i64) -> Option<Hold> {
    match (was_on, saved_end_secs) {
        (false, _) => None,
        (true, None) => Some(Hold::Indefinite),
        (true, Some(end_secs)) if end_secs > now_secs => Some(Hold::Until(end_secs)),
        (true, Some(_)) => None,
    }
}

pub fn format_status(end: &glib::DateTime, time_format: &str) -> Option<String> {
    let clock = end.format(&strftime_from_qt(time_format)).ok()?;
    Some(trf("Until %1", &[&clock]))
}

pub fn format_status_at(end_secs: i64) -> String {
    glib::DateTime::from_unix_local(end_secs)
        .ok()
        .and_then(|end| format_status(&end, &config::current().time_format))
        .unwrap_or_default()
}

fn now_secs() -> i64 {
    glib::real_time() / MICROS_PER_SEC
}

pub struct Deadline {
    key: [&'static str; 2],
    end_secs: Cell<Option<i64>>,
    source: RefCell<Option<glib::SourceId>>,
    on_expire: Box<dyn Fn()>,
    _resume_subscription: Option<gio::SignalSubscription>,
}

impl Deadline {
    pub fn new(
        key: [&'static str; 2],
        system: Option<&gio::DBusConnection>,
        on_expire: impl Fn() + 'static,
    ) -> Rc<Self> {
        Rc::new_cyclic(|weak: &Weak<Self>| {
            let weak = weak.clone();
            Deadline {
                key,
                end_secs: Cell::new(None),
                source: RefCell::new(None),
                on_expire: Box::new(on_expire),
                _resume_subscription: system.map(|system| {
                    subscribe_to_resume(system, move || {
                        if let Some(deadline) = weak.upgrade() {
                            deadline.check();
                        }
                    })
                }),
            }
        })
    }

    pub fn end_secs(&self) -> Option<i64> {
        self.end_secs.get()
    }

    pub fn restore(&self, was_on: bool) -> Option<Hold> {
        let saved_end_secs = persistent::read(&self.key).and_then(|value| value.as_i64());
        restore_hold(was_on, saved_end_secs, now_secs())
    }

    pub fn start(self: &Rc<Self>, end_secs: i64) {
        self.end_secs.set(Some(end_secs));
        persistent::write(&self.key, Value::from(end_secs));
        self.check();
    }

    pub fn clear(&self) {
        if let Some(source) = self.source.take() {
            source.remove();
        }
        self.end_secs.set(None);
        persistent::write(&self.key, Value::Null);
    }

    fn check(self: &Rc<Self>) {
        let Some(end_secs) = self.end_secs.get() else {
            return;
        };
        let remaining_secs = end_secs - now_secs();
        if remaining_secs <= 0 {
            self.clear();
            (self.on_expire)();
            return;
        }
        if let Some(source) = self.source.take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_seconds_local_once(
            u32::try_from(remaining_secs).unwrap_or(u32::MAX),
            move || {
                let Some(deadline) = weak.upgrade() else {
                    return;
                };
                deadline.source.take();
                deadline.check();
            },
        );
        self.source.replace(Some(source));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::i18n::use_language;

    fn build_utc_time(
        year: i32,
        month: i32,
        day: i32,
        hour: i32,
        minute: i32,
        second: f64,
    ) -> glib::DateTime {
        glib::DateTime::new(
            &glib::TimeZone::utc(),
            year,
            month,
            day,
            hour,
            minute,
            second,
        )
        .unwrap()
    }

    #[test]
    fn a_clock_time_before_now_is_tomorrows() {
        let cases = [
            (
                build_utc_time(2026, 10, 31, 7, 0, 0.0),
                build_utc_time(2026, 10, 31, 8, 0, 0.0),
            ),
            (
                build_utc_time(2026, 10, 31, 7, 59, 59.0),
                build_utc_time(2026, 10, 31, 8, 0, 0.0),
            ),
            (
                build_utc_time(2026, 10, 31, 8, 0, 0.0),
                build_utc_time(2026, 11, 1, 8, 0, 0.0),
            ),
            (
                build_utc_time(2026, 10, 31, 8, 0, 1.0),
                build_utc_time(2026, 11, 1, 8, 0, 0.0),
            ),
            (
                build_utc_time(2026, 10, 31, 23, 30, 0.0),
                build_utc_time(2026, 11, 1, 8, 0, 0.0),
            ),
            (
                build_utc_time(2026, 12, 31, 23, 30, 0.0),
                build_utc_time(2027, 1, 1, 8, 0, 0.0),
            ),
            (
                build_utc_time(2026, 10, 31, 0, 0, 0.0),
                build_utc_time(2026, 10, 31, 8, 0, 0.0),
            ),
        ];
        for (now, expected) in cases {
            assert_eq!(
                find_next_clock_secs(&now, 8, 0),
                Some(expected.to_unix()),
                "{now:?}"
            );
        }
    }

    #[test]
    fn a_preset_holds_for_its_minutes_or_until_its_clock_time() {
        let now = build_utc_time(2026, 10, 31, 23, 30, 0.0);
        assert_eq!(
            Preset::Minutes(15).resolve(&now),
            Some(Hold::Until(now.to_unix() + 15 * 60))
        );
        assert_eq!(
            Preset::Minutes(480).resolve(&now),
            Some(Hold::Until(now.to_unix() + 8 * 3600))
        );
        assert_eq!(
            MORNING.resolve(&now),
            Some(Hold::Until(
                build_utc_time(2026, 11, 1, 8, 0, 0.0).to_unix()
            ))
        );
        assert_eq!(Preset::Indefinite.resolve(&now), Some(Hold::Indefinite));
    }

    #[test]
    fn clock_text_becomes_a_preset_only_when_it_is_a_valid_time() {
        assert_eq!(
            Preset::from_clock_text("7:05"),
            Some(Preset::Clock { hour: 7, minute: 5 })
        );
        assert_eq!(
            Preset::from_clock_text(" 23:59 "),
            Some(Preset::Clock {
                hour: 23,
                minute: 59
            })
        );
        for text in ["24:00", "12:60", "12", "ab:cd", ""] {
            assert_eq!(Preset::from_clock_text(text), None, "{text}");
        }
    }

    #[test]
    fn minutes_text_becomes_a_preset_only_when_it_is_a_positive_whole_number() {
        assert_eq!(Preset::from_minutes_text(" 45 "), Some(Preset::Minutes(45)));
        for text in ["0", "-5", "1.5", "soon", ""] {
            assert_eq!(Preset::from_minutes_text(text), None, "{text}");
        }
    }

    #[test]
    fn a_saved_end_is_kept_only_while_it_is_in_the_future() {
        let now_secs = 1_000;
        let cases = [
            (false, None, None),
            (false, Some(2_000), None),
            (true, None, Some(Hold::Indefinite)),
            (true, Some(2_000), Some(Hold::Until(2_000))),
            (true, Some(1_001), Some(Hold::Until(1_001))),
            (true, Some(1_000), None),
            (true, Some(10), None),
        ];
        for (was_on, saved_end_secs, expected) in cases {
            assert_eq!(
                restore_hold(was_on, saved_end_secs, now_secs),
                expected,
                "{was_on} {saved_end_secs:?}"
            );
        }
    }

    #[test]
    fn the_status_reads_until_the_end_in_the_shell_time_format() {
        let end = build_utc_time(2026, 10, 31, 8, 5, 0.0);
        use_language("en_US");
        assert_eq!(format_status(&end, "hh:mm").as_deref(), Some("Until 08:05"));
        assert_eq!(format_status(&end, "h:mm").as_deref(), Some("Until 8:05"));
        use_language("ru_RU");
        assert_eq!(format_status(&end, "hh:mm").as_deref(), Some("До 08:05"));
    }
}

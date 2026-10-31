use std::cell::Cell;
use std::rc::Rc;

use super::location::Location;
use super::schedule::{Span, plan_dark_switch};
use super::{Session, now_seconds, parse_clock_minutes};
use crate::core::config::{self, ScheduleMode};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Settings {
    schedule: ScheduleMode,
    dark_from_minutes: i32,
    light_from_minutes: i32,
}

pub struct DarkModeSchedule {
    session: Rc<Session>,
    location: Rc<Location>,
    settings: Cell<Option<Settings>>,
    schedule_is_dark: Cell<Option<bool>>,
}

impl DarkModeSchedule {
    pub fn new(session: &Rc<Session>, location: &Rc<Location>) -> Rc<Self> {
        Rc::new(DarkModeSchedule {
            session: session.clone(),
            location: location.clone(),
            settings: Cell::new(None),
            schedule_is_dark: Cell::new(None),
        })
    }

    pub fn follow_clock(&self) {
        let config = config::current();
        if !config.is_dark_mode_automatic {
            self.settings.set(None);
            self.schedule_is_dark.set(None);
            return;
        }
        let settings = Settings {
            schedule: config.dark_mode_schedule,
            dark_from_minutes: parse_clock_minutes(
                &config.dark_mode_dark_from,
                config::DARK_MODE_DARK_FROM,
            ),
            light_from_minutes: parse_clock_minutes(
                &config.dark_mode_light_from,
                config::DARK_MODE_LIGHT_FROM,
            ),
        };
        if self.settings.replace(Some(settings)) != Some(settings) {
            self.schedule_is_dark.set(None);
        }
        let daylight = (settings.schedule == ScheduleMode::Sun)
            .then(|| self.location.compute_daylight_today())
            .flatten();
        let span = Span::resolve(
            settings.schedule,
            settings.dark_from_minutes,
            settings.light_from_minutes,
            daylight,
        );
        let schedule_is_dark = span.contains(now_seconds() / 60);
        let previous_schedule_is_dark = self.schedule_is_dark.replace(Some(schedule_is_dark));
        if let Some(is_dark) = plan_dark_switch(
            previous_schedule_is_dark,
            schedule_is_dark,
            self.session.is_dark_mode.get(),
        ) {
            self.session.set_dark_mode(is_dark);
        }
    }
}

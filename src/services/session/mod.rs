pub mod accounts;
pub mod appearance;
pub mod background;
pub mod states;
pub mod timedate;
pub mod timer;
pub mod todo;
pub mod weather;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::{self, detach};
use crate::core::{persistent, watch};
use crate::services::hyprland::screencolor::{NEUTRAL_TEMPERATURE, ScreenColor};

pub struct Session {
    pub is_night_light_on: Cell<bool>,
    pub keeps_awake: Cell<bool>,
    pub is_dark_mode: Cell<bool>,
    pub is_night_light_automatic: Cell<bool>,
    pub temperature: Cell<i32>,
    color: Rc<ScreenColor>,
    inhibitor: RefCell<Option<process::Running>>,
    schedule: Schedule,
    listeners: Listeners,
    config_watch: RefCell<Option<watch::Watch>>,
    dark_mode_monitor: RefCell<Option<gio::FileMonitor>>,
}

struct Schedule {
    from: Cell<i32>,
    to: Cell<i32>,
    transition: Cell<i32>,
    manual_override: Cell<Option<(bool, i64)>>,
    scheduled_kelvin: Cell<Option<i32>>,
    needs_apply: Cell<bool>,
}

#[derive(Clone, Copy)]
struct Night {
    from: i32,
    to: i32,
    transition: i32,
    temperature: i32,
}

const KELVIN_STEP: f64 = 10.0;
const DAY_SECONDS: i32 = 24 * 60 * 60;
const INHIBIT: [&str; 2] = ["idle", "inhibit"];

fn temperature_at(now: i32, night: &Night) -> Option<i32> {
    let start = night.from - night.transition;
    let length = (night.to - start).rem_euclid(DAY_SECONDS);
    let offset = (now - start).rem_euclid(DAY_SECONDS);
    if offset >= length + night.transition {
        return None;
    }
    let progress = |seconds: i32| {
        if night.transition == 0 {
            1.0
        } else {
            (seconds as f64 / night.transition as f64).clamp(0.0, 1.0)
        }
    };
    let fade_in_part = progress(offset);
    let fade_out_part = if offset > length {
        1.0 - progress(offset - length)
    } else {
        1.0
    };
    let part = fade_in_part.min(fade_out_part);
    let day_mired = 1e6 / NEUTRAL_TEMPERATURE as f64;
    let night_mired = 1e6 / night.temperature.max(1000) as f64;
    let kelvin = 1e6 / (day_mired + (night_mired - day_mired) * part);
    Some(((kelvin / KELVIN_STEP).round() * KELVIN_STEP) as i32)
}

fn minutes(clock: &str) -> Option<i32> {
    let mut parts = clock.split(':').map(|part| part.trim().parse::<i32>());
    let hour = parts.next()?.ok()?;
    let minute = parts.next().transpose().ok()?.unwrap_or(0);
    Some(hour * 60 + minute)
}

fn parse_night_span(from: &str, to: &str) -> (i32, i32) {
    let parse_time = |clock: &str, default: &str| {
        minutes(clock)
            .or_else(|| minutes(default))
            .unwrap_or_default()
    };
    (
        parse_time(from, config::NIGHT_FROM),
        parse_time(to, config::NIGHT_TO),
    )
}

fn now_seconds() -> i32 {
    glib::DateTime::now_local()
        .map(|now| (now.hour() * 60 + now.minute()) * 60 + now.second())
        .unwrap_or(0)
}

fn releases(is_on: bool, from: i32, to: i32, transition: i32) -> [i32; 2] {
    if is_on {
        [from, to]
    } else {
        [from - transition, to + transition]
    }
}

fn has_passed(boundary: i32, override_start_seconds: i64, now: &glib::DateTime) -> bool {
    let boundary = boundary.rem_euclid(24 * 60);
    let Ok(today) = glib::DateTime::new(
        &now.timezone(),
        now.year(),
        now.month(),
        now.day_of_month(),
        boundary / 60,
        boundary % 60,
        0.0,
    ) else {
        return false;
    };
    let last = if today.to_unix() > now.to_unix() {
        today.add_days(-1)
    } else {
        Ok(today)
    };
    last.is_ok_and(|last| last.to_unix() > override_start_seconds)
}

impl Session {
    pub fn new(config: &Config, color: &Rc<ScreenColor>) -> Rc<Self> {
        let (from, to) = parse_night_span(&config.night_from, &config.night_to);
        let session = Rc::new(Session {
            is_night_light_on: Cell::new(false),
            keeps_awake: Cell::new(false),
            is_dark_mode: Cell::new(read_dark_mode()),
            is_night_light_automatic: Cell::new(config.night_automatic),
            temperature: Cell::new(config.night_temperature),
            color: color.clone(),
            inhibitor: RefCell::new(None),
            schedule: Schedule {
                from: Cell::new(from),
                to: Cell::new(to),
                transition: Cell::new(config.night_transition),
                manual_override: Cell::new(None),
                scheduled_kelvin: Cell::new(None),
                needs_apply: Cell::new(true),
            },
            listeners: Listeners::default(),
            config_watch: RefCell::new(None),
            dark_mode_monitor: RefCell::new(None),
        });
        session.follow_dark_mode();
        session.re_evaluate();
        let was_kept_awake = !persistent::is_new_hyprland_instance()
            && persistent::read(&INHIBIT)
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        if was_kept_awake {
            session.toggle_keep_awake();
        } else {
            persistent::write(&INHIBIT, false.into());
        }
        session
            .config_watch
            .replace(Some(watch::config("/light/night", {
                let session = Rc::downgrade(&session);
                move || {
                    let Some(session) = session.upgrade() else {
                        return;
                    };
                    session.follow_config();
                }
            })));
        session
    }

    fn follow_config(&self) {
        let config = config::current();
        let schedule = &self.schedule;
        let (from, to) = parse_night_span(&config.night_from, &config.night_to);
        let transition = config.night_transition;
        let has_schedule_changed = (schedule.from.replace(from) != from)
            | (schedule.to.replace(to) != to)
            | (schedule.transition.replace(transition) != transition);
        let has_automatic_changed = self
            .is_night_light_automatic
            .replace(config.night_automatic)
            != config.night_automatic;
        let has_temperature_changed =
            self.temperature.replace(config.night_temperature) != config.night_temperature;
        if has_schedule_changed || has_automatic_changed {
            schedule.manual_override.set(None);
            schedule.needs_apply.set(true);
        }
        if has_temperature_changed && self.is_night_light_on.get() && !self.follows_schedule() {
            self.color.set_temperature(Some(config.night_temperature));
        }
        self.re_evaluate();
        self.announce();
    }

    fn follows_schedule(&self) -> bool {
        self.is_night_light_automatic.get() && self.schedule.manual_override.get().is_none()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn follow_clock(&self) {
        self.re_evaluate();
    }

    fn re_evaluate(&self) {
        let schedule = &self.schedule;
        let is_released = match (schedule.manual_override.get(), glib::DateTime::now_local()) {
            (Some((is_on, override_start_seconds)), Ok(now)) => releases(
                is_on,
                schedule.from.get(),
                schedule.to.get(),
                schedule.transition.get(),
            )
            .iter()
            .any(|release| has_passed(*release, override_start_seconds, &now)),
            _ => false,
        };
        if is_released {
            schedule.manual_override.set(None);
        }
        let night = Night {
            from: schedule.from.get() * 60,
            to: schedule.to.get() * 60,
            transition: schedule.transition.get() * 60,
            temperature: self.temperature.get(),
        };
        let kelvin = temperature_at(now_seconds(), &night);
        let has_kelvin_changed = schedule.scheduled_kelvin.replace(kelvin) != kelvin;
        if schedule.needs_apply.replace(false) || has_kelvin_changed || is_released {
            self.ensure_state();
        }
    }

    fn ensure_state(&self) {
        if !self.follows_schedule() {
            return;
        }
        match self.schedule.scheduled_kelvin.get() {
            Some(kelvin) => self.enable_temperature(kelvin),
            None => self.disable_temperature(),
        }
    }

    fn enable_temperature(&self, kelvin: i32) {
        self.is_night_light_on.set(true);
        self.color.set_temperature(Some(kelvin));
        self.announce();
    }

    fn disable_temperature(&self) {
        self.is_night_light_on.set(false);
        self.color.set_temperature(None);
        self.announce();
    }

    pub fn toggle_night_light(&self) {
        self.toggle_temperature(None);
    }

    pub fn toggle_temperature(&self, should_be_on: Option<bool>) {
        let schedule = &self.schedule;
        let (was_on, override_start_seconds) = schedule
            .manual_override
            .get()
            .unwrap_or((self.is_night_light_on.get(), glib::real_time() / 1_000_000));
        let should_turn_on = should_be_on.unwrap_or(!was_on);
        schedule
            .manual_override
            .set(Some((should_turn_on, override_start_seconds)));
        if should_turn_on {
            self.enable_temperature(self.temperature.get());
        } else {
            self.disable_temperature();
        }
    }

    pub fn set_night_light_automatic(&self, is_automatic: bool) {
        self.is_night_light_automatic.set(is_automatic);
        Config::store_night("automatic", serde_json::Value::Bool(is_automatic));
        self.schedule.manual_override.set(None);
        self.schedule.needs_apply.set(true);
        self.re_evaluate();
        self.announce();
    }

    pub fn set_temperature(&self, kelvin: i32) {
        self.temperature.set(kelvin);
        Config::store_night("colorTemperature", serde_json::Value::from(kelvin));
        if self.follows_schedule() {
            self.re_evaluate();
        } else if self.is_night_light_on.get() {
            self.color.set_temperature(Some(kelvin));
        }
        self.announce();
    }

    pub fn toggle_keep_awake(&self) {
        let mut inhibitor = self.inhibitor.borrow_mut();
        match inhibitor.take() {
            Some(mut child) => {
                child.stop();
                self.keeps_awake.set(false);
            }
            None => {
                let mut command = process::quiet(&[
                    "systemd-inhibit",
                    "--what=idle:sleep",
                    "--who=proscenio",
                    "--why=Keep awake",
                    "cat",
                ]);
                command.stdin(std::process::Stdio::piped());
                *inhibitor = process::start(command);
                self.keeps_awake.set(inhibitor.is_some());
            }
        }
        drop(inhibitor);
        persistent::write(&INHIBIT, self.keeps_awake.get().into());
        self.announce();
    }

    pub fn lock(&self) {
        detach(&["loginctl", "lock-session"]);
    }

    pub fn suspend(&self) {
        detach(&["bash", "-c", "systemctl suspend || loginctl suspend"]);
    }

    pub fn hibernate(&self) {
        detach(&["bash", "-c", "systemctl hibernate || loginctl hibernate"]);
    }

    pub fn logout(&self) {
        close_all_windows();
        process::launch(&[
            "bash",
            "-c",
            "systemctl --user stop graphical-session.target; pkill -i Hyprland",
        ]);
    }

    pub fn poweroff(&self) {
        close_all_windows();
        detach(&["bash", "-c", "systemctl poweroff || loginctl poweroff"]);
    }

    pub fn reboot(&self) {
        close_all_windows();
        detach(&["bash", "-c", "reboot || loginctl reboot"]);
    }

    pub fn reboot_to_firmware(&self) {
        close_all_windows();
        detach(&[
            "bash",
            "-c",
            "systemctl reboot --firmware-setup || loginctl reboot --firmware-setup",
        ]);
    }

    pub fn reboot_to_windows(&self) {
        close_all_windows();
        std::thread::spawn(|| {
            let Some(entry) = crate::core::process::output(&["efibootmgr"])
                .as_deref()
                .and_then(windows_entry)
            else {
                eprintln!(
                    "boot-next-windows: no 'Windows Boot Manager' entry in the UEFI boot list"
                );
                return;
            };
            if !crate::core::process::run(&["pkexec", "efibootmgr", "--bootnext", &entry]) {
                return;
            }
            if !crate::core::process::run(&["systemctl", "reboot"]) {
                crate::core::process::run(&["loginctl", "reboot"]);
            }
        });
    }

    pub fn toggle_dark_mode(&self) {
        let mode = if self.is_dark_mode.get() {
            "light"
        } else {
            "dark"
        };
        crate::theming::switchwall::detach(&["--mode", mode, "--noswitch"]);
    }

    fn follow_dark_mode(self: &Rc<Self>) {
        let session = Rc::downgrade(self);
        let monitor = watch_dark_mode(move |is_dark| {
            let Some(session) = session.upgrade() else {
                return;
            };
            if is_dark != session.is_dark_mode.get() {
                session.is_dark_mode.set(is_dark);
                session.announce();
            }
        });
        self.dark_mode_monitor.replace(monitor);
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

fn palette_path() -> std::path::PathBuf {
    crate::core::paths::generated_dir().join("material_colors.scss")
}

pub fn watch_dark_mode(on_change: impl Fn(bool) + 'static) -> Option<gio::FileMonitor> {
    let monitor = gio::File::for_path(palette_path())
        .monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        .ok()?;
    monitor.connect_changed(move |_, _, _, event| {
        if matches!(
            event,
            gio::FileMonitorEvent::ChangesDoneHint
                | gio::FileMonitorEvent::Created
                | gio::FileMonitorEvent::MovedIn
        ) {
            on_change(read_dark_mode());
        }
    });
    Some(monitor)
}

pub fn read_dark_mode() -> bool {
    std::fs::read_to_string(palette_path())
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("$darkmode:"))
                .map(|value| {
                    value
                        .trim()
                        .trim_end_matches(';')
                        .eq_ignore_ascii_case("true")
                })
        })
        .unwrap_or(true)
}

fn close_all_windows() {
    let Some(clients) = crate::platform::hyprland::hypr::json("clients")
        .and_then(|value| value.as_array().cloned())
    else {
        return;
    };
    for pid in clients
        .iter()
        .filter_map(|client| client.get("pid").and_then(serde_json::Value::as_i64))
    {
        detach(&["kill", &pid.to_string()]);
    }
}

fn windows_entry(boot_list: &str) -> Option<String> {
    boot_list.lines().find_map(|line| {
        let rest = line.strip_prefix("Boot")?;
        let number = rest.get(..4)?;
        if !number.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let label = rest[4..].strip_prefix('*').unwrap_or(&rest[4..]);
        if !label.starts_with(char::is_whitespace) {
            return None;
        }
        let tail = label.trim_start().strip_prefix("Windows Boot Manager")?;
        (tail.is_empty() || tail.starts_with(char::is_whitespace)).then(|| number.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schedule_is_fully_warm_from_its_start_and_eases_out_after_its_end() {
        let clock = |hour: i32, minute: i32| (hour * 60 + minute) * 60;
        let night = Night {
            from: clock(19, 0),
            to: clock(6, 30),
            transition: 30 * 60,
            temperature: 4000,
        };
        let cases = [
            (clock(18, 29), None),
            (clock(18, 30), Some(6600)),
            (clock(18, 45), Some(4980)),
            (clock(19, 0), Some(4000)),
            (clock(3, 0), Some(4000)),
            (clock(6, 30), Some(4000)),
            (clock(6, 45), Some(4980)),
            (clock(7, 0), None),
        ];
        for (now, expected) in cases {
            assert_eq!(temperature_at(now, &night), expected, "at {now} s");
        }
        let instant = Night {
            transition: 0,
            ..night
        };
        assert_eq!(temperature_at(clock(19, 0), &instant), Some(4000));
        assert_eq!(temperature_at(clock(6, 30), &instant), None);
    }

    #[test]
    fn a_stored_time_that_does_not_read_as_a_time_falls_back_to_its_default() {
        assert_eq!(parse_night_span("21:15", "24:00"), (21 * 60 + 15, 24 * 60));
        assert_eq!(parse_night_span("20", " 7:05 "), (20 * 60, 7 * 60 + 5));
        assert_eq!(parse_night_span("7pm", "06.30"), (19 * 60, 6 * 60 + 30));
        assert_eq!(parse_night_span("19:3O", ""), (19 * 60, 6 * 60 + 30));
    }

    #[test]
    fn a_manual_switch_hands_over_where_the_schedule_shows_the_same() {
        let (from, to, transition) = (19 * 60, 6 * 60 + 30, 30);
        let night = Night {
            from: from * 60,
            to: to * 60,
            transition: transition * 60,
            temperature: 4000,
        };
        assert_eq!(releases(true, from, to, transition), [from, to]);
        assert_eq!(
            releases(false, from, to, transition),
            [from - transition, to + transition]
        );
        for release in releases(true, from, to, transition) {
            assert_eq!(
                temperature_at(release * 60, &night),
                Some(4000),
                "on at {release}"
            );
        }
        for release in releases(false, from, to, transition) {
            let temperature = temperature_at(release * 60, &night);
            assert!(
                temperature.is_none() || temperature == Some(NEUTRAL_TEMPERATURE),
                "off at {release}: {temperature:?}"
            );
        }
    }

    #[test]
    fn a_boundary_counts_as_passed_only_when_it_came_after_the_toggle() {
        let time_zone = glib::TimeZone::new(Some("Europe/Berlin"));
        assert_eq!(time_zone.identifier(), "Europe/Berlin");
        let local = |day: i32, hour: i32, minute: i32, second: f64| {
            glib::DateTime::new(&time_zone, 2026, 10, day, hour, minute, second).unwrap()
        };
        let utc = |day: i32, hour: i32, minute: i32| {
            glib::DateTime::from_utc(2026, 10, day, hour, minute, 0.0)
                .and_then(|time| time.to_timezone(&time_zone))
                .unwrap()
        };
        let cases = [
            (600, local(14, 10, 0, 10.0), local(14, 10, 0, 50.0), false),
            (600, local(14, 9, 59, 50.0), local(14, 10, 0, 10.0), true),
            (1110, local(14, 18, 0, 0.0), local(14, 18, 31, 0.0), true),
            (1110, local(14, 18, 0, 0.0), local(14, 18, 29, 0.0), false),
            (390, local(14, 23, 0, 0.0), local(15, 6, 31, 0.0), true),
            (1110, local(14, 10, 0, 20.0), local(15, 10, 0, 40.0), true),
            (390, local(14, 10, 0, 0.0), local(16, 9, 0, 0.0), true),
            (390, utc(25, 0, 40), utc(25, 1, 5), false),
            (1440, local(14, 23, 0, 0.0), local(15, 0, 30, 0.0), true),
            (1515, local(14, 23, 0, 0.0), local(15, 0, 30, 0.0), false),
        ];
        for (boundary, override_start, now, expected) in cases {
            assert_eq!(
                has_passed(boundary, override_start.to_unix(), &now),
                expected,
                "{boundary} between {:?} and {:?}",
                override_start.format_iso8601().unwrap(),
                now.format_iso8601().unwrap()
            );
        }
    }

    #[test]
    fn windows_entry_matches_what_the_script_grepped_for() {
        let cases = [
            (
                "BootCurrent: 0001\nBootOrder: 0001,0000\nBoot0000* Windows Boot Manager\tHD(1,GPT)\nBoot0001* Linux Boot Manager\tHD(1,GPT)\n",
                Some("0000"),
            ),
            ("Boot000A  Windows Boot Manager\n", Some("000A")),
            ("Boot0003* Windows Boot Manager2\tHD\n", None),
            (
                "Boot0004* Linux\nBoot0005* Windows Boot Manager",
                Some("0005"),
            ),
            ("BootCurrent: 0001\n", None),
        ];
        for (list, expected) in cases {
            assert_eq!(windows_entry(list).as_deref(), expected);
        }
    }
}

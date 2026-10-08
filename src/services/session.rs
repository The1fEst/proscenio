use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::{self, detach};
use crate::core::{persistent, watch};
use crate::services::screencolor::{NEUTRAL_TEMPERATURE, ScreenColor};

#[derive(Clone)]
pub struct Session {
    pub night: Rc<Cell<bool>>,
    pub awake: Rc<Cell<bool>>,
    pub dark: Rc<Cell<bool>>,
    pub automatic: Rc<Cell<bool>>,
    pub temperature: Rc<Cell<i32>>,
    color: Rc<ScreenColor>,
    inhibitor: Rc<RefCell<Option<process::Running>>>,
    schedule: Rc<Schedule>,
    listeners: Rc<Listeners>,
    following: Rc<RefCell<Option<watch::Watch>>>,
}

struct Schedule {
    from: Cell<i32>,
    to: Cell<i32>,
    transition: Cell<i32>,
    manual: Cell<Option<(bool, i32)>>,
    scheduled: Cell<Option<i32>>,
    first: Cell<bool>,
}

#[derive(Clone, Copy)]
struct Night {
    from: i32,
    to: i32,
    transition: i32,
    temperature: i32,
}

const KELVIN_STEP: f64 = 10.0;
const DAY: i32 = 24 * 60 * 60;
const INHIBIT: [&str; 2] = ["idle", "inhibit"];

fn scheduled(now: i32, night: &Night) -> Option<i32> {
    let start = night.from - night.transition;
    let length = (night.to - start).rem_euclid(DAY);
    let into = (now - start).rem_euclid(DAY);
    if into >= length + night.transition {
        return None;
    }
    let progress = |seconds: i32| {
        if night.transition == 0 {
            1.0
        } else {
            (seconds as f64 / night.transition as f64).clamp(0.0, 1.0)
        }
    };
    let warming = progress(into);
    let cooling = if into > length {
        1.0 - progress(into - length)
    } else {
        1.0
    };
    let part = warming.min(cooling);
    let day = 1e6 / NEUTRAL_TEMPERATURE as f64;
    let warm = 1e6 / night.temperature.max(1000) as f64;
    let kelvin = 1e6 / (day + (warm - day) * part);
    Some(((kelvin / KELVIN_STEP).round() * KELVIN_STEP) as i32)
}

fn minutes(clock: &str) -> i32 {
    let mut parts = clock
        .split(':')
        .map(|part| part.trim().parse::<i32>().unwrap_or(0));
    parts.next().unwrap_or(0) * 60 + parts.next().unwrap_or(0)
}

fn now_minutes() -> i32 {
    glib::DateTime::now_local()
        .map(|now| now.hour() * 60 + now.minute())
        .unwrap_or(0)
}

fn now_seconds() -> i32 {
    glib::DateTime::now_local()
        .map(|now| (now.hour() * 60 + now.minute()) * 60 + now.second())
        .unwrap_or(0)
}

fn between(time: i32, from: i32, to: i32) -> bool {
    if from < to {
        time >= from && time <= to
    } else {
        time >= from || time <= to
    }
}

impl Session {
    pub fn new(config: &Config, color: &Rc<ScreenColor>) -> Self {
        let session = Session {
            night: Rc::new(Cell::new(false)),
            awake: Rc::new(Cell::new(false)),
            dark: Rc::new(Cell::new(dark_mode())),
            automatic: Rc::new(Cell::new(config.night_automatic)),
            temperature: Rc::new(Cell::new(config.night_temperature)),
            color: color.clone(),
            inhibitor: Rc::new(RefCell::new(None)),
            schedule: Rc::new(Schedule {
                from: Cell::new(minutes(&config.night_from)),
                to: Cell::new(minutes(&config.night_to)),
                transition: Cell::new(config.night_transition),
                manual: Cell::new(None),
                scheduled: Cell::new(None),
                first: Cell::new(true),
            }),
            listeners: Rc::default(),
            following: Rc::default(),
        };
        session.watch_mode();
        session.re_evaluate();
        let kept_awake = !persistent::is_new_hyprland_instance()
            && persistent::read(&INHIBIT)
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        if kept_awake {
            session.toggle_awake();
        } else {
            persistent::write(&INHIBIT, false.into());
        }
        let follower = session.clone();
        session
            .following
            .replace(Some(watch::config("/light/night", move || {
                follower.follow_config()
            })));
        session
    }

    fn follow_config(&self) {
        let config = config::current();
        let schedule = &self.schedule;
        let from = minutes(&config.night_from);
        let to = minutes(&config.night_to);
        let transition = config.night_transition;
        let rescheduled = (schedule.from.replace(from) != from)
            | (schedule.to.replace(to) != to)
            | (schedule.transition.replace(transition) != transition);
        let switched = self.automatic.replace(config.night_automatic) != config.night_automatic;
        let retuned =
            self.temperature.replace(config.night_temperature) != config.night_temperature;
        if rescheduled || switched {
            schedule.manual.set(None);
            schedule.first.set(true);
        }
        if retuned && self.night.get() && !self.follows_schedule() {
            self.color.set_temperature(Some(config.night_temperature));
        }
        self.re_evaluate();
        self.announce();
    }

    fn follows_schedule(&self) -> bool {
        self.automatic.get() && self.schedule.manual.get().is_none()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn follow_clock(&self) {
        self.re_evaluate();
    }

    fn re_evaluate(&self) {
        let schedule = &self.schedule;
        let now = now_minutes();
        let start = (schedule.from.get() - schedule.transition.get()).rem_euclid(24 * 60);
        if let Some((_, since)) = schedule.manual.get()
            && (between(start, since, now) || between(schedule.to.get(), since, now))
        {
            schedule.manual.set(None);
        }
        let night = Night {
            from: schedule.from.get() * 60,
            to: schedule.to.get() * 60,
            transition: schedule.transition.get() * 60,
            temperature: self.temperature.get(),
        };
        let kelvin = scheduled(now_seconds(), &night);
        let changed = schedule.scheduled.replace(kelvin) != kelvin;
        if schedule.first.replace(false) || changed {
            self.ensure_state();
        }
    }

    fn ensure_state(&self) {
        if !self.follows_schedule() {
            return;
        }
        match self.schedule.scheduled.get() {
            Some(kelvin) => self.enable_temperature(kelvin),
            None => self.disable_temperature(),
        }
    }

    fn enable_temperature(&self, kelvin: i32) {
        self.night.set(true);
        self.color.set_temperature(Some(kelvin));
        self.announce();
    }

    fn disable_temperature(&self) {
        self.night.set(false);
        self.color.set_temperature(None);
        self.announce();
    }

    pub fn toggle_night(&self) {
        self.toggle_temperature(None);
    }

    pub fn toggle_temperature(&self, active: Option<bool>) {
        let schedule = &self.schedule;
        let (previous, since) = schedule
            .manual
            .get()
            .unwrap_or((self.night.get(), now_minutes()));
        let wanted = active.unwrap_or(!previous);
        schedule.manual.set(Some((wanted, since)));
        if wanted {
            self.enable_temperature(self.temperature.get());
        } else {
            self.disable_temperature();
        }
    }

    pub fn set_automatic(&self, on: bool) {
        self.automatic.set(on);
        Config::store_night("automatic", serde_json::Value::Bool(on));
        self.schedule.manual.set(None);
        self.schedule.first.set(true);
        self.re_evaluate();
        self.announce();
    }

    pub fn set_temperature(&self, kelvin: i32) {
        self.temperature.set(kelvin);
        Config::store_night("colorTemperature", serde_json::Value::from(kelvin));
        if self.follows_schedule() {
            self.re_evaluate();
        } else if self.night.get() {
            self.color.set_temperature(Some(kelvin));
        }
        self.announce();
    }

    pub fn toggle_awake(&self) {
        let mut held = self.inhibitor.borrow_mut();
        match held.take() {
            Some(mut running) => {
                running.stop();
                self.awake.set(false);
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
                let started = process::start(command);
                self.awake.set(started.is_some());
                *held = started;
            }
        }
        drop(held);
        persistent::write(&INHIBIT, self.awake.get().into());
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

    pub fn toggle_dark(&self) {
        let mode = if self.dark.get() { "light" } else { "dark" };
        crate::theming::switchwall::detach(&["--mode", mode, "--noswitch"]);
    }

    fn watch_mode(&self) {
        let session = self.clone();
        let monitor = watch_dark_mode(move |now| {
            if now != session.dark.get() {
                session.dark.set(now);
                session.announce();
            }
        });
        std::mem::forget(monitor);
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

fn palette_path() -> std::path::PathBuf {
    crate::core::paths::generated().join("material_colors.scss")
}

pub fn watch_dark_mode(changed: impl Fn(bool) + 'static) -> Option<gio::FileMonitor> {
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
            changed(dark_mode());
        }
    });
    Some(monitor)
}

pub fn dark_mode() -> bool {
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
    let Some(clients) =
        crate::platform::hypr::json("clients").and_then(|value| value.as_array().cloned())
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
            assert_eq!(scheduled(now, &night), expected, "at {now} s");
        }
        let instant = Night {
            transition: 0,
            ..night
        };
        assert_eq!(scheduled(clock(19, 0), &instant), Some(4000));
        assert_eq!(scheduled(clock(6, 30), &instant), None);
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

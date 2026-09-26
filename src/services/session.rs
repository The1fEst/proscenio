use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::detach;
use crate::core::{persistent, watch};

#[derive(Clone)]
pub struct Session {
    pub night: Rc<Cell<bool>>,
    pub awake: Rc<Cell<bool>>,
    pub dark: Rc<Cell<bool>>,
    pub automatic: Rc<Cell<bool>>,
    pub temperature: Rc<Cell<i32>>,
    inhibitor: Rc<RefCell<Option<gio::Subprocess>>>,
    schedule: Rc<Schedule>,
    listeners: Rc<Listeners>,
    following: Rc<RefCell<Option<watch::Watch>>>,
}

struct Schedule {
    from: Cell<i32>,
    to: Cell<i32>,
    manual: Cell<Option<(bool, i32)>>,
    should_be_on: Cell<Option<bool>>,
    first: Cell<bool>,
    minute: Cell<i32>,
}

const DEFAULT_TEMPERATURE: i32 = 6000;
const INHIBIT: [&str; 2] = ["idle", "inhibit"];

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

fn between(time: i32, from: i32, to: i32) -> bool {
    if from < to {
        time >= from && time <= to
    } else {
        time >= from || time <= to
    }
}

impl Session {
    pub fn new(config: &Config) -> Self {
        let session = Session {
            night: Rc::new(Cell::new(false)),
            awake: Rc::new(Cell::new(false)),
            dark: Rc::new(Cell::new(dark_mode())),
            automatic: Rc::new(Cell::new(config.night_automatic)),
            temperature: Rc::new(Cell::new(config.night_temperature)),
            inhibitor: Rc::new(RefCell::new(None)),
            schedule: Rc::new(Schedule {
                from: Cell::new(minutes(&config.night_from)),
                to: Cell::new(minutes(&config.night_to)),
                manual: Cell::new(None),
                should_be_on: Cell::new(None),
                first: Cell::new(true),
                minute: Cell::new(-1),
            }),
            listeners: Rc::default(),
            following: Rc::default(),
        };
        session.read_night();
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
        let rescheduled = schedule.from.replace(from) != from || schedule.to.replace(to) != to;
        let switched = self.automatic.replace(config.night_automatic) != config.night_automatic;
        if self.temperature.replace(config.night_temperature) != config.night_temperature
            && self.night.get()
        {
            detach(&[
                "hyprctl",
                "hyprsunset",
                "temperature",
                &config.night_temperature.to_string(),
            ]);
        }
        if rescheduled || switched {
            schedule.manual.set(None);
            schedule.first.set(true);
            self.re_evaluate();
        }
        self.announce();
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn follow_clock(&self) {
        let now = now_minutes();
        if self.schedule.minute.replace(now) != now {
            self.re_evaluate();
        }
    }

    fn re_evaluate(&self) {
        let schedule = &self.schedule;
        let now = now_minutes();
        schedule.minute.set(now);
        if let Some((_, since)) = schedule.manual.get()
            && (between(schedule.from.get(), since, now) || between(schedule.to.get(), since, now))
        {
            schedule.manual.set(None);
        }
        let should = between(now, schedule.from.get(), schedule.to.get());
        let changed = schedule.should_be_on.replace(Some(should)) != Some(should);
        if schedule.first.replace(false) || changed {
            self.ensure_state();
        }
    }

    fn ensure_state(&self) {
        let schedule = &self.schedule;
        if !self.automatic.get() || schedule.manual.get().is_some() {
            return;
        }
        if schedule.should_be_on.get() == Some(true) {
            self.enable_temperature();
        } else {
            self.disable_temperature();
        }
    }

    fn enable_temperature(&self) {
        self.night.set(true);
        let kelvin = self.temperature.get().to_string();
        detach(&[
            "bash",
            "-c",
            "if pidof hyprsunset >/dev/null; then hyprctl hyprsunset temperature \"$1\"; else hyprsunset -t \"$1\"; fi",
            "night",
            &kelvin,
        ]);
        self.announce();
    }

    fn disable_temperature(&self) {
        self.night.set(false);
        detach(&[
            "hyprctl",
            "hyprsunset",
            "temperature",
            &DEFAULT_TEMPERATURE.to_string(),
        ]);
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
            self.enable_temperature();
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
        if self.night.get() {
            detach(&["hyprctl", "hyprsunset", "temperature", &kelvin.to_string()]);
        }
        self.announce();
    }

    pub fn toggle_awake(&self) {
        let mut held = self.inhibitor.borrow_mut();
        match held.take() {
            Some(process) => {
                process.force_exit();
                self.awake.set(false);
            }
            None => {
                let started = gio::Subprocess::newv(
                    &[
                        "systemd-inhibit",
                        "--what=idle:sleep",
                        "--who=proscenio",
                        "--why=Keep awake",
                        "cat",
                    ]
                    .map(std::ffi::OsStr::new),
                    gio::SubprocessFlags::STDIN_PIPE
                        | gio::SubprocessFlags::STDOUT_SILENCE
                        | gio::SubprocessFlags::STDERR_SILENCE,
                )
                .ok();
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
        detach(&["pkill", "-i", "Hyprland"]);
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

    fn read_night(&self) {
        let session = self.clone();
        let Ok(process) = gio::Subprocess::newv(
            &["hyprctl", "hyprsunset", "temperature"].map(std::ffi::OsStr::new),
            gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
        ) else {
            return;
        };
        glib::spawn_future_local(async move {
            if let Ok((Some(output), _)) = process.communicate_utf8_future(None).await
                && let Ok(value) = output.trim().parse::<i32>()
            {
                session.night.set(value != DEFAULT_TEMPERATURE);
                session.announce();
            }
        });
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

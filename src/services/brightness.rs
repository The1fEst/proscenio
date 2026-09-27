use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hypr;

pub const GAMMA_FLOOR: f64 = 25.0;
const STEP: f64 = 0.05;
const DDC_DELAY: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, PartialEq)]
pub enum Change {
    Read,
    Level,
    Gamma,
}

struct Screen {
    name: String,
    bus: Option<String>,
    max: Cell<f64>,
    level: Cell<f64>,
    ready: Cell<bool>,
    pending: RefCell<Option<glib::SourceId>>,
}

#[derive(Clone)]
pub struct Light {
    screens: Rc<RefCell<Vec<Rc<Screen>>>>,
    names: Rc<RefCell<Vec<String>>>,
    pub gamma: Rc<Cell<f64>>,
    writer: Rc<RefCell<Option<gio::Subprocess>>>,
    generation: Rc<Cell<u64>>,
    listeners: Rc<Listeners<Change>>,
}

impl Light {
    pub fn new() -> Self {
        Light {
            screens: Rc::default(),
            names: Rc::default(),
            gamma: Rc::new(Cell::new(100.0)),
            writer: Rc::default(),
            generation: Rc::default(),
            listeners: Rc::default(),
        }
    }

    pub fn subscribe(&self, listener: impl Fn(Change) + 'static) -> Subscription {
        self.listeners.add_with(move |change| listener(*change))
    }

    fn announce(&self, change: Change) {
        self.listeners.notify_with(&change);
    }

    pub fn set_screens(&self, names: Vec<String>) {
        if *self.names.borrow() == names {
            return;
        }
        self.names.replace(names.clone());
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        for screen in self.screens.take() {
            if let Some(pending) = screen.pending.take() {
                pending.remove();
            }
        }
        let light = self.clone();
        read(&["ddcutil", "detect", "--brief"], move |output| {
            if light.generation.get() != generation {
                return;
            }
            let detected = ddc_displays(&output);
            let mut taken: Vec<String> = Vec::new();
            let screens: Vec<Rc<Screen>> = names
                .iter()
                .map(|name| {
                    let bus = detected
                        .iter()
                        .find(|(connector, bus)| connector == name && !taken.contains(bus))
                        .map(|(_, bus)| bus.clone());
                    if let Some(bus) = &bus {
                        taken.push(bus.clone());
                    }
                    Rc::new(Screen {
                        name: name.clone(),
                        bus,
                        max: Cell::new(100.0),
                        level: Cell::new(0.0),
                        ready: Cell::new(false),
                        pending: RefCell::new(None),
                    })
                })
                .collect();
            light.screens.replace(screens.clone());
            light.initialize(screens, generation);
        });
    }

    fn initialize(&self, mut rest: Vec<Rc<Screen>>, generation: u64) {
        if rest.is_empty() {
            return;
        }
        let screen = rest.remove(0);
        let light = self.clone();
        self.read_level(&screen, move || {
            if light.generation.get() == generation {
                light.initialize(rest, generation);
            }
        });
    }

    fn read_level(&self, screen: &Rc<Screen>, then: impl FnOnce() + 'static) {
        let command: Vec<String> = match &screen.bus {
            Some(bus) => ["ddcutil", "-b", bus, "getvcp", "10", "--brief"]
                .iter()
                .map(|part| part.to_string())
                .collect(),
            None => [
                "sh",
                "-c",
                "echo a b c $(brightnessctl g) $(brightnessctl m)",
            ]
            .iter()
            .map(|part| part.to_string())
            .collect(),
        };
        let borrowed: Vec<&str> = command.iter().map(String::as_str).collect();
        let light = self.clone();
        let screen = screen.clone();
        let then = RefCell::new(Some(then));
        read(&borrowed, move |output| {
            let fields: Vec<&str> = output.split_whitespace().collect();
            if let (Some(current), Some(max)) = (
                fields.get(3).and_then(|value| value.parse::<f64>().ok()),
                fields.get(4).and_then(|value| value.parse::<f64>().ok()),
            ) && max > 0.0
            {
                let level = current / max;
                let moved = screen.ready.get() && screen.level.get() != level;
                screen.max.set(max);
                screen.level.set(level);
                screen.ready.set(true);
                light.announce(if moved { Change::Level } else { Change::Read });
            }
            if let Some(then) = then.take() {
                then();
            }
        });
    }

    fn screen(&self, name: &str) -> Option<Rc<Screen>> {
        self.screens
            .borrow()
            .iter()
            .find(|screen| screen.name == name)
            .cloned()
    }

    pub fn level(&self, name: &str) -> f64 {
        self.screen(name).map_or(0.0, |screen| screen.level.get())
    }

    pub fn set_level(&self, name: &str, part: f64) {
        let Some(screen) = self.screen(name) else {
            return;
        };
        screen.level.set(part.clamp(0.0, 1.0));
        if !screen.ready.get() {
            return;
        }
        self.announce(Change::Level);
        if let Some(pending) = screen.pending.take() {
            pending.remove();
        }
        let delay = if screen.bus.is_some() {
            DDC_DELAY
        } else {
            Duration::ZERO
        };
        let light = self.clone();
        let target = screen.clone();
        let pending = glib::timeout_add_local_once(delay, move || {
            target.pending.take();
            light.write(&target);
        });
        screen.pending.replace(Some(pending));
    }

    fn write(&self, screen: &Screen) {
        let level = screen.level.get().max(0.0);
        let command: Vec<String> = match &screen.bus {
            Some(bus) => vec![
                "ddcutil".into(),
                "-b".into(),
                bus.clone(),
                "setvcp".into(),
                "10".into(),
                ((level * screen.max.get()).floor() as i64)
                    .max(1)
                    .to_string(),
            ],
            None => {
                let percent = (level * 100.0).floor() as i64;
                vec![
                    "brightnessctl".into(),
                    "--class".into(),
                    "backlight".into(),
                    "s".into(),
                    if percent == 0 {
                        "1".into()
                    } else {
                        format!("{percent}%")
                    },
                    "--quiet".into(),
                ]
            }
        };
        if let Some(previous) = self.writer.take() {
            previous.force_exit();
        }
        let started = gio::Subprocess::newv(
            &command.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>(),
            gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
        );
        self.writer.replace(started.ok());
    }

    fn focused(&self) -> Option<String> {
        hypr::focused_monitor()
    }

    pub fn raise(&self) {
        if self.gamma.get() != 100.0 {
            self.set_gamma(self.gamma.get() + 5.0);
            return;
        }
        if let Some(name) = self.focused() {
            self.set_level(&name, self.level(&name) + STEP);
        }
    }

    pub fn lower(&self) {
        let name = self.focused();
        match name.filter(|name| self.level(name) > 0.0) {
            Some(name) => self.set_level(&name, self.level(&name) - STEP),
            None => self.set_gamma(self.gamma.get() - 5.0),
        }
    }

    pub fn set_gamma(&self, percent: f64) {
        let percent = percent.clamp(GAMMA_FLOOR, 100.0).round();
        self.gamma.set(percent);
        let _ = gio::Subprocess::newv(
            &["hyprctl", "hyprsunset", "gamma", &percent.to_string()].map(std::ffi::OsStr::new),
            gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
        );
        self.announce(Change::Gamma);
    }
}

fn ddc_displays(output: &str) -> Vec<(String, String)> {
    output
        .split("\n\n")
        .filter(|block| block.starts_with("Display "))
        .filter_map(|block| {
            let field = |key: &str| {
                block
                    .lines()
                    .map(str::trim)
                    .find_map(|line| line.strip_prefix(key))
                    .map(str::trim)
            };
            let connector = field("DRM connector:")?.split_once('-')?.1.to_owned();
            let bus = field("I2C bus:")?.strip_prefix("/dev/i2c-")?.to_owned();
            Some((connector, bus))
        })
        .collect()
}

fn read(command: &[&str], handler: impl Fn(String) + 'static) {
    let process = gio::Subprocess::newv(
        &command.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>(),
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    );
    glib::spawn_future_local(async move {
        let output = match process {
            Ok(process) => process
                .communicate_utf8_future(None)
                .await
                .ok()
                .and_then(|(output, _)| output)
                .map(|output| output.to_string())
                .unwrap_or_default(),
            Err(_) => String::new(),
        };
        handler(output);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddcutil_blocks_give_each_display_its_connector_and_bus() {
        let output = "Display 1\n   I2C bus:          /dev/i2c-7\n   DRM connector:    card1-DP-1\n   drm_connector_id: 133\n   Monitor:          PHL:27M1N5500ZA:UHB2335025229\n\nInvalid display\n   I2C bus:  /dev/i2c-4\n\nDisplay 2\n   I2C bus:          /dev/i2c-8\n   DRM connector:    card1-DP-2\n";
        assert_eq!(
            ddc_displays(output),
            vec![
                ("DP-1".to_owned(), "7".to_owned()),
                ("DP-2".to_owned(), "8".to_owned())
            ]
        );
    }
}

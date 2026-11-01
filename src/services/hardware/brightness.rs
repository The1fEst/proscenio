use gtk4::{gio, glib};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::hyprland::hypr;
use crate::platform::system::backlight::{self, Device};
use crate::platform::system::udev;
use crate::platform::wayland::readable;
use crate::services::hyprland::screencolor::ScreenColor;
use crate::ui::anim::EXPRESSIVE_EFFECTS;

pub const GAMMA_FLOOR: f64 = 25.0;
const STEP: f64 = 0.05;
const DDC_DELAY: Duration = Duration::from_millis(300);
const FADE_MICROS: f64 = 200_000.0;
const FADE_FRAME: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, PartialEq)]
pub enum Change {
    Read,
    Level,
    Gamma,
}

enum Control {
    Ddc(String),
    Backlight,
}

struct Screen {
    name: String,
    control: Option<Control>,
    max: Cell<f64>,
    level: Cell<f64>,
    applied_level: Cell<f64>,
    is_ready: Cell<bool>,
    write_timer: RefCell<Option<glib::SourceId>>,
}

#[derive(Clone)]
pub struct Light {
    screens: Rc<RefCell<Vec<Rc<Screen>>>>,
    names: Rc<RefCell<Vec<String>>>,
    pub gamma: Rc<Cell<f64>>,
    color: Rc<ScreenColor>,
    system: Option<gio::DBusConnection>,
    backlight: Option<Rc<Device>>,
    pending_backlight_writes: Rc<Cell<u32>>,
    writer: Rc<RefCell<Option<process::Running>>>,
    generation: Rc<Cell<u64>>,
    listeners: Rc<Listeners<Change>>,
}

impl Light {
    pub fn new(color: &Rc<ScreenColor>, system: Option<gio::DBusConnection>) -> Self {
        let light = Light {
            screens: Rc::default(),
            names: Rc::default(),
            gamma: Rc::new(Cell::new(100.0)),
            color: color.clone(),
            backlight: system
                .as_ref()
                .and_then(|_| backlight::find_screen_backlight(Path::new(backlight::SYSFS_CLASS)))
                .map(Rc::new),
            system,
            pending_backlight_writes: Rc::default(),
            writer: Rc::default(),
            generation: Rc::default(),
            listeners: Rc::default(),
        };
        light.watch_backlight();
        light
    }

    pub fn subscribe(&self, listener: impl Fn(Change) + 'static) -> Subscription {
        self.listeners.add_with(move |change| listener(*change))
    }

    fn announce(&self, change: Change) {
        self.listeners.notify_with(&change);
    }

    fn watch_backlight(&self) {
        if self.backlight.is_none() {
            return;
        }
        let Some(monitor) = udev::Monitor::for_subsystem(backlight::BACKLIGHT) else {
            return;
        };
        let light = self.clone();
        readable::add_watch(monitor.fd(), move || {
            if monitor.receive().is_some() {
                light.follow_backlight();
            }
            glib::ControlFlow::Continue
        });
    }

    fn follow_backlight(&self) {
        let Some(device) = &self.backlight else {
            return;
        };
        if self.pending_backlight_writes.get() > 0 {
            return;
        }
        let screens: Vec<Rc<Screen>> = self
            .screens
            .borrow()
            .iter()
            .filter(|screen| {
                matches!(screen.control, Some(Control::Backlight))
                    && screen.is_ready.get()
                    && screen.write_timer.borrow().is_none()
            })
            .cloned()
            .collect();
        if screens.is_empty() {
            return;
        }
        let (Some(brightness), Some(actual_brightness)) =
            (device.read_brightness(), device.read_actual_brightness())
        else {
            return;
        };
        for screen in screens {
            if brightness != scale_to_raw(screen.level.get(), screen.max.get()) {
                self.accept_reading(&screen, f64::from(actual_brightness), screen.max.get());
            }
        }
    }

    pub fn set_screens(&self, names: Vec<String>) {
        if *self.names.borrow() == names {
            return;
        }
        self.names.replace(names.clone());
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        for screen in self.screens.take() {
            if let Some(source) = screen.write_timer.take() {
                source.remove();
            }
        }
        let light = self.clone();
        read(&["ddcutil", "detect", "--brief"], move |output| {
            if light.generation.get() != generation {
                return;
            }
            let displays = ddc_displays(&output);
            let mut used_buses: Vec<String> = Vec::new();
            let screens: Vec<Rc<Screen>> = names
                .iter()
                .map(|name| {
                    let bus = displays
                        .iter()
                        .find(|(connector, bus)| connector == name && !used_buses.contains(bus))
                        .map(|(_, bus)| bus.clone());
                    if let Some(bus) = &bus {
                        used_buses.push(bus.clone());
                    }
                    Rc::new(Screen {
                        name: name.clone(),
                        control: bus
                            .map(Control::Ddc)
                            .or_else(|| light.backlight.as_ref().map(|_| Control::Backlight)),
                        max: Cell::new(100.0),
                        level: Cell::new(0.0),
                        applied_level: Cell::new(0.0),
                        is_ready: Cell::new(false),
                        write_timer: RefCell::new(None),
                    })
                })
                .collect();
            light.screens.replace(screens.clone());
            light.initialize(screens, generation);
        });
    }

    fn initialize(&self, mut remaining_screens: Vec<Rc<Screen>>, generation: u64) {
        if remaining_screens.is_empty() {
            return;
        }
        let screen = remaining_screens.remove(0);
        let light = self.clone();
        self.read_level(&screen, move || {
            if light.generation.get() == generation {
                light.initialize(remaining_screens, generation);
            }
        });
    }

    fn read_level(&self, screen: &Rc<Screen>, on_done: impl FnOnce() + 'static) {
        match &screen.control {
            Some(Control::Ddc(bus)) => {
                let light = self.clone();
                let screen = screen.clone();
                let on_done = RefCell::new(Some(on_done));
                read(
                    &["ddcutil", "-b", bus.as_str(), "getvcp", "10", "--brief"],
                    move |output| {
                        let fields: Vec<&str> = output.split_whitespace().collect();
                        if let (Some(current), Some(max)) = (
                            fields.get(3).and_then(|value| value.parse::<f64>().ok()),
                            fields.get(4).and_then(|value| value.parse::<f64>().ok()),
                        ) && max > 0.0
                        {
                            light.accept_reading(&screen, current, max);
                        }
                        if let Some(on_done) = on_done.take() {
                            on_done();
                        }
                    },
                );
            }
            Some(Control::Backlight) => {
                if let Some(device) = &self.backlight
                    && let (Some(current), Some(max)) = (
                        device.read_actual_brightness(),
                        device.read_max_brightness(),
                    )
                {
                    self.accept_reading(screen, f64::from(current), f64::from(max));
                }
                on_done();
            }
            None => on_done(),
        }
    }

    fn accept_reading(&self, screen: &Screen, current: f64, max: f64) {
        let level = current / max;
        let has_level_changed = screen.is_ready.get() && screen.level.get() != level;
        screen.max.set(max);
        screen.level.set(level);
        screen.applied_level.set(level);
        screen.is_ready.set(true);
        self.announce(if has_level_changed {
            Change::Level
        } else {
            Change::Read
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
        if !screen.is_ready.get() {
            return;
        }
        self.announce(Change::Level);
        if let Some(source) = screen.write_timer.take() {
            source.remove();
        }
        let light = self.clone();
        let target = screen.clone();
        let source = if matches!(screen.control, Some(Control::Ddc(_))) {
            glib::timeout_add_local_once(DDC_DELAY, move || {
                target.write_timer.take();
                light.write(&target, target.level.get());
            })
        } else {
            let start_level = screen.applied_level.get();
            let start_time = glib::monotonic_time();
            glib::timeout_add_local(FADE_FRAME, move || {
                let part = ((glib::monotonic_time() - start_time) as f64 / FADE_MICROS).min(1.0);
                let target_level = target.level.get();
                let level =
                    start_level + (target_level - start_level) * EXPRESSIVE_EFFECTS.at(part);
                let max = target.max.get();
                if scale_to_raw(level, max) != scale_to_raw(target.applied_level.get(), max) {
                    light.write(&target, level);
                }
                target.applied_level.set(level);
                if part < 1.0 {
                    return glib::ControlFlow::Continue;
                }
                target.write_timer.take();
                glib::ControlFlow::Break
            })
        };
        screen.write_timer.replace(Some(source));
    }

    fn write(&self, screen: &Screen, level: f64) {
        match &screen.control {
            Some(Control::Ddc(bus)) => {
                let value = ((level.max(0.0) * screen.max.get()).floor() as i64).max(1);
                let command = [
                    "ddcutil".to_owned(),
                    "-b".to_owned(),
                    bus.clone(),
                    "setvcp".to_owned(),
                    "10".to_owned(),
                    value.to_string(),
                ];
                if let Some(mut previous) = self.writer.take() {
                    previous.stop();
                }
                self.writer
                    .replace(process::start(process::quiet(&command)));
            }
            Some(Control::Backlight) => {
                let (Some(device), Some(system)) = (&self.backlight, &self.system) else {
                    return;
                };
                let pending_writes = self.pending_backlight_writes.clone();
                pending_writes.set(pending_writes.get() + 1);
                device.set_brightness(system, scale_to_raw(level, screen.max.get()), move || {
                    pending_writes.set(pending_writes.get() - 1);
                });
            }
            None => {}
        }
    }

    fn focused_monitor(&self) -> Option<String> {
        hypr::focused_monitor()
    }

    pub fn raise(&self) {
        if self.gamma.get() != 100.0 {
            self.set_gamma(self.gamma.get() + 5.0);
            return;
        }
        if let Some(name) = self.focused_monitor() {
            self.set_level(&name, self.level(&name) + STEP);
        }
    }

    pub fn lower(&self) {
        let name = self.focused_monitor();
        match name.filter(|name| self.level(name) > 0.0) {
            Some(name) => self.set_level(&name, self.level(&name) - STEP),
            None => self.set_gamma(self.gamma.get() - 5.0),
        }
    }

    pub fn set_gamma(&self, percent: f64) {
        let percent = percent.clamp(GAMMA_FLOOR, 100.0).round();
        self.gamma.set(percent);
        self.color.set_gamma(percent);
        self.announce(Change::Gamma);
    }
}

fn scale_to_raw(level: f64, max: f64) -> u32 {
    (level.clamp(0.0, 1.0) * max).round().max(1.0) as u32
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

fn read(command: &[&str], on_output: impl Fn(String) + 'static) {
    let command = process::command(command);
    glib::spawn_future_local(async move {
        on_output(process::capture_text(command).await.unwrap_or_default());
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

    #[test]
    fn a_backlight_level_maps_to_the_nearest_raw_value_and_never_to_zero() {
        let values: Vec<u32> = [0.0, 0.004, 0.5, 0.537, 1.0, 1.4]
            .iter()
            .map(|level| scale_to_raw(*level, 255.0))
            .collect();
        assert_eq!(values, vec![1, 1, 128, 137, 255, 255]);
    }
}

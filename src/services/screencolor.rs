use gtk4::glib;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::listeners::Subscription;
use crate::core::process;
use crate::platform::ctm::{Change, CtmControl};
use crate::platform::hypr::{self, Events};

pub const NEUTRAL_TEMPERATURE: i32 = 6600;
const SDR_GAMMA: f64 = 2.2;
const SDR_WHITE: f64 = 80.0;
const PQ_PEAK: f64 = 10000.0;
const REBIND_DELAY: Duration = Duration::from_secs(1);
const REBIND_ATTEMPTS: u32 = 3;
const MONITOR_EVENTS: [&str; 3] = ["configreloaded", "monitoradded", "monitoraddedv2"];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Transfer {
    Gamma,
    Pq { white: f64 },
}

fn kelvin_scale(kelvin: i32) -> [f64; 3] {
    if kelvin >= NEUTRAL_TEMPERATURE {
        return [1.0; 3];
    }
    let temperature = f64::from(kelvin.max(1000) / 100);
    let green = 99.4708025861 * temperature.ln() - 161.1195681661;
    let blue = if temperature <= 19.0 {
        0.0
    } else {
        (temperature - 10.0).ln() * 138.5177312231 - 305.0447927307
    };
    [255.0, green, blue].map(|channel| (channel / 255.0).clamp(0.0, 1.0))
}

fn pq(nits: f64) -> f64 {
    const M1: f64 = 0.1593017578125;
    const M2: f64 = 78.84375;
    const C1: f64 = 0.8359375;
    const C2: f64 = 18.8515625;
    const C3: f64 = 18.6875;
    let level = (nits / PQ_PEAK).max(0.0).powf(M1);
    ((C1 + C2 * level) / (1.0 + C3 * level)).powf(M2)
}

fn encode(scale: f64, transfer: Transfer) -> f64 {
    match transfer {
        Transfer::Gamma => scale,
        Transfer::Pq { white } => pq(scale.powf(SDR_GAMMA) * white) / pq(white),
    }
}

fn transfer_of(monitor: &Value) -> Transfer {
    let preset = monitor
        .get("colorManagementPreset")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !matches!(preset, "hdr" | "hdredid") {
        return Transfer::Gamma;
    }
    let number = |key: &str| monitor.get(key).and_then(Value::as_f64);
    let white = number("sdrMaxLuminance")
        .filter(|nits| *nits >= 0.0)
        .unwrap_or(SDR_WHITE);
    let brightness = number("sdrBrightness")
        .filter(|brightness| *brightness > 0.0)
        .unwrap_or(1.0);
    Transfer::Pq {
        white: white * brightness,
    }
}

pub struct ScreenColor {
    temperature: Cell<Option<i32>>,
    gamma: Cell<f64>,
    control: RefCell<Option<Rc<CtmControl>>>,
    attempts: Cell<u32>,
    following: RefCell<Option<Subscription>>,
}

impl ScreenColor {
    pub fn new(events: &Events) -> Rc<Self> {
        let color = Rc::new(ScreenColor {
            temperature: Cell::new(None),
            gamma: Cell::new(100.0),
            control: RefCell::new(None),
            attempts: Cell::new(0),
            following: RefCell::new(None),
        });
        let weak = Rc::downgrade(&color);
        color.control.replace(CtmControl::connect({
            let weak = weak.clone();
            move |change| {
                if let Some(color) = weak.upgrade() {
                    color.changed(change);
                }
            }
        }));
        color.following.replace(Some(events.subscribe({
            let weak = weak.clone();
            move |event, _| {
                if MONITOR_EVENTS.contains(&event)
                    && let Some(color) = weak.upgrade()
                {
                    color.apply();
                }
            }
        })));
        color.apply();
        color
    }

    pub fn set_temperature(&self, kelvin: Option<i32>) {
        self.temperature.set(kelvin);
        self.apply();
    }

    pub fn set_gamma(&self, percent: f64) {
        self.gamma.set(percent);
        self.apply();
    }

    fn changed(self: &Rc<Self>, change: Change) {
        match change {
            Change::Outputs => self.apply(),
            Change::Blocked => self.take_over(),
        }
    }

    fn take_over(self: &Rc<Self>) {
        let attempt = self.attempts.get() + 1;
        if attempt > REBIND_ATTEMPTS {
            return;
        }
        self.attempts.set(attempt);
        process::run(&["pkill", "-x", "hyprsunset"]);
        let weak: Weak<Self> = Rc::downgrade(self);
        glib::timeout_add_local_once(REBIND_DELAY, move || {
            let Some(color) = weak.upgrade() else {
                return;
            };
            if let Some(control) = color.control.borrow().clone() {
                control.rebind();
            }
            color.apply();
        });
    }

    fn scale(&self) -> [f64; 3] {
        let gamma = self.gamma.get() / 100.0;
        self.temperature
            .get()
            .map(kelvin_scale)
            .unwrap_or([1.0; 3])
            .map(|channel| channel * gamma)
    }

    fn apply(&self) {
        let Some(control) = self.control.borrow().clone() else {
            return;
        };
        let monitors = hypr::json("monitors")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let scale = self.scale();
        control.set(|name| {
            let transfer = monitors
                .iter()
                .find(|monitor| monitor.get("name").and_then(Value::as_str) == Some(name))
                .map(transfer_of)
                .unwrap_or(Transfer::Gamma);
            scale.map(|channel| encode(channel, transfer))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(signal: f64) -> f64 {
        const M1: f64 = 0.1593017578125;
        const M2: f64 = 78.84375;
        const C1: f64 = 0.8359375;
        const C2: f64 = 18.8515625;
        const C3: f64 = 18.6875;
        let power = signal.powf(1.0 / M2);
        PQ_PEAK * ((power - C1).max(0.0) / (C2 - C3 * power)).powf(1.0 / M1)
    }

    fn rounded(channels: [f64; 3]) -> [f64; 3] {
        channels.map(|channel| (channel * 1000.0).round() / 1000.0)
    }

    #[test]
    fn the_temperature_scale_follows_the_blackbody_fit_and_is_neutral_by_day() {
        assert_eq!(rounded(kelvin_scale(5000)), [1.0, 0.894, 0.808]);
        assert_eq!(rounded(kelvin_scale(3000)), [1.0, 0.695, 0.431]);
        assert_eq!(kelvin_scale(NEUTRAL_TEMPERATURE), [1.0; 3]);
        assert_eq!(kelvin_scale(9000), [1.0; 3]);
    }

    #[test]
    fn an_hdr_monitor_shows_the_tint_an_sdr_monitor_shows_at_its_sdr_white() {
        let white = 200.0;
        for scale in kelvin_scale(5000).into_iter().chain([0.25, 0.6, 1.0]) {
            let encoded = encode(scale, Transfer::Pq { white });
            let shown = luminance(encoded * pq(white)) / white;
            let sdr = encode(scale, Transfer::Gamma).powf(SDR_GAMMA);
            assert!((shown - sdr).abs() < 1e-6, "{scale}: {shown} against {sdr}");
        }
        assert_eq!(encode(1.0, Transfer::Pq { white }), 1.0);
    }

    #[test]
    fn the_transfer_comes_from_the_monitors_color_preset_and_sdr_white() {
        let monitor = |json: &str| transfer_of(&serde_json::from_str(json).unwrap());
        let cases = [
            (
                r#"{"colorManagementPreset":"srgb","sdrMaxLuminance":200}"#,
                Transfer::Gamma,
            ),
            (r#"{"colorManagementPreset":"wide"}"#, Transfer::Gamma),
            (
                r#"{"colorManagementPreset":"hdredid","sdrMaxLuminance":200,"sdrBrightness":1}"#,
                Transfer::Pq { white: 200.0 },
            ),
            (
                r#"{"colorManagementPreset":"hdr","sdrMaxLuminance":250,"sdrBrightness":1.2}"#,
                Transfer::Pq { white: 300.0 },
            ),
            (
                r#"{"colorManagementPreset":"hdr","sdrMaxLuminance":-1,"sdrBrightness":0}"#,
                Transfer::Pq { white: SDR_WHITE },
            ),
            ("{}", Transfer::Gamma),
        ];
        for (json, expected) in cases {
            assert_eq!(monitor(json), expected, "{json}");
        }
    }
}

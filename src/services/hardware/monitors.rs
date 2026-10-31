use serde_json::Value;
use std::cmp::Ordering;

use crate::platform::hyprland::hypr;

const CONNECTOR_PREFERENCE: [(&str, u32); 6] = [
    ("DP", 0),
    ("HDMI", 1),
    ("DVI", 2),
    ("eDP", 3),
    ("LVDS", 4),
    ("VGA", 5),
];
const SCALE_DIVISORS: [f64; 6] = [1.0, 1.25, 4.0 / 3.0, 1.5, 1.6, 2.0];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub model: String,
    pub width: i64,
    pub height: i64,
    pub x: i64,
    pub y: i64,
    pub scale: f64,
    pub transform: i64,
    pub refresh_rate: f64,
    pub mirror_of: String,
    pub current_format: String,
    pub color_preset: String,
    pub available_modes: Vec<String>,
    pub is_disabled: bool,
    pub active_workspace_id: i64,
    pub is_hdr_capable: Option<bool>,
    pub is_vrr_capable: Option<bool>,
}

impl Monitor {
    fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let int = |key: &str| value.get(key).and_then(Value::as_i64).unwrap_or(0);
        let hardware_flag = |key: &str| {
            value
                .get("hardwareDetails")
                .and_then(|details| details.get(key))
                .and_then(Value::as_bool)
        };
        Some(Monitor {
            id: int("id"),
            name: value.get("name")?.as_str()?.to_owned(),
            model: text("model"),
            width: int("width"),
            height: int("height"),
            x: int("x"),
            y: int("y"),
            scale: value.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
            transform: int("transform"),
            refresh_rate: value
                .get("refreshRate")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            mirror_of: value
                .get("mirrorOf")
                .map(|mirror| match mirror {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "none".to_owned()),
            current_format: text("currentFormat"),
            color_preset: text("colorManagementPreset"),
            available_modes: value
                .get("availableModes")
                .and_then(Value::as_array)
                .map(|modes| {
                    modes
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            is_disabled: value
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            active_workspace_id: value
                .get("activeWorkspace")
                .and_then(|workspace| workspace.get("id"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
            is_hdr_capable: hardware_flag("hdr")
                .zip(hardware_flag("bt2020"))
                .map(|(has_hdr, has_bt2020)| has_hdr && has_bt2020),
            is_vrr_capable: hardware_flag("vrrCapable"),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mode {
    pub width: i64,
    pub height: i64,
    pub rates: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShownMode {
    pub width: i64,
    pub height: i64,
    pub mode: Mode,
    pub scale: f64,
    pub rate: Option<f64>,
    pub is_native: bool,
}

pub fn modes_of(monitor: &Monitor) -> Vec<Mode> {
    let mut modes: Vec<Mode> = Vec::new();
    for mode in &monitor.available_modes {
        let Some((size, rate)) = mode
            .strip_suffix("Hz")
            .and_then(|mode| mode.split_once('@'))
        else {
            continue;
        };
        let Some((width, height)) = size.split_once('x') else {
            continue;
        };
        let (Ok(width), Ok(height), Ok(rate)) = (
            width.parse::<i64>(),
            height.parse::<i64>(),
            rate.parse::<f64>(),
        ) else {
            continue;
        };
        match modes
            .iter_mut()
            .find(|known| known.width == width && known.height == height)
        {
            None => modes.push(Mode {
                width,
                height,
                rates: vec![rate],
            }),
            Some(known) if !known.rates.contains(&rate) => known.rates.push(rate),
            Some(_) => {}
        }
    }
    let is_listed = modes
        .iter()
        .any(|mode| mode.width == monitor.width && mode.height == monitor.height);
    if !is_listed && monitor.width > 0 && monitor.height > 0 {
        modes.push(Mode {
            width: monitor.width,
            height: monitor.height,
            rates: vec![monitor.refresh_rate],
        });
    }
    modes.sort_by_key(|mode| std::cmp::Reverse(mode.width * mode.height));
    modes
}

fn scaled_modes_of(monitor: &Monitor) -> Vec<ShownMode> {
    let Some(native_mode) = modes_of(monitor).into_iter().next() else {
        return Vec::new();
    };
    SCALE_DIVISORS
        .iter()
        .filter_map(|&divisor| {
            let width = native_mode.width as f64 / divisor;
            let height = native_mode.height as f64 / divisor;
            if (width - width.round()).abs() > 0.001 || (height - height.round()).abs() > 0.001 {
                return None;
            }
            Some(ShownMode {
                width: width.round() as i64,
                height: height.round() as i64,
                mode: native_mode.clone(),
                scale: (divisor * 1e6).round() / 1e6,
                rate: None,
                is_native: divisor == 1.0,
            })
        })
        .collect()
}

fn panel_modes_of(monitor: &Monitor) -> Vec<ShownMode> {
    let modes = modes_of(monitor);
    let native_mode = modes.first().cloned();
    modes
        .iter()
        .map(|mode| ShownMode {
            width: mode.width,
            height: mode.height,
            mode: mode.clone(),
            scale: 1.0,
            rate: mode.rates.iter().copied().fold(None, |best, rate| {
                Some(best.map_or(rate, |best: f64| best.max(rate)))
            }),
            is_native: Some(mode) == native_mode.as_ref(),
        })
        .collect()
}

pub fn shown_modes_of(monitor: &Monitor, should_include_all: bool) -> Vec<ShownMode> {
    let mut modes = scaled_modes_of(monitor);
    for entry in panel_modes_of(monitor) {
        let fits_current = entry.mode.width == monitor.width && entry.mode.height == monitor.height;
        if !should_include_all && !fits_current {
            continue;
        }
        if modes
            .iter()
            .any(|known| known.width == entry.width && known.height == entry.height)
        {
            continue;
        }
        modes.push(entry);
    }
    modes.sort_by_key(|entry| std::cmp::Reverse(entry.width * entry.height));
    modes
}

pub fn rates_of(monitor: &Monitor) -> Vec<f64> {
    let mut rates = modes_of(monitor)
        .into_iter()
        .find(|mode| mode.width == monitor.width && mode.height == monitor.height)
        .map(|mode| mode.rates)
        .unwrap_or_default();
    rates.sort_by(|a, b| b.total_cmp(a));
    rates
}

fn connector_rank(name: &str) -> u32 {
    let head = name.split('-').next().unwrap_or("");
    CONNECTOR_PREFERENCE
        .iter()
        .find(|(prefix, _)| *prefix == head)
        .map_or(9, |(_, rank)| *rank)
}

fn connector_number(name: &str) -> u64 {
    let digits: String = name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().unwrap_or(0)
}

pub(super) fn primary_of(monitors: &[Monitor], preferred: &str) -> String {
    let enabled_monitors = || monitors.iter().filter(|monitor| !monitor.is_disabled);
    if enabled_monitors().any(|monitor| monitor.name == preferred) {
        return preferred.to_owned();
    }
    let mut names: Vec<&String> = enabled_monitors().map(|monitor| &monitor.name).collect();
    names.sort_by(|a, b| compare_connectors(a, b));
    names
        .first()
        .map(|name| (*name).clone())
        .unwrap_or_default()
}

fn compare_connectors(a: &str, b: &str) -> Ordering {
    connector_rank(a)
        .cmp(&connector_rank(b))
        .then(connector_number(a).cmp(&connector_number(b)))
        .then(a.cmp(b))
}

pub fn read_monitors() -> Option<Vec<Monitor>> {
    Some(
        hypr::json("monitors all")?
            .as_array()?
            .iter()
            .filter_map(Monitor::from_json)
            .collect(),
    )
}

pub fn order_main_first(monitors: &[Monitor], preferred: &str) -> Vec<String> {
    let mut names: Vec<String> = monitors
        .iter()
        .filter(|monitor| !monitor.is_disabled && monitor.mirror_of == "none")
        .map(|monitor| monitor.name.clone())
        .collect();
    names.sort_by(|a, b| {
        (a != preferred)
            .cmp(&(b != preferred))
            .then(compare_connectors(a, b))
    });
    names
}

#[cfg(test)]
pub(super) fn monitor(modes: &[&str], width: i64, height: i64) -> Monitor {
    Monitor {
        id: 0,
        name: "DP-1".to_owned(),
        model: String::new(),
        width,
        height,
        x: 0,
        y: 0,
        scale: 1.0,
        transform: 0,
        refresh_rate: 60.0,
        mirror_of: "none".to_owned(),
        current_format: String::new(),
        color_preset: "srgb".to_owned(),
        available_modes: modes.iter().map(|mode| (*mode).to_owned()).collect(),
        is_disabled: false,
        active_workspace_id: 0,
        is_hdr_capable: None,
        is_vrr_capable: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_the_monitor_does_not_list_is_still_offered_as_the_current_one() {
        let screen = monitor(&["5120x2160@50.00Hz", "3840x2160@60.00Hz"], 2560, 1440);
        let mode_sizes: Vec<(i64, i64)> = shown_modes_of(&screen, false)
            .iter()
            .map(|mode| (mode.width, mode.height))
            .collect();
        assert!(mode_sizes.contains(&(2560, 1440)), "{mode_sizes:?}");
        assert_eq!(rates_of(&screen), [60.0]);
        assert_eq!(modes_of(&screen)[0].width, 5120);
    }

    #[test]
    fn resolutions_offer_the_native_mode_scaled_and_the_panel_modes_on_request() {
        let screen = monitor(
            &[
                "2560x1440@144.00Hz",
                "2560x1440@60.00Hz",
                "1680x1050@60.00Hz",
                "1280x720@60.00Hz",
            ],
            2560,
            1440,
        );
        let sizes = |should_include_all| -> Vec<(i64, i64, f64)> {
            shown_modes_of(&screen, should_include_all)
                .into_iter()
                .map(|mode| (mode.width, mode.height, mode.scale))
                .collect()
        };
        let scaled_sizes = [
            (2560, 1440, 1.0),
            (2048, 1152, 1.25),
            (1920, 1080, 1.333333),
            (1600, 900, 1.6),
            (1280, 720, 2.0),
        ];
        assert_eq!(sizes(false), scaled_sizes);
        let mut all_sizes = scaled_sizes.to_vec();
        all_sizes.insert(3, (1680, 1050, 1.0));
        assert_eq!(sizes(true), all_sizes);
        assert_eq!(rates_of(&screen), [144.0, 60.0]);
    }

    #[test]
    fn the_main_display_is_never_one_that_is_off() {
        let screen = |name: &str, is_disabled: bool| Monitor {
            name: name.to_owned(),
            is_disabled,
            ..monitor(&[], 1920, 1080)
        };
        let monitors = [screen("DP-1", true), screen("HDMI-A-1", false)];
        assert_eq!(primary_of(&monitors, ""), "HDMI-A-1");
        assert_eq!(primary_of(&monitors, "DP-1"), "HDMI-A-1");
        assert_eq!(primary_of(&monitors, "HDMI-A-1"), "HDMI-A-1");
        assert_eq!(
            primary_of(&[screen("DP-1", false), screen("DP-2", false)], ""),
            "DP-1"
        );
    }

    #[test]
    fn a_monitor_reports_what_its_hardware_can_do() {
        let parse = |details: &str| {
            let reply = format!(r#"{{"name": "DP-1"{details}}}"#);
            let value: Value = serde_json::from_str(&reply).unwrap();
            let monitor = Monitor::from_json(&value).unwrap();
            (monitor.is_hdr_capable, monitor.is_vrr_capable)
        };
        let details = |hdr: bool, bt2020: bool, vrr: bool| {
            format!(
                r#", "hardwareDetails": {{"hdr": {hdr}, "bt2020": {bt2020}, "vrrCapable": {vrr}}}"#
            )
        };
        assert_eq!(parse(&details(true, true, true)), (Some(true), Some(true)));
        assert_eq!(
            parse(&details(true, false, false)),
            (Some(false), Some(false))
        );
        assert_eq!(
            parse(&details(false, true, false)),
            (Some(false), Some(false))
        );
        assert_eq!(parse(r#", "hardwareDetails": {}"#), (None, None));
        assert_eq!(parse(""), (None, None));
    }

    #[test]
    fn workspace_groups_go_to_the_main_display_first_then_by_connector() {
        let screen = |name: &str, is_disabled: bool, mirror_of: &str| Monitor {
            name: name.to_owned(),
            is_disabled,
            mirror_of: mirror_of.to_owned(),
            ..monitor(&[], 1920, 1080)
        };
        let monitors = [
            screen("HDMI-A-1", false, "none"),
            screen("DP-10", false, "none"),
            screen("eDP-1", false, "none"),
            screen("DP-2", false, "none"),
            screen("DP-1", true, "none"),
            screen("DP-3", false, "0"),
        ];
        assert_eq!(
            order_main_first(&monitors, "eDP-1"),
            ["eDP-1", "DP-2", "DP-10", "HDMI-A-1"]
        );
        assert_eq!(
            order_main_first(&monitors, "DP-1"),
            ["DP-2", "DP-10", "HDMI-A-1", "eDP-1"]
        );
    }
}

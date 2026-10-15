use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::hyprland::hypr;
use crate::platform::hyprland::monitorrules::{self, Rule, RuleSet};

const CONNECTOR_PREFERENCE: [(&str, u32); 6] = [
    ("DP", 0),
    ("HDMI", 1),
    ("DVI", 2),
    ("eDP", 3),
    ("LVDS", 4),
    ("VGA", 5),
];
const SCALE_DIVISORS: [f64; 6] = [1.0, 1.25, 4.0 / 3.0, 1.5, 1.6, 2.0];

pub const RULE_DEFAULTS: [(&str, &str); 14] = [
    ("bitdepth", "8"),
    ("sdr_eotf", "default"),
    ("sdrbrightness", "1"),
    ("sdrsaturation", "1"),
    ("vrr", "-1"),
    ("icc", ""),
    ("supports_wide_color", "0"),
    ("supports_hdr", "0"),
    ("sdr_min_luminance", "0.2"),
    ("sdr_max_luminance", "80"),
    ("min_luminance", "-1"),
    ("max_luminance", "-1"),
    ("max_avg_luminance", "-1"),
    ("reserved_area", "0"),
];

#[derive(Clone, Debug, PartialEq)]
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

fn primary_of(monitors: &[Monitor], preferred: &str) -> String {
    let enabled_monitors = || monitors.iter().filter(|monitor| !monitor.is_disabled);
    if enabled_monitors().any(|monitor| monitor.name == preferred) {
        return preferred.to_owned();
    }
    let mut names: Vec<&String> = enabled_monitors().map(|monitor| &monitor.name).collect();
    names.sort_by(|a, b| {
        connector_rank(a)
            .cmp(&connector_rank(b))
            .then(connector_number(a).cmp(&connector_number(b)))
            .then(a.cmp(b))
    });
    names
        .first()
        .map(|name| (*name).clone())
        .unwrap_or_default()
}

pub fn format_number(value: f64) -> String {
    format!("{value}")
}

pub struct Displays {
    pub monitors: RefCell<Vec<Monitor>>,
    pub requested: RefCell<RuleSet>,
    pub icc_profiles: Vec<String>,
    pub write_error: RefCell<Option<String>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Displays {
    pub fn new() -> Rc<Self> {
        let displays = Rc::new(Displays {
            monitors: RefCell::new(Vec::new()),
            requested: RefCell::new(RuleSet::default()),
            icc_profiles: monitorrules::icc_profiles(),
            write_error: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
        });
        displays.reload();
        displays
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn reload(&self) {
        let monitors = hypr::json("monitors all")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(Monitor::from_json)
            .collect();
        self.monitors.replace(monitors);
        self.requested.replace(monitorrules::read());
        self.emit_changed();
    }

    fn emit_changed(&self) {
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    pub fn primary(&self) -> String {
        primary_of(&self.monitors.borrow(), &self.requested.borrow().primary)
    }

    pub fn rule_of(&self, name: &str) -> Rule {
        self.requested
            .borrow()
            .monitors
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn value_of(&self, name: &str, key: &str) -> String {
        self.rule_of(name).get(key).cloned().unwrap_or_else(|| {
            RULE_DEFAULTS
                .iter()
                .find(|(default, _)| *default == key)
                .map(|(_, value)| (*value).to_owned())
                .unwrap_or_default()
        })
    }

    pub fn number_of(&self, name: &str, key: &str) -> f64 {
        self.value_of(name, key).trim().parse().unwrap_or(f64::NAN)
    }

    pub fn color_profile_of(&self, monitor: &Monitor) -> String {
        self.rule_of(&monitor.name)
            .get("cm")
            .cloned()
            .unwrap_or_else(|| {
                if monitor.color_preset.is_empty() {
                    "srgb".to_owned()
                } else {
                    monitor.color_preset.clone()
                }
            })
    }

    pub fn reserved_side_of(&self, name: &str, side: &str) -> i64 {
        reserved_side(&self.value_of(name, "reserved_area"), side)
    }

    pub fn set_reserved_side(&self, monitor: &Monitor, side: &str, size: i64) {
        let sides: Vec<String> = ["top", "right", "bottom", "left"]
            .iter()
            .map(|name| {
                let value = if *name == side {
                    size
                } else {
                    self.reserved_side_of(&monitor.name, name)
                };
                format!("{name} = {value}")
            })
            .collect();
        self.apply(
            monitor,
            &[(
                "reserved_area".to_owned(),
                format!("{{ {} }}", sides.join(", ")),
            )],
        );
    }

    pub fn set_color_profile(&self, monitor: &Monitor, profile: &str) {
        let mut keys = vec![("cm".to_owned(), profile.to_owned())];
        if profile.starts_with("hdr") {
            keys.push(("bitdepth".to_owned(), "10".to_owned()));
        }
        self.apply(monitor, &keys);
    }

    fn rule_for_what_is_running(
        &self,
        monitor: &Monitor,
        size: &str,
        rate: f64,
    ) -> Vec<(String, String)> {
        let rule = self.rule_of(&monitor.name);
        vec![
            ("mode".to_owned(), format!("{size}@{rate:.2}")),
            (
                "position".to_owned(),
                format!("{}x{}", monitor.x, monitor.y),
            ),
            ("scale".to_owned(), format_number(monitor.scale)),
            ("transform".to_owned(), monitor.transform.to_string()),
            (
                "bitdepth".to_owned(),
                rule.get("bitdepth").cloned().unwrap_or_else(|| {
                    if monitor.current_format.contains("2101010") {
                        "10".to_owned()
                    } else {
                        "8".to_owned()
                    }
                }),
            ),
            (
                "cm".to_owned(),
                rule.get("cm")
                    .cloned()
                    .unwrap_or_else(|| monitor.color_preset.clone()),
            ),
        ]
    }

    fn rule_pairs(&self, monitor: &Monitor, keys: &[(String, String)]) -> Vec<(String, String)> {
        let lookup = |name: &str| {
            keys.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        let size =
            lookup("size").unwrap_or_else(|| format!("{}x{}", monitor.width, monitor.height));
        let rate = lookup("rate")
            .and_then(|rate| rate.parse().ok())
            .unwrap_or(monitor.refresh_rate);
        let mut settings = self.rule_for_what_is_running(monitor, &size, rate);
        for (key, value) in keys {
            match settings.iter_mut().find(|(known, _)| known == key) {
                Some(slot) => slot.1 = value.clone(),
                None => settings.push((key.clone(), value.clone())),
            }
        }
        settings.retain(|(key, value)| {
            key != "size" && key != "rate" && !(key == "icc" && value.is_empty())
        });
        settings
    }

    pub fn apply(&self, monitor: &Monitor, keys: &[(String, String)]) {
        self.apply_with_primary(monitor, keys, None);
    }

    pub fn apply_with_primary(
        &self,
        monitor: &Monitor,
        keys: &[(String, String)],
        primary: Option<&str>,
    ) {
        let pairs = self.rule_pairs(monitor, keys);
        self.write(vec![(monitor.name.clone(), pairs)], primary);
    }

    pub fn set_disabled(
        &self,
        monitor: &Monitor,
        is_disabled: bool,
        primary: Option<&str>,
    ) -> bool {
        let pairs = vec![("disabled".to_owned(), is_disabled.to_string())];
        self.write(vec![(monitor.name.clone(), pairs)], primary)
    }

    fn write(
        &self,
        mut rules: Vec<(String, Vec<(String, String)>)>,
        primary: Option<&str>,
    ) -> bool {
        if let Some(name) = primary {
            merge_primary_rules(&mut rules, self.build_primary_rules(name));
        }
        match monitorrules::write_rules(&rules, primary) {
            Ok(()) => {
                self.write_error.take();
                hypr::request("reload");
                self.reload();
                true
            }
            Err(error) => {
                self.write_error.replace(Some(error.to_string()));
                self.emit_changed();
                false
            }
        }
    }

    fn build_position_rules(
        &self,
        positions: &[(String, i64, i64)],
    ) -> Vec<(String, Vec<(String, String)>)> {
        let monitors = self.monitors.borrow();
        let mut rules = Vec::new();
        for (name, x, y) in positions {
            let Some(monitor) = monitors.iter().find(|monitor| &monitor.name == name) else {
                continue;
            };
            let position = format!("{x}x{y}");
            if monitor.x == *x
                && monitor.y == *y
                && self.rule_of(name).get("position") == Some(&position)
            {
                continue;
            }
            let pairs = if monitor.is_disabled {
                if !self.requested.borrow().monitors.contains_key(name) {
                    continue;
                }
                vec![("position".to_owned(), position)]
            } else {
                self.rule_pairs(monitor, &[("position".to_owned(), position)])
            };
            rules.push((monitor.name.clone(), pairs));
        }
        rules
    }

    pub fn move_to(&self, name: &str, x: i64, y: i64) {
        let absolute_positions: Vec<(String, i64, i64)> = self
            .monitors
            .borrow()
            .iter()
            .map(|monitor| {
                if monitor.name == name {
                    (monitor.name.clone(), x, y)
                } else {
                    (monitor.name.clone(), monitor.x, monitor.y)
                }
            })
            .collect();
        let primary = self.primary();
        let Some(anchor) = absolute_positions
            .iter()
            .find(|(name, _, _)| *name == primary)
            .or(absolute_positions.first())
            .cloned()
        else {
            return;
        };
        let anchored_positions: Vec<(String, i64, i64)> = absolute_positions
            .into_iter()
            .map(|(name, x, y)| (name, x - anchor.1, y - anchor.2))
            .collect();
        let rules = self.build_position_rules(&anchored_positions);
        if !rules.is_empty() {
            self.write(rules, None);
        }
    }

    pub fn set_primary(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        self.write(Vec::new(), Some(name))
    }

    fn build_primary_rules(&self, name: &str) -> Vec<(String, Vec<(String, String)>)> {
        let monitors = self.monitors.borrow();
        let Some(anchor) = monitors.iter().find(|monitor| monitor.name == name) else {
            return Vec::new();
        };
        let positions: Vec<(String, i64, i64)> = monitors
            .iter()
            .map(|monitor| {
                (
                    monitor.name.clone(),
                    monitor.x - anchor.x,
                    monitor.y - anchor.y,
                )
            })
            .collect();
        let mut rules = Vec::new();
        if anchor.mirror_of != "none" {
            let keys = [("mirror".to_owned(), String::new())];
            rules.push((name.to_owned(), self.rule_pairs(anchor, &keys)));
        }
        merge_primary_rules(&mut rules, self.build_position_rules(&positions));
        rules
    }
}

fn merge_primary_rules(
    rules: &mut Vec<(String, Vec<(String, String)>)>,
    position_rules: Vec<(String, Vec<(String, String)>)>,
) {
    for (output, pairs) in position_rules {
        let Some((_, own_pairs)) = rules.iter_mut().find(|(name, _)| *name == output) else {
            rules.push((output, pairs));
            continue;
        };
        for (key, value) in pairs {
            match own_pairs.iter_mut().find(|(own_key, _)| *own_key == key) {
                Some(slot) if key == "position" => slot.1 = value,
                Some(_) => {}
                None => own_pairs.push((key, value)),
            }
        }
    }
}

pub fn reserved_side(area: &str, side: &str) -> i64 {
    let mut search_start = 0;
    while let Some(match_index) = area[search_start..]
        .find(side)
        .map(|offset| offset + search_start)
    {
        let rest = area[match_index + side.len()..].trim_start();
        if let Some(value) = rest.strip_prefix('=') {
            let value = value.trim_start();
            let digits: String = value
                .char_indices()
                .take_while(|(index, character)| {
                    character.is_ascii_digit() || (*index == 0 && *character == '-')
                })
                .map(|(_, character)| character)
                .collect();
            if let Ok(number) = digits.parse() {
                return number;
            }
        }
        search_start = match_index + 1;
    }
    let leading_digits: String = area
        .trim_start()
        .char_indices()
        .take_while(|(index, character)| {
            character.is_ascii_digit() || (*index == 0 && (*character == '-' || *character == '+'))
        })
        .map(|(_, character)| character)
        .collect();
    leading_digits.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_main_display_moves_the_changed_display_and_keeps_its_other_keys() {
        let pair = |key: &str, value: &str| (key.to_owned(), value.to_owned());
        let mut rules = vec![
            (
                "DP-1".to_owned(),
                vec![
                    pair("position", "0x0"),
                    pair("mirror", "HDMI-A-1"),
                    pair("mode", "1920x1080"),
                ],
            ),
            ("DP-2".to_owned(), vec![pair("disabled", "true")]),
        ];
        let running_rule = |position: &str| {
            vec![
                pair("mode", "2560x1440"),
                pair("position", position),
                pair("scale", "1"),
            ]
        };
        merge_primary_rules(
            &mut rules,
            vec![
                ("DP-1".to_owned(), running_rule("-2560x0")),
                ("DP-2".to_owned(), running_rule("2560x0")),
                ("HDMI-A-1".to_owned(), vec![pair("position", "0x0")]),
            ],
        );
        assert_eq!(
            rules,
            [
                (
                    "DP-1".to_owned(),
                    vec![
                        pair("position", "-2560x0"),
                        pair("mirror", "HDMI-A-1"),
                        pair("mode", "1920x1080"),
                        pair("scale", "1"),
                    ]
                ),
                (
                    "DP-2".to_owned(),
                    vec![
                        pair("disabled", "true"),
                        pair("mode", "2560x1440"),
                        pair("position", "2560x0"),
                        pair("scale", "1"),
                    ]
                ),
                ("HDMI-A-1".to_owned(), vec![pair("position", "0x0")]),
            ]
        );
    }

    fn monitor(modes: &[&str], width: i64, height: i64) -> Monitor {
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
        }
    }

    #[test]
    fn a_mirroring_display_that_becomes_main_stops_mirroring() {
        let mirror = Monitor {
            id: 1,
            name: "HDMI-A-1".to_owned(),
            mirror_of: "0".to_owned(),
            ..monitor(&[], 1920, 1080)
        };
        let displays = Displays {
            monitors: RefCell::new(vec![monitor(&[], 1920, 1080), mirror]),
            requested: RefCell::default(),
            icc_profiles: Vec::new(),
            write_error: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
        };
        let rules = displays.build_primary_rules("HDMI-A-1");
        let cleared_mirror = ("mirror".to_owned(), String::new());
        assert!(
            rules
                .iter()
                .any(|(name, pairs)| name == "HDMI-A-1" && pairs.contains(&cleared_mirror)),
            "{rules:?}"
        );
    }

    #[test]
    fn a_new_main_display_moves_displays_that_are_off_by_their_position_only() {
        let main = Monitor {
            name: "HDMI-A-1".to_owned(),
            x: 1920,
            ..monitor(&[], 1920, 1080)
        };
        let make_off = |name: &str, x: i64| Monitor {
            name: name.to_owned(),
            x,
            is_disabled: true,
            ..monitor(&[], 1920, 1080)
        };
        let disabled_rule = Rule::from([("disabled".to_owned(), "true".to_owned())]);
        let displays = Displays {
            monitors: RefCell::new(vec![
                monitor(&[], 1920, 1080),
                main,
                make_off("DP-2", 3840),
                make_off("DP-3", 5760),
            ]),
            requested: RefCell::new(RuleSet {
                monitors: [("DP-2".to_owned(), disabled_rule)].into(),
                primary: String::new(),
            }),
            icc_profiles: Vec::new(),
            write_error: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
        };
        let rules = displays.build_primary_rules("HDMI-A-1");
        let names: Vec<&str> = rules.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["DP-1", "HDMI-A-1", "DP-2"]);
        assert_eq!(rules[2].1, [("position".to_owned(), "1920x0".to_owned())]);
    }

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
    fn a_reserved_area_is_read_side_by_side_or_as_one_number() {
        assert_eq!(
            reserved_side("{ top = 5, right = 0, bottom = -3, left = 0 }", "top"),
            5
        );
        assert_eq!(
            reserved_side("{ top = 5, right = 0, bottom = -3, left = 0 }", "bottom"),
            -3
        );
        assert_eq!(reserved_side("12", "left"), 12);
        assert_eq!(reserved_side("0", "top"), 0);
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
}

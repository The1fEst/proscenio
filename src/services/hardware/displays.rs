use std::cell::RefCell;
use std::rc::Rc;

use crate::core::listeners::Subscription;
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprconfig::{self, Area};
use crate::platform::hyprland::monitorrules::{self, Rule, RuleSet};
use crate::services::hardware::displayconfirm::{DisplayConfirmation, needs_confirmation};
use crate::services::hardware::monitors::{Monitor, primary_of, read_monitors};

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

pub fn format_number(value: f64) -> String {
    format!("{value}")
}

pub struct Displays {
    pub monitors: RefCell<Vec<Monitor>>,
    pub requested: RefCell<RuleSet>,
    pub icc_profiles: Vec<String>,
    pub write_error: RefCell<Option<String>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    reverted_subscription: RefCell<Option<Subscription>>,
}

impl Displays {
    pub fn new() -> Rc<Self> {
        let displays = Rc::new(Displays {
            monitors: RefCell::new(Vec::new()),
            requested: RefCell::new(RuleSet::default()),
            icc_profiles: monitorrules::icc_profiles(),
            write_error: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
            reverted_subscription: RefCell::new(None),
        });
        displays.reverted_subscription.replace(Some(
            DisplayConfirmation::shared().connect_reverted({
                let displays = Rc::downgrade(&displays);
                move || {
                    if let Some(displays) = displays.upgrade() {
                        displays.reload();
                    }
                }
            }),
        ));
        displays.reload();
        displays
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn reload(&self) {
        self.monitors.replace(read_monitors().unwrap_or_default());
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
        let requires_confirmation =
            needs_confirmation(&self.current_values(monitor, &pairs), &pairs);
        self.write(
            vec![(monitor.name.clone(), pairs)],
            primary,
            requires_confirmation,
        );
    }

    fn current_values(
        &self,
        monitor: &Monitor,
        pairs: &[(String, String)],
    ) -> Vec<(String, String)> {
        let mut values = self.rule_pairs(monitor, &[]);
        for (key, _) in pairs {
            if !values.iter().any(|(name, _)| name == key) {
                values.push((key.clone(), self.value_of(&monitor.name, key)));
            }
        }
        values
    }

    pub fn set_disabled(
        &self,
        monitor: &Monitor,
        is_disabled: bool,
        primary: Option<&str>,
    ) -> bool {
        let pairs = vec![("disabled".to_owned(), is_disabled.to_string())];
        let current_pairs = [("disabled".to_owned(), monitor.is_disabled.to_string())];
        let requires_confirmation = needs_confirmation(&current_pairs, &pairs);
        self.write(
            vec![(monitor.name.clone(), pairs)],
            primary,
            requires_confirmation,
        )
    }

    fn write(
        &self,
        mut rules: Vec<(String, Vec<(String, String)>)>,
        primary: Option<&str>,
        requires_confirmation: bool,
    ) -> bool {
        if let Some(name) = primary {
            merge_primary_rules(&mut rules, self.build_primary_rules(name));
        }
        let restore_text = requires_confirmation
            .then(|| hyprconfig::read_for_edit(Area::Displays).ok())
            .flatten();
        match monitorrules::write_rules(&rules, primary) {
            Ok(()) => {
                self.write_error.take();
                hypr::request("reload");
                self.reload();
                if let Some(restore_text) = restore_text {
                    DisplayConfirmation::shared().begin(restore_text);
                }
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
            self.write(rules, None, false);
        }
    }

    pub fn set_primary(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        self.write(Vec::new(), Some(name), false)
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
    use crate::services::hardware::monitors::monitor;

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
            reverted_subscription: RefCell::new(None),
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
            reverted_subscription: RefCell::new(None),
        };
        let rules = displays.build_primary_rules("HDMI-A-1");
        let names: Vec<&str> = rules.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["DP-1", "HDMI-A-1", "DP-2"]);
        assert_eq!(rules[2].1, [("position".to_owned(), "1920x0".to_owned())]);
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
    fn choosing_what_a_display_already_runs_needs_no_confirmation() {
        let displays = Displays {
            monitors: RefCell::new(vec![monitor(&[], 1920, 1080)]),
            requested: RefCell::default(),
            icc_profiles: Vec::new(),
            write_error: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
            reverted_subscription: RefCell::new(None),
        };
        let current_monitor = displays.monitors.borrow()[0].clone();
        let needs = |keys: &[(&str, &str)]| {
            let keys: Vec<(String, String)> = keys
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect();
            let pairs = displays.rule_pairs(&current_monitor, &keys);
            needs_confirmation(&displays.current_values(&current_monitor, &pairs), &pairs)
        };
        for (keys, expected) in [
            (&[("rate", "60")][..], false),
            (
                &[("size", "1920x1080"), ("scale", "1"), ("rate", "60")][..],
                false,
            ),
            (&[("position", "10x0")][..], false),
            (&[("supports_hdr", "0")][..], false),
            (&[("vrr", "-1")][..], false),
            (&[("rate", "30")][..], true),
            (&[("size", "1280x720")][..], true),
            (&[("scale", "1.25")][..], true),
            (&[("transform", "1")][..], true),
            (&[("cm", "hdr")][..], true),
            (&[("supports_hdr", "1")][..], true),
            (&[("vrr", "1")][..], true),
        ] {
            assert_eq!(needs(keys), expected, "{keys:?}");
        }
    }
}

use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::{config, tools};
use crate::panels::settings::arrangement::Arrangement;
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::panels::settings::hyprrows;
use crate::platform::hypr;
use crate::services::displays::{Displays, Monitor, number, rates_of, shown_modes_of};
use crate::services::hyproptions::HyprOptions;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::controls::ComboBox;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::spinbox::SpinBox;

const AUTO_HDR: &str = "render:cm_auto_hdr";
const GLOBAL_VRR: &str = "misc:vrr";
const ZERO_SCALING: &str = "xwayland:force_zero_scaling";
const GLOBAL_VRR_MODES: [(&str, &str); 4] = [
    ("Off", "0"),
    ("On", "1"),
    ("Fullscreen only", "2"),
    ("Fullscreen games and video", "3"),
];
const NIGHT_AUTOMATIC: &str = "/light/night/automatic";

type Commit = Box<dyn Fn(&Rc<Displays>, &Monitor, i64)>;

struct Spin {
    spin: Rc<SpinBox>,
    current: Box<dyn Fn(&Displays, &str) -> i64>,
}

struct Widgets {
    arrangement_section: Option<gtk4::Widget>,
    arrangement: Rc<Arrangement>,
    choices: gtk4::Box,
    selection: RefCell<Option<(Vec<String>, Rc<Selection>)>>,
    use_as_section: Option<gtk4::Widget>,
    use_as: Rc<ComboBox>,
    resolution: Rc<ComboBox>,
    rate: Rc<ComboBox>,
    rotation: Rc<ComboBox>,
    vrr: Rc<ComboBox>,
    profile: Rc<ComboBox>,
    depth: Rc<ComboBox>,
    wide: Rc<ComboBox>,
    hdr: Rc<ComboBox>,
    auto_hdr: Rc<ComboBox>,
    eotf: Rc<ComboBox>,
    icc: Rc<ComboBox>,
    spins: Vec<Spin>,
}

struct State {
    theme: SharedTheme,
    displays: Rc<Displays>,
    options: Rc<HyprOptions>,
    selected: RefCell<String>,
    all_resolutions: Cell<bool>,
    widgets: RefCell<Option<Widgets>>,
}

const ROTATIONS: [(&str, i64); 8] = [
    ("Standard", 0),
    ("90°", 1),
    ("180°", 2),
    ("270°", 3),
    ("Flipped", 4),
    ("Flipped, 90°", 5),
    ("Flipped, 180°", 6),
    ("Flipped, 270°", 7),
];
const VRR: [(&str, i64); 5] = [
    ("Follow the global setting", -1),
    ("Off", 0),
    ("On", 1),
    ("Fullscreen only", 2),
    ("Fullscreen games and video", 3),
];
const FORCED: [(&str, i64); 3] = [("Automatic", 0), ("On", 1), ("Off", -1)];
const DEPTHS: [(&str, i64); 2] = [("8-bit", 8), ("10-bit", 10)];
const AUTO_HDR_MODES: [(&str, i64); 3] = [("Off", 0), ("HDR", 1), ("HDR (display profile)", 2)];
const EOTFS: [(&str, &str); 5] = [
    ("Default", "default"),
    ("Automatic", "auto"),
    ("sRGB", "srgb"),
    ("Gamma 2.2", "gamma22"),
    ("Gamma 2.2, forced", "gamma22force"),
];

struct RuleSpin {
    key: &'static str,
    icon: &'static str,
    label: &'static str,
    factor: f64,
    decimals: u32,
    range: (i64, i64),
    step: i64,
}

const SDR_RULES: [RuleSpin; 4] = [
    RuleSpin {
        key: "sdrbrightness",
        icon: "brightness_medium",
        label: "SDR brightness",
        factor: 100.0,
        decimals: 2,
        range: (0, 1000),
        step: 5,
    },
    RuleSpin {
        key: "sdrsaturation",
        icon: "invert_colors",
        label: "SDR saturation",
        factor: 100.0,
        decimals: 2,
        range: (0, 1000),
        step: 5,
    },
    RuleSpin {
        key: "sdr_min_luminance",
        icon: "brightness_low",
        label: "SDR minimum luminance",
        factor: 100.0,
        decimals: 2,
        range: (0, 100000),
        step: 10,
    },
    RuleSpin {
        key: "sdr_max_luminance",
        icon: "brightness_high",
        label: "SDR maximum luminance",
        factor: 1.0,
        decimals: 0,
        range: (0, 10000),
        step: 10,
    },
];

const DISPLAY_RULES: [RuleSpin; 3] = [
    RuleSpin {
        key: "min_luminance",
        icon: "nightlight",
        label: "Display minimum luminance",
        factor: 100.0,
        decimals: 2,
        range: (-100, 100000),
        step: 10,
    },
    RuleSpin {
        key: "max_luminance",
        icon: "wb_sunny",
        label: "Display maximum luminance",
        factor: 1.0,
        decimals: 0,
        range: (-1, 10000),
        step: 10,
    },
    RuleSpin {
        key: "max_avg_luminance",
        icon: "exposure",
        label: "Display maximum average luminance",
        factor: 1.0,
        decimals: 0,
        range: (-1, 10000),
        step: 10,
    },
];

fn position<T: PartialEq>(values: &[T], current: &T) -> i32 {
    values
        .iter()
        .position(|value| value == current)
        .map_or(0, |index| index as i32)
}

fn labels<T>(list: &[(&str, T)]) -> Vec<String> {
    list.iter().map(|(label, _)| (*label).to_owned()).collect()
}

fn color_profiles(monitor: &Monitor) -> Vec<(String, &'static str)> {
    let model = |fallback: &str| {
        if monitor.model.is_empty() {
            fallback.to_owned()
        } else {
            monitor.model.clone()
        }
    };
    vec![
        ("Automatic".to_owned(), "auto"),
        ("sRGB".to_owned(), "srgb"),
        ("DCI P3".to_owned(), "dcip3"),
        ("Display P3".to_owned(), "dp3"),
        ("Adobe RGB".to_owned(), "adobe"),
        ("Wide color".to_owned(), "wide"),
        (model("Display profile"), "edid"),
        ("HDR".to_owned(), "hdr"),
        (format!("HDR ({})", model("display profile")), "hdredid"),
    ]
}

impl State {
    fn monitors(&self) -> Vec<Monitor> {
        self.displays.monitors.borrow().clone()
    }

    fn monitor(&self) -> Option<Monitor> {
        let monitors = self.monitors();
        let selected = self.selected.borrow().clone();
        monitors
            .iter()
            .find(|monitor| monitor.name == selected)
            .or(monitors.first())
            .cloned()
    }

    fn others(&self, monitor: &Monitor) -> Vec<Monitor> {
        self.monitors()
            .into_iter()
            .filter(|other| other.name != monitor.name)
            .collect()
    }

    fn mirror_target(&self, monitor: &Monitor) -> String {
        if monitor.mirror_of == "none" {
            return String::new();
        }
        self.monitors()
            .iter()
            .find(|other| other.id.to_string() == monitor.mirror_of)
            .map_or_else(|| monitor.mirror_of.clone(), |other| other.name.clone())
    }

    fn apply(&self, keys: &[(&str, String)]) {
        let Some(monitor) = self.monitor() else {
            return;
        };
        let keys: Vec<(String, String)> = keys
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        self.displays.apply(&monitor, &keys);
    }

    fn refresh(self: &Rc<Self>) {
        let widgets = self.widgets.borrow();
        let Some(widgets) = widgets.as_ref() else {
            return;
        };
        let monitors = self.monitors();
        let Some(monitor) = self.monitor() else {
            return;
        };
        let name = monitor.name.clone();

        if let Some(section) = &widgets.arrangement_section {
            section.set_visible(monitors.len() > 1);
        }
        widgets.arrangement.set(
            monitors
                .iter()
                .filter(|monitor| !monitor.disabled)
                .cloned()
                .collect(),
            &name,
        );

        let names: Vec<String> = monitors
            .iter()
            .map(|monitor| monitor.name.clone())
            .collect();
        let rebuild = widgets
            .selection
            .borrow()
            .as_ref()
            .is_none_or(|(shown, _)| *shown != names);
        if rebuild {
            if let Some((_, old)) = widgets.selection.take() {
                widgets.choices.remove(&old.root);
            }
            let choices = monitors
                .iter()
                .map(|monitor| Choice {
                    label: format!(
                        "{} ({})",
                        if monitor.model.is_empty() {
                            &monitor.name
                        } else {
                            &monitor.model
                        },
                        monitor.name
                    ),
                    icon: "",
                    value: Value::from(monitor.name.clone()),
                })
                .collect();
            let state = Rc::downgrade(self);
            let selection = Selection::new(&self.theme, choices, move |value| {
                if let (Some(state), Some(name)) = (state.upgrade(), value.as_str()) {
                    state.selected.replace(name.to_owned());
                    state.refresh();
                }
            });
            selection.root.set_hexpand(true);
            widgets.choices.prepend(&selection.root);
            widgets.selection.replace(Some((names, selection)));
        }
        if let Some((_, selection)) = widgets.selection.borrow().as_ref() {
            selection.set_current(&Value::from(name.clone()));
        }

        let others = self.others(&monitor);
        if let Some(section) = &widgets.use_as_section {
            section.set_visible(!others.is_empty());
        }
        let primary = self.displays.primary();
        let target = self.mirror_target(&monitor);
        let mut use_as_values = vec!["main".to_owned(), "extend".to_owned()];
        let mut use_as_labels = vec!["Main display".to_owned(), "Extended display".to_owned()];
        for other in &others {
            use_as_values.push(other.name.clone());
            use_as_labels.push(format!(
                "Mirror for {}",
                if other.model.is_empty() {
                    &other.name
                } else {
                    &other.model
                }
            ));
        }
        if can_turn_off(&monitor, &others) {
            use_as_values.push("off".to_owned());
            use_as_labels.push("Off".to_owned());
        }
        let current_use = if monitor.disabled {
            "off".to_owned()
        } else if primary == name {
            "main".to_owned()
        } else {
            others
                .iter()
                .find(|other| other.name == target)
                .map_or_else(|| "extend".to_owned(), |other| other.name.clone())
        };
        widgets
            .use_as
            .set_items(&use_as_labels, position(&use_as_values, &current_use));

        let modes = shown_modes_of(&monitor, self.all_resolutions.get());
        let scale = monitor.scale.max(0.01);
        let logical = format!(
            "{}x{}",
            (monitor.width as f64 / scale).round(),
            (monitor.height as f64 / scale).round()
        );
        let mode_values: Vec<String> = modes
            .iter()
            .map(|mode| format!("{}x{}", mode.width, mode.height))
            .collect();
        let mode_labels: Vec<String> = modes
            .iter()
            .map(|mode| {
                if mode.native {
                    format!("{} × {} (Default)", mode.width, mode.height)
                } else {
                    format!("{} × {}", mode.width, mode.height)
                }
            })
            .collect();
        widgets
            .resolution
            .set_items(&mode_labels, position(&mode_values, &logical));

        let rates: Vec<i64> = rates_of(&monitor)
            .iter()
            .map(|rate| rate.round() as i64)
            .collect();
        let rate_labels: Vec<String> = rates.iter().map(|rate| format!("{rate} Hz")).collect();
        widgets.rate.set_items(
            &rate_labels,
            position(&rates, &(monitor.refresh_rate.round() as i64)),
        );
        widgets.rotation.set_items(
            &labels(&ROTATIONS),
            position(&ROTATIONS.map(|(_, value)| value), &monitor.transform),
        );
        let rule_number = |key: &str| self.displays.number_of(&name, key) as i64;
        widgets.vrr.set_items(
            &labels(&VRR),
            position(&VRR.map(|(_, value)| value), &rule_number("vrr")),
        );

        let profiles = color_profiles(&monitor);
        let profile_labels: Vec<String> = profiles.iter().map(|(label, _)| label.clone()).collect();
        let profile_values: Vec<&str> = profiles.iter().map(|(_, value)| *value).collect();
        let current_profile = self.displays.color_profile_of(&monitor);
        widgets.profile.set_items(
            &profile_labels,
            position(&profile_values, &current_profile.as_str()),
        );
        widgets.depth.set_items(
            &labels(&DEPTHS),
            position(&DEPTHS.map(|(_, value)| value), &rule_number("bitdepth")),
        );
        widgets.wide.set_items(
            &labels(&FORCED),
            position(
                &FORCED.map(|(_, value)| value),
                &rule_number("supports_wide_color"),
            ),
        );
        widgets.hdr.set_items(
            &labels(&FORCED),
            position(
                &FORCED.map(|(_, value)| value),
                &rule_number("supports_hdr"),
            ),
        );
        widgets.auto_hdr.set_items(
            &labels(&AUTO_HDR_MODES),
            position(
                &AUTO_HDR_MODES.map(|(_, value)| value),
                &(self.options.number_or(AUTO_HDR, 1.0) as i64),
            ),
        );
        let eotf = self.displays.value_of(&name, "sdr_eotf");
        widgets.eotf.set_items(
            &labels(&EOTFS),
            position(&EOTFS.map(|(_, value)| value), &eotf.as_str()),
        );
        let icc = self.displays.value_of(&name, "icc");
        let mut icc_values = vec![String::new()];
        let mut icc_labels = vec!["None".to_owned()];
        for path in &self.displays.icc_profiles {
            icc_values.push(path.clone());
            icc_labels.push(path.rsplit('/').next().unwrap_or(path).to_owned());
        }
        widgets
            .icc
            .set_items(&icc_labels, position(&icc_values, &icc));

        for spin in &widgets.spins {
            spin.spin.set_value((spin.current)(&self.displays, &name));
        }
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let state = Rc::new(State {
        theme: page.theme.clone(),
        displays: Displays::new(),
        options: HyprOptions::new(&[AUTO_HDR, GLOBAL_VRR, ZERO_SCALING]),
        selected: RefCell::new(String::new()),
        all_resolutions: Cell::new(false),
        widgets: RefCell::new(None),
    });

    let arranged = page.section("", "");
    let arrangement = Arrangement::new(&page.theme);
    arranged.append(&arrangement.paint);
    arrangement.connect_picked({
        let state = Rc::downgrade(&state);
        move |name| {
            if let Some(state) = state.upgrade() {
                state.selected.replace(name.to_owned());
                state.refresh();
            }
        }
    });
    arrangement.connect_moved({
        let state = Rc::downgrade(&state);
        move |name, x, y| {
            if let Some(state) = state.upgrade() {
                state.displays.move_to(name, x, y);
            }
        }
    });

    page.keep(context.services.events.subscribe({
        let state = Rc::downgrade(&state);
        move |event, _| {
            if !matches!(
                event,
                "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
            ) {
                return;
            }
            if let Some(state) = state.upgrade() {
                state.displays.reload();
            }
        }
    }));

    let main = page.section("", "");
    let choices = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    main.append(&choices);
    let (rescan, _) = page.icon_button("refresh", true, "Rescan displays", {
        let state = Rc::downgrade(&state);
        move || {
            hypr::request("dispatch hl.dsp.force_renderer_reload()");
            if let Some(state) = state.upgrade() {
                state.displays.reload();
            }
        }
    });
    rescan.set_valign(gtk4::Align::Center);
    choices.append(&rescan);
    page.tip(
        &rescan,
        "Asks every monitor what it can do again. Modes a display only reports after it is fully awake show up after this.",
    );

    let use_as_group = page.subsection(&main, "Use as", "");
    let use_as = page.combo(&use_as_group, "desktop_windows");
    use_as.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            let others = state.others(&monitor);
            let value = match index {
                0 => "main".to_owned(),
                1 => "extend".to_owned(),
                other => match others.get(other - 2) {
                    Some(other) => other.name.clone(),
                    None if other == others.len() + 2 && can_turn_off(&monitor, &others) => {
                        "off".to_owned()
                    }
                    None => return,
                },
            };
            if value == "off" {
                if state.displays.primary() == monitor.name
                    && let Some(next) = others.iter().find(|other| !other.disabled)
                {
                    state.displays.set_primary(&next.name);
                }
                state.displays.set_disabled(&monitor, true);
                return;
            }
            if monitor.disabled {
                state.displays.set_disabled(&monitor, false);
                if value == "main" {
                    state.displays.set_primary(&monitor.name);
                } else if value != "extend" {
                    state.apply(&[("mirror", value)]);
                }
                return;
            }
            if value == "main" {
                state.displays.set_primary(&monitor.name);
                return;
            }
            if state.displays.primary() == monitor.name {
                let next = others
                    .first()
                    .map(|other| other.name.clone())
                    .unwrap_or_default();
                state.displays.set_primary(&next);
            }
            if value != "extend" {
                state.apply(&[("mirror", value)]);
                return;
            }
            if monitor.mirror_of == "none" {
                state.apply(&[("mirror", String::new())]);
                return;
            }
            let right = others
                .iter()
                .map(|other| {
                    let across = if other.transform % 2 == 1 {
                        other.height
                    } else {
                        other.width
                    };
                    other.x + (across as f64 / other.scale).round() as i64
                })
                .max()
                .unwrap_or(0);
            state.apply(&[
                ("mirror", String::new()),
                ("position", format!("{right}x0")),
            ]);
        }
    });

    let resolution_group = page.subsection(&main, "Resolution", "");
    let resolution = page.combo(&resolution_group, "aspect_ratio");
    resolution.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            let Some(mode) = shown_modes_of(&monitor, state.all_resolutions.get())
                .into_iter()
                .nth(index)
            else {
                return;
            };
            state.apply(&[
                ("size", format!("{}x{}", mode.mode.width, mode.mode.height)),
                ("scale", number(mode.scale)),
                ("rate", number(mode.rate.unwrap_or(monitor.refresh_rate))),
            ]);
        }
    });
    let all = page.switch(&main, "list", "Show all resolutions", {
        let state = Rc::downgrade(&state);
        move |on| {
            if let Some(state) = state.upgrade() {
                state.all_resolutions.set(on);
                state.refresh();
            }
        }
    });
    all.bind({
        let state = Rc::downgrade(&state);
        move || {
            state
                .upgrade()
                .is_some_and(|state| state.all_resolutions.get())
        }
    });

    let rate_group = page.subsection(&main, "Refresh rate", "");
    let rate = page.combo(&rate_group, "refresh");
    rate.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            if let Some(rate) = rates_of(&monitor).get(index) {
                state.apply(&[("rate", number(rate.round()))]);
            }
        }
    });

    let rotation = keyed_combo(&page, &state, &main, "Rotation", "", "screen_rotation", {
        move |index| ("transform", ROTATIONS[index].1.to_string())
    });
    let vrr = keyed_combo(&page, &state, &main, "Variable refresh rate", "", "sync", {
        move |index| ("vrr", VRR[index].1.to_string())
    });

    let color = page.section("palette", "Color");
    let profile_group = page.subsection(&color, "Color profile", "");
    let profile = page.combo(&profile_group, "colors");
    profile.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            if let Some((_, value)) = color_profiles(&monitor).get(index) {
                state.displays.set_color_profile(&monitor, value);
            }
        }
    });
    let depth = keyed_combo(&page, &state, &color, "Bit depth", "", "gradient", {
        move |index| ("bitdepth", DEPTHS[index].1.to_string())
    });
    let wide = keyed_combo(
        &page,
        &state,
        &color,
        "Force wide color",
        "",
        "invert_colors",
        move |index| ("supports_wide_color", FORCED[index].1.to_string()),
    );
    let hdr = keyed_combo(
        &page,
        &state,
        &color,
        "Force HDR",
        "Forcing this on a display that does not report HDR can leave the screen black.",
        "hdr_on",
        move |index| ("supports_hdr", FORCED[index].1.to_string()),
    );
    let eotf = keyed_combo(
        &page,
        &state,
        &color,
        "SDR transfer function",
        "",
        "functions",
        move |index| ("sdr_eotf", EOTFS[index].1.to_owned()),
    );
    let icc = keyed_combo(&page, &state, &color, "ICC profile", "", "description", {
        let state = Rc::downgrade(&state);
        move |index| {
            let path = match index {
                0 => String::new(),
                other => state
                    .upgrade()
                    .and_then(|state| state.displays.icc_profiles.get(other - 1).cloned())
                    .unwrap_or_default(),
            };
            ("icc", path)
        }
    });

    let mut spins = Vec::new();
    let luminance = page.section("brightness_6", "Luminance");
    for rule in &SDR_RULES {
        let (row, spin) = rule_spin(&page, &state, &luminance, rule);
        if rule.key == "sdrbrightness" {
            page.tip(
                &row,
                "How bright content that is not HDR is drawn while the display is in HDR",
            );
        }
        spins.push(spin);
    }
    let display_group = page.subsection(
        &luminance,
        "Display",
        "A luminance of −1 leaves the figure to what the display reports.",
    );
    for rule in &DISPLAY_RULES {
        spins.push(rule_spin(&page, &state, &display_group, rule).1);
    }

    let reserved = page.section("border_outer", "Reserved area");
    for (side, icon, label) in [
        ("top", "vertical_align_top", "Top"),
        ("right", "align_horizontal_right", "Right"),
        ("bottom", "vertical_align_bottom", "Bottom"),
        ("left", "align_horizontal_left", "Left"),
    ] {
        let (_, spin) = option_spin(
            &page,
            &state,
            &reserved,
            icon,
            label,
            (0, 2000),
            1,
            0,
            Box::new(move |displays, name| displays.reserved_side_of(name, side)),
            Box::new(move |displays, monitor, value| {
                displays.set_reserved_side(monitor, side, value)
            }),
        );
        spins.push(spin);
    }

    let every = page.section("desktop_windows", "All displays");
    let auto_group = page.subsection(
        &every,
        "Auto HDR",
        "Switches to HDR while a fullscreen window has HDR content.",
    );
    let auto_hdr = page.combo(&auto_group, "hdr_auto");
    auto_hdr.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            if let Some(state) = state.upgrade() {
                state
                    .options
                    .set(AUTO_HDR, &AUTO_HDR_MODES[index].1.to_string());
            }
        }
    });
    let sync_group = page.subsection(
        &every,
        "Variable refresh rate",
        "What a display set to follow the global setting does.",
    );
    hyprrows::combo(
        &page,
        &sync_group,
        &state.options,
        "sync",
        (GLOBAL_VRR, "0"),
        &GLOBAL_VRR_MODES,
    );
    let xwayland = page.subsection(&every, "X11 apps", "");
    let sharp = hyprrows::switch(
        &page,
        &xwayland,
        &state.options,
        "high_density",
        "Keep X11 apps sharp on scaled displays",
        ZERO_SCALING,
    );
    page.tip(
        &sharp.button,
        "X11 apps are drawn unscaled instead of stretched. They look sharp, and small unless they scale themselves (GDK_SCALE, QT_SCALE_FACTOR).",
    );

    let night = page.section("nightlight", "Night light");
    page.tools_notice(&night, &[&tools::HYPRSUNSET], "night light does nothing");
    page.config_switch(
        &night,
        "schedule",
        "Automatic schedule",
        NIGHT_AUTOMATIC,
        true,
    );
    let times = page.uniform_row(&night);
    let from = page.text_field(
        &times,
        Style::Outlined,
        "From (HH:mm)",
        || config::value_str("/light/night/from").unwrap_or_else(|| "19:00".to_owned()),
        |text| config::store_value("/light/night/from", Value::from(text.trim())),
    );
    page.refresh_text_on("/light/night/from", &from);
    let to = page.text_field(
        &times,
        Style::Outlined,
        "To (HH:mm)",
        || config::value_str("/light/night/to").unwrap_or_else(|| "06:30".to_owned()),
        |text| config::store_value("/light/night/to", Value::from(text.trim())),
    );
    page.refresh_text_on("/light/night/to", &to);
    let schedule_enabled = move || {
        let automatic = config::value_bool(NIGHT_AUTOMATIC, true);
        from.set_enabled(automatic);
        to.set_enabled(automatic);
    };
    schedule_enabled();
    page.watch(NIGHT_AUTOMATIC, schedule_enabled);
    page.config_spin(
        &night,
        "thermostat",
        "Color temperature (K)",
        "/light/night/colorTemperature",
        5000,
        (1000, 6500),
        100,
    );

    state.widgets.replace(Some(Widgets {
        arrangement_section: arranged.parent(),
        arrangement,
        choices,
        selection: RefCell::new(None),
        use_as_section: use_as_group.parent(),
        use_as,
        resolution,
        rate,
        rotation,
        vrr,
        profile,
        depth,
        wide,
        hdr,
        auto_hdr,
        eotf,
        icc,
        spins,
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(state) = state.upgrade() {
                state.refresh();
            }
        }
    });
    state.displays.connect_changed({
        let refresh = refresh.clone();
        move || refresh()
    });
    state.options.connect_changed(move || refresh());
    state.refresh();
    page.keep(state);
    page
}

fn can_turn_off(monitor: &Monitor, others: &[Monitor]) -> bool {
    monitor.disabled || others.iter().any(|other| !other.disabled)
}

fn keyed_combo(
    page: &Page,
    state: &Rc<State>,
    parent: &gtk4::Box,
    title: &str,
    tip: &str,
    icon: &str,
    key: impl Fn(usize) -> (&'static str, String) + 'static,
) -> Rc<ComboBox> {
    let group = page.subsection(parent, title, tip);
    let combo = page.combo(&group, icon);
    combo.connect_activated({
        let state = Rc::downgrade(state);
        move |index| {
            if let Some(state) = state.upgrade() {
                let (name, value) = key(index);
                state.apply(&[(name, value)]);
            }
        }
    });
    combo
}

#[allow(clippy::too_many_arguments)]
fn option_spin(
    page: &Page,
    state: &Rc<State>,
    parent: &gtk4::Box,
    icon: &str,
    label: &str,
    range: (i64, i64),
    step: i64,
    decimals: u32,
    current: Box<dyn Fn(&Displays, &str) -> i64>,
    commit: Commit,
) -> (gtk4::Box, Spin) {
    let spin = SpinBox::new(&page.theme, range.0, range.1, step, decimals);
    spin.connect_changed({
        let state = Rc::downgrade(state);
        move |value| {
            let Some(state) = state.upgrade() else {
                return;
            };
            if let Some(monitor) = state.monitor() {
                commit(&state.displays, &monitor, value);
            }
        }
    });
    let row = page.spin_row(parent, icon, label, &spin);
    (row, Spin { spin, current })
}

fn rule_spin(
    page: &Page,
    state: &Rc<State>,
    parent: &gtk4::Box,
    rule: &RuleSpin,
) -> (gtk4::Box, Spin) {
    let (key, factor) = (rule.key, rule.factor);
    option_spin(
        page,
        state,
        parent,
        rule.icon,
        rule.label,
        rule.range,
        rule.step,
        rule.decimals,
        Box::new(move |displays, name| (displays.number_of(name, key) * factor).round() as i64),
        Box::new(move |displays, monitor, value| {
            displays.apply(monitor, &[(key.to_owned(), number(value as f64 / factor))]);
        }),
    )
}

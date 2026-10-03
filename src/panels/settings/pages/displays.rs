use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::arrangement::Arrangement;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::hyprrows;
use crate::platform::hypr;
use crate::platform::hyprconfig::Area;
use crate::services::displays::{Displays, Monitor, number, rates_of, shown_modes_of};
use crate::services::hyproptions::HyprOptions;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::controls::ComboBox;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::spinbox::SpinBox;

const AUTO_HDR: &str = "render:cm_auto_hdr";
const GLOBAL_VRR: &str = "misc:vrr";
const ZERO_SCALING: &str = "xwayland:force_zero_scaling";
pub const OPTIONS: [&str; 3] = [AUTO_HDR, GLOBAL_VRR, ZERO_SCALING];
const GLOBAL_VRR_MODES: [(&str, &str); 4] = [
    ("Off", "0"),
    ("On", "1"),
    ("Fullscreen only", "2"),
    ("Fullscreen games and video", "3"),
];

type Commit = Box<dyn Fn(&Rc<Displays>, &Monitor, i64)>;

pub(super) struct Spin {
    spin: Rc<SpinBox>,
    current: Box<dyn Fn(&Displays, &str) -> i64>,
}

pub(super) struct MainWidgets {
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
    auto_hdr: Rc<ComboBox>,
}

pub(super) struct ColorWidgets {
    pub(super) profile: Rc<ComboBox>,
    pub(super) depth: Rc<ComboBox>,
    pub(super) wide: Rc<ComboBox>,
    pub(super) hdr: Rc<ComboBox>,
    pub(super) eotf: Rc<ComboBox>,
    pub(super) icc: Rc<ComboBox>,
}

pub(super) struct Widgets {
    pub(super) main: Option<MainWidgets>,
    pub(super) color: Option<ColorWidgets>,
    pub(super) spins: Vec<Spin>,
}

pub(super) struct State {
    theme: SharedTheme,
    pub(super) displays: Rc<Displays>,
    options: Rc<HyprOptions>,
    selected: RefCell<String>,
    all_resolutions: Cell<bool>,
    pub(super) widgets: RefCell<Option<Widgets>>,
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
pub(super) const FORCED: [(&str, i64); 3] = [("Automatic", 0), ("On", 1), ("Off", -1)];
pub(super) const DEPTHS: [(&str, i64); 2] = [("8-bit", 8), ("10-bit", 10)];
const AUTO_HDR_MODES: [(&str, i64); 3] = [("Off", 0), ("HDR", 1), ("HDR (display profile)", 2)];
pub(super) const EOTFS: [(&str, &str); 5] = [
    ("Default", "default"),
    ("Automatic", "auto"),
    ("sRGB", "srgb"),
    ("Gamma 2.2", "gamma22"),
    ("Gamma 2.2, forced", "gamma22force"),
];

pub(super) struct RuleSpin {
    pub(super) key: &'static str,
    icon: &'static str,
    label: &'static str,
    factor: f64,
    decimals: u32,
    range: (i64, i64),
    step: i64,
}

pub(super) const SDR_RULES: [RuleSpin; 4] = [
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

pub(super) const DISPLAY_RULES: [RuleSpin; 3] = [
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
    list.iter().map(|(label, _)| tr(label)).collect()
}

pub(super) fn color_profiles(monitor: &Monitor) -> Vec<(String, &'static str)> {
    let model = |fallback: String| {
        if monitor.model.is_empty() {
            fallback
        } else {
            monitor.model.clone()
        }
    };
    vec![
        (tr("Automatic"), "auto"),
        (tr("sRGB"), "srgb"),
        (tr("DCI P3"), "dcip3"),
        (tr("Display P3"), "dp3"),
        (tr("Adobe RGB"), "adobe"),
        (tr("Wide color"), "wide"),
        (model(tr("Display profile")), "edid"),
        (tr("HDR"), "hdr"),
        (trf("HDR (%1)", &[&model(tr("display profile"))]), "hdredid"),
    ]
}

impl State {
    pub(super) fn new(theme: &SharedTheme, selected: String) -> Rc<State> {
        Rc::new(State {
            theme: theme.clone(),
            displays: Displays::new(),
            options: HyprOptions::new(Area::Displays, &OPTIONS),
            selected: RefCell::new(selected),
            all_resolutions: Cell::new(false),
            widgets: RefCell::new(None),
        })
    }

    pub(super) fn follow_changes(self: &Rc<Self>) {
        let refresh: Rc<dyn Fn()> = Rc::new({
            let state = Rc::downgrade(self);
            move || {
                if let Some(state) = state.upgrade() {
                    state.refresh();
                }
            }
        });
        self.displays.connect_changed({
            let refresh = refresh.clone();
            move || refresh()
        });
        self.options.connect_changed(move || refresh());
        self.refresh();
    }

    fn monitors(&self) -> Vec<Monitor> {
        self.displays.monitors.borrow().clone()
    }

    pub(super) fn monitor(&self) -> Option<Monitor> {
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
        let Some(monitor) = self.monitor() else {
            return;
        };
        if let Some(main) = &widgets.main {
            self.refresh_main(main, &monitor);
        }
        if let Some(color) = &widgets.color {
            self.refresh_color(color, &monitor);
        }
        for spin in &widgets.spins {
            spin.spin
                .set_value((spin.current)(&self.displays, &monitor.name));
        }
    }

    fn refresh_main(self: &Rc<Self>, widgets: &MainWidgets, monitor: &Monitor) {
        let monitors = self.monitors();
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
        let mut use_as_labels = vec![tr("Main display"), tr("Extended display")];
        for other in &others {
            use_as_values.push(other.name.clone());
            use_as_labels.push(trf(
                "Mirror for %1",
                &[if other.model.is_empty() {
                    other.name.as_str()
                } else {
                    other.model.as_str()
                }],
            ));
        }
        if can_turn_off(&monitor, &others) {
            use_as_values.push("off".to_owned());
            use_as_labels.push(tr("Off"));
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
                    trf(
                        "%1 × %2 (Default)",
                        &[&mode.width.to_string(), &mode.height.to_string()],
                    )
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
        let rate_labels: Vec<String> = rates
            .iter()
            .map(|rate| trf("%1 Hz", &[&rate.to_string()]))
            .collect();
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
        widgets.auto_hdr.set_items(
            &labels(&AUTO_HDR_MODES),
            position(
                &AUTO_HDR_MODES.map(|(_, value)| value),
                &(self.options.number_or(AUTO_HDR, 1.0) as i64),
            ),
        );
    }

    fn refresh_color(&self, widgets: &ColorWidgets, monitor: &Monitor) {
        let name = monitor.name.clone();
        let rule_number = |key: &str| self.displays.number_of(&name, key) as i64;
        let profiles = color_profiles(monitor);
        let profile_labels: Vec<String> = profiles.iter().map(|(label, _)| label.clone()).collect();
        let profile_values: Vec<&str> = profiles.iter().map(|(_, value)| *value).collect();
        let current_profile = self.displays.color_profile_of(monitor);
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
        let eotf = self.displays.value_of(&name, "sdr_eotf");
        widgets.eotf.set_items(
            &labels(&EOTFS),
            position(&EOTFS.map(|(_, value)| value), &eotf.as_str()),
        );
        let icc = self.displays.value_of(&name, "icc");
        let mut icc_values = vec![String::new()];
        let mut icc_labels = vec![tr("None")];
        for path in &self.displays.icc_profiles {
            icc_values.push(path.clone());
            icc_labels.push(path.rsplit('/').next().unwrap_or(path).to_owned());
        }
        widgets
            .icc
            .set_items(&icc_labels, position(&icc_values, &icc));
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let state = State::new(&page.theme, String::new());

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
    let (rescan, _) = page.icon_button("refresh", true, &tr("Rescan displays"), {
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
        &tr("Asks every monitor what it can do again. Modes a display only reports after it is fully awake show up after this."),
    );

    let use_as_group = page.subsection(&main, &tr("Use as"), "");
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

    let resolution_group = page.subsection(&main, &tr("Resolution"), "");
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
    let all = page.switch(&main, "list", &tr("Show all resolutions"), {
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

    let rate_group = page.subsection(&main, &tr("Refresh rate"), "");
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

    let rotation = keyed_combo(
        &page,
        &state,
        &main,
        &tr("Rotation"),
        "",
        "screen_rotation",
        move |index| ("transform", ROTATIONS[index].1.to_string()),
    );
    let vrr = keyed_combo(
        &page,
        &state,
        &main,
        &tr("Variable refresh rate"),
        "",
        "sync",
        move |index| ("vrr", VRR[index].1.to_string()),
    );
    let open_color = context.subpage_opener_with("displaycolor");
    page.link_row(
        &main,
        "palette",
        &tr("Color"),
        &tr("Color profile, HDR and luminance of this display"),
        {
            let state = Rc::downgrade(&state);
            move || {
                if let Some(monitor) = state.upgrade().and_then(|state| state.monitor()) {
                    open_color(&monitor.name);
                }
            }
        },
    );

    let mut spins = Vec::new();
    let reserved = page.section("border_outer", &tr("Reserved area"));
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
            &tr(label),
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

    let every = page.section("desktop_windows", &tr("All displays"));
    let auto_group = page.subsection(
        &every,
        &tr("Auto HDR"),
        &tr("Switches to HDR while a fullscreen window has HDR content."),
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
        &tr("Variable refresh rate"),
        &tr("What a display set to follow the global setting does."),
    );
    hyprrows::combo(
        &page,
        &sync_group,
        &state.options,
        "sync",
        (GLOBAL_VRR, "0"),
        &GLOBAL_VRR_MODES,
    );
    let xwayland = page.subsection(&every, &tr("X11 apps"), "");
    let sharp = hyprrows::switch(
        &page,
        &xwayland,
        &state.options,
        "high_density",
        &tr("Keep X11 apps sharp on scaled displays"),
        ZERO_SCALING,
    );
    page.tip(
        &sharp.button,
        &tr("X11 apps are drawn unscaled instead of stretched. They look sharp, and small unless they scale themselves (GDK_SCALE, QT_SCALE_FACTOR)."),
    );

    let night = page.section("", "");
    page.link_row(
        &night,
        "nightlight",
        &tr("Night light"),
        &tr("Schedule and color temperature"),
        context.subpage_opener("nightlight"),
    );

    state.widgets.replace(Some(Widgets {
        main: Some(MainWidgets {
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
            auto_hdr,
        }),
        color: None,
        spins,
    }));
    state.follow_changes();
    page.keep(state);
    page
}

fn can_turn_off(monitor: &Monitor, others: &[Monitor]) -> bool {
    monitor.disabled || others.iter().any(|other| !other.disabled)
}

pub(super) fn keyed_combo(
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

pub(super) fn rule_spin(
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
        &tr(rule.label),
        rule.range,
        rule.step,
        rule.decimals,
        Box::new(move |displays, name| (displays.number_of(name, key) * factor).round() as i64),
        Box::new(move |displays, monitor, value| {
            displays.apply(monitor, &[(key.to_owned(), number(value as f64 / factor))]);
        }),
    )
}

pub mod builder;
pub mod displaycolor;
pub mod nightlight;
mod presenter;
mod primary;
pub mod rules;

use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::arrangement::Arrangement;
use crate::platform::hyprland::hyprconfig::Area;
use crate::services::hardware::displays::{Displays, Monitor};
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ComboBox;
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::spinbox::SpinBox;

const AUTO_HDR: &str = "render:cm_auto_hdr";
const GLOBAL_VRR: &str = "misc:vrr";
const ZERO_SCALING: &str = "xwayland:force_zero_scaling";
const TEARING: &str = "general:allow_tearing";
pub const OPTIONS: [&str; 4] = [AUTO_HDR, GLOBAL_VRR, TEARING, ZERO_SCALING];
const GLOBAL_VRR_MODES: [(&str, &str); 4] = [
    ("Off", "0"),
    ("On", "1"),
    ("Fullscreen only", "2"),
    ("Fullscreen games and video", "3"),
];

type Commit = Box<dyn Fn(&Rc<Displays>, &Monitor, i64)>;

pub(super) struct Spin {
    spin: Rc<SpinBox>,
    read_current: Box<dyn Fn(&Displays, &str) -> i64>,
}

pub(super) struct MainWidgets {
    arrangement_root: Option<gtk4::Widget>,
    arrangement: Rc<Arrangement>,
    choices: gtk4::Box,
    selection: RefCell<Option<(Vec<String>, Rc<Selection>)>>,
    use_as_root: Option<gtk4::Widget>,
    use_as_combo: Rc<ComboBox>,
    resolution_combo: Rc<ComboBox>,
    rate_combo: Rc<ComboBox>,
    rotation_combo: Rc<ComboBox>,
    vrr_combo: Rc<ComboBox>,
    vrr_notice: gtk4::Box,
    auto_hdr_root: gtk4::Widget,
    auto_hdr_combo: Rc<ComboBox>,
    global_vrr_combo: Rc<ComboBox>,
    global_vrr_notice: gtk4::Box,
}

pub(super) struct ColorWidgets {
    pub(super) profile_combo: Rc<ComboBox>,
    pub(super) depth_combo: Rc<ComboBox>,
    pub(super) wide_color_combo: Rc<ComboBox>,
    pub(super) hdr_combo: Rc<ComboBox>,
    pub(super) eotf_combo: Rc<ComboBox>,
    pub(super) icc_combo: Rc<ComboBox>,
    pub(super) luminance_root: Option<gtk4::Widget>,
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
    selected_name: RefCell<String>,
    shows_all_resolutions: Cell<bool>,
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
const AUTO_HDR_MODES: [(&str, i64); 3] = [("Off", 0), ("HDR", 1), ("HDR (display profile)", 2)];

pub(super) fn color_profiles(monitor: &Monitor, includes_hdr: bool) -> Vec<(String, &'static str)> {
    let model_or_else = |fallback: String| {
        if monitor.model.is_empty() {
            fallback
        } else {
            monitor.model.clone()
        }
    };
    let mut profiles = vec![
        (tr("Automatic"), "auto"),
        (tr("sRGB"), "srgb"),
        (tr("DCI P3"), "dcip3"),
        (tr("Display P3"), "dp3"),
        (tr("Adobe RGB"), "adobe"),
        (tr("Wide color"), "wide"),
        (model_or_else(tr("Display profile")), "edid"),
    ];
    if includes_hdr {
        profiles.push((tr("HDR"), "hdr"));
        profiles.push((
            trf("HDR (%1)", &[&model_or_else(tr("display profile"))]),
            "hdredid",
        ));
    }
    profiles
}

fn can_show_hdr(is_capable: Option<bool>, force_mode: i64, profile: &str) -> bool {
    is_capable.unwrap_or(true) || force_mode == 1 || profile.starts_with("hdr")
}

fn can_change_vrr(is_capable: Option<bool>, mode: i64) -> bool {
    is_capable.unwrap_or(true) || mode != -1
}

fn can_change_global_vrr(global_mode: i64, can_change_displays: &[bool]) -> bool {
    global_mode != 0 || can_change_displays.contains(&true)
}

impl State {
    pub(super) fn new(theme: &SharedTheme, selected_name: String) -> Rc<State> {
        Rc::new(State {
            theme: theme.clone(),
            displays: Displays::new(),
            options: HyprOptions::new(Area::Displays, &OPTIONS),
            selected_name: RefCell::new(selected_name),
            shows_all_resolutions: Cell::new(false),
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
        let selected_name = self.selected_name.borrow().clone();
        monitors
            .iter()
            .find(|monitor| monitor.name == selected_name)
            .or(monitors.first())
            .cloned()
    }

    fn offers_hdr(&self, monitor: &Monitor) -> bool {
        can_show_hdr(
            monitor.is_hdr_capable,
            self.displays.number_of(&monitor.name, "supports_hdr") as i64,
            &self.displays.color_profile_of(monitor),
        )
    }

    fn allows_vrr(&self, monitor: &Monitor) -> bool {
        can_change_vrr(
            monitor.is_vrr_capable,
            self.displays.number_of(&monitor.name, "vrr") as i64,
        )
    }

    fn others(&self, monitor: &Monitor) -> Vec<Monitor> {
        self.monitors()
            .into_iter()
            .filter(|other| other.name != monitor.name)
            .collect()
    }

    fn mirror_target_name(&self, monitor: &Monitor) -> String {
        if monitor.mirror_of == "none" {
            return String::new();
        }
        self.monitors()
            .iter()
            .find(|other| other.id.to_string() == monitor.mirror_of)
            .map_or_else(|| monitor.mirror_of.clone(), |other| other.name.clone())
    }

    fn apply(&self, keys: &[(&str, String)]) {
        self.apply_with_primary(keys, None);
    }

    fn apply_with_primary(&self, keys: &[(&str, String)], primary: Option<&str>) {
        let Some(monitor) = self.monitor() else {
            return;
        };
        let keys: Vec<(String, String)> = keys
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        self.displays.apply_with_primary(&monitor, &keys, primary);
    }
}

pub fn show_write_errors(displays: &Rc<Displays>, parent: &gtk4::Box) {
    let write_status = text::styled("");
    text::set_color(&write_status, "colError");
    write_status.set_xalign(0.0);
    write_status.set_wrap(true);
    write_status.set_visible(false);
    parent.append(&write_status);
    displays.connect_changed({
        let displays = Rc::downgrade(displays);
        let write_status = write_status.downgrade();
        move || {
            let (Some(displays), Some(write_status)) = (displays.upgrade(), write_status.upgrade())
            else {
                return;
            };
            let write_error = displays.write_error.borrow();
            if let Some(error) = write_error.as_deref() {
                write_status.set_text(&trf("displays.lua could not be written: %1", &[error]));
            }
            write_status.set_visible(write_error.is_some());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdr_rows_show_for_a_display_that_can_do_hdr_or_is_set_to() {
        for (is_capable, force_mode, profile, expected) in [
            (Some(true), 0, "srgb", true),
            (Some(false), 0, "srgb", false),
            (Some(false), -1, "auto", false),
            (Some(false), 1, "srgb", true),
            (Some(false), 0, "hdr", true),
            (Some(false), 0, "hdredid", true),
            (None, 0, "srgb", true),
        ] {
            assert_eq!(
                can_show_hdr(is_capable, force_mode, profile),
                expected,
                "{is_capable:?} {force_mode} {profile}"
            );
        }
    }

    #[test]
    fn the_vrr_choices_are_enabled_for_a_display_that_can_do_vrr_or_is_set_to() {
        for (is_capable, mode, expected) in [
            (Some(true), -1, true),
            (Some(false), -1, false),
            (Some(false), 0, true),
            (Some(false), 3, true),
            (None, -1, true),
        ] {
            assert_eq!(
                can_change_vrr(is_capable, mode),
                expected,
                "{is_capable:?} {mode}"
            );
        }
    }

    #[test]
    fn the_global_vrr_choices_are_enabled_while_a_display_can_use_them_or_they_are_set() {
        for (global_mode, displays, expected) in [
            (0, &[true, false][..], true),
            (0, &[false, false][..], false),
            (0, &[][..], false),
            (2, &[false][..], true),
        ] {
            assert_eq!(
                can_change_global_vrr(global_mode, displays),
                expected,
                "{global_mode} {displays:?}"
            );
        }
    }

    #[test]
    fn the_hdr_color_profiles_are_listed_only_where_hdr_shows() {
        let values = |includes_hdr: bool| -> Vec<&str> {
            color_profiles(&Monitor::default(), includes_hdr)
                .into_iter()
                .map(|(_, value)| value)
                .collect()
        };
        assert_eq!(
            values(false),
            ["auto", "srgb", "dcip3", "dp3", "adobe", "wide", "edid"]
        );
        assert_eq!(
            values(true),
            [
                "auto", "srgb", "dcip3", "dp3", "adobe", "wide", "edid", "hdr", "hdredid"
            ]
        );
    }
}

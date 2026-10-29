use std::rc::Rc;

use super::{Commit, Spin, State};
use crate::core::i18n::tr;
use crate::panels::settings::content::Page;
use crate::services::hardware::displays::{Displays, format_number};
use crate::ui::widgets::input::controls::ComboBox;
use crate::ui::widgets::input::spinbox::SpinBox;

pub(in super::super) const FORCE_MODES: [(&str, i64); 3] =
    [("Automatic", 0), ("On", 1), ("Off", -1)];
pub(in super::super) const DEPTHS: [(&str, i64); 2] = [("8-bit", 8), ("10-bit", 10)];
pub(in super::super) const EOTFS: [(&str, &str); 5] = [
    ("Default", "default"),
    ("Automatic", "auto"),
    ("sRGB", "srgb"),
    ("Gamma 2.2", "gamma22"),
    ("Gamma 2.2, forced", "gamma22force"),
];

pub(in super::super) struct RuleSpin {
    pub(in super::super) key: &'static str,
    icon: &'static str,
    label: &'static str,
    factor: f64,
    decimals: u32,
    range: (i64, i64),
    step: i64,
}

pub(in super::super) const SDR_RULES: [RuleSpin; 4] = [
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

pub(in super::super) const DISPLAY_RULES: [RuleSpin; 3] = [
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

pub(in super::super) fn keyed_combo(
    page: &Page,
    state: &Rc<State>,
    parent: &gtk4::Box,
    title: &str,
    tip: &str,
    icon: &str,
    rule_for: impl Fn(usize) -> (&'static str, String) + 'static,
) -> Rc<ComboBox> {
    let group = page.subsection(parent, title, tip);
    keyed_combo_in(page, state, &group, icon, rule_for)
}

pub(in super::super) fn keyed_combo_in(
    page: &Page,
    state: &Rc<State>,
    group: &gtk4::Box,
    icon: &str,
    rule_for: impl Fn(usize) -> (&'static str, String) + 'static,
) -> Rc<ComboBox> {
    let combo = page.combo(group, icon);
    combo.connect_activated({
        let state = Rc::downgrade(state);
        move |index| {
            if let Some(state) = state.upgrade() {
                let (key, value) = rule_for(index);
                state.apply(&[(key, value)]);
            }
        }
    });
    combo
}

#[allow(clippy::too_many_arguments)]
pub(super) fn option_spin(
    page: &Page,
    state: &Rc<State>,
    parent: &gtk4::Box,
    icon: &str,
    label: &str,
    range: (i64, i64),
    step: i64,
    decimals: u32,
    read_current: Box<dyn Fn(&Displays, &str) -> i64>,
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
    (row, Spin { spin, read_current })
}

pub(in super::super) fn rule_spin(
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
            displays.apply(
                monitor,
                &[(key.to_owned(), format_number(value as f64 / factor))],
            );
        }),
    )
}

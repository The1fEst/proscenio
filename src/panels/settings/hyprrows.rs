use std::rc::Rc;

use gtk4::prelude::*;
use serde_json::Value;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Page, Parent};
use crate::platform::hyprland::{hypr, hyprconfig};
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::ui::widgets::input::controls::{ComboBox, ConfigSwitch};
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::spinbox::SpinBox;

pub fn switch(
    page: &Page,
    parent: &impl Parent,
    options: &Rc<HyprOptions>,
    icon: &str,
    label: &str,
    option: &'static str,
) -> Rc<ConfigSwitch> {
    option_switch(
        page,
        parent,
        options,
        (icon, label),
        move |options| options.flag(option),
        move |options, is_on| options.set(option, &is_on.to_string()),
    )
}

pub fn lines_switch(
    page: &Page,
    parent: &impl Parent,
    icon: &str,
    label: &str,
    lines: &'static hyprconfig::Lines,
) -> Rc<ConfigSwitch> {
    let switch = page.switch(parent, icon, label, move |is_on| {
        if hyprconfig::are_lines_present(lines) == is_on {
            return;
        }
        let _ = hyprconfig::set_lines(lines, is_on);
        hypr::request("reload");
    });
    switch.bind(move || hyprconfig::are_lines_present(lines));
    switch
}

pub fn option_switch(
    page: &Page,
    parent: &impl Parent,
    options: &Rc<HyprOptions>,
    (icon, label): (&str, impl AsRef<str>),
    read_current: impl Fn(&HyprOptions) -> bool + 'static,
    on_committed: impl Fn(&Rc<HyprOptions>, bool) + 'static,
) -> Rc<ConfigSwitch> {
    let read_current = Rc::new(read_current);
    let switch = page.switch(parent, icon, label.as_ref(), {
        let options = Rc::downgrade(options);
        let read_current = read_current.clone();
        move |is_on| {
            if let Some(options) = options.upgrade()
                && read_current(&options) != is_on
            {
                on_committed(&options, is_on);
            }
        }
    });
    switch.bind({
        let options = Rc::downgrade(options);
        move || {
            options
                .upgrade()
                .is_some_and(|options| read_current(&options))
        }
    });
    options.connect_changed({
        let switch = Rc::downgrade(&switch);
        move || {
            if let Some(switch) = switch.upgrade() {
                switch.refresh();
            }
        }
    });
    switch
}

pub struct Spin {
    pub icon: &'static str,
    pub label: &'static str,
    pub option: &'static str,
    pub factor: f64,
    pub range: (i64, i64),
    pub step: i64,
    pub decimals: u32,
}

pub fn spin(
    page: &Page,
    parent: &impl Parent,
    options: &Rc<HyprOptions>,
    spec: &Spin,
) -> (gtk4::Box, Rc<SpinBox>) {
    let (option, factor) = (spec.option, spec.factor);
    let spin = SpinBox::new(
        &page.theme,
        spec.range.0,
        spec.range.1,
        spec.step,
        spec.decimals,
    );
    let read_current =
        move |options: &HyprOptions| (options.number(option) * factor).round() as i64;
    spin.set_value(read_current(options));
    spin.connect_changed({
        let options = Rc::downgrade(options);
        move |value| {
            if let Some(options) = options.upgrade() {
                options.set(option, &format!("{}", value as f64 / factor));
            }
        }
    });
    options.connect_changed({
        let spin = Rc::downgrade(&spin);
        let options = Rc::downgrade(options);
        move || {
            if let (Some(spin), Some(options)) = (spin.upgrade(), options.upgrade()) {
                spin.set_value(read_current(&options));
            }
        }
    });
    let row = page.spin_row(parent, spec.icon, &tr(spec.label), &spin);
    (row, spin)
}

pub fn combo(
    page: &Page,
    parent: &gtk4::Box,
    options: &Rc<HyprOptions>,
    icon: &str,
    (option, fallback): (&'static str, &'static str),
    list: &'static [(&'static str, &'static str)],
) -> Rc<ComboBox> {
    let combo = page.combo(parent, icon);
    let labels: Vec<String> = list.iter().map(|(label, _)| tr(label)).collect();
    let show = {
        let combo = Rc::downgrade(&combo);
        let options = Rc::downgrade(options);
        move || {
            let (Some(combo), Some(options)) = (combo.upgrade(), options.upgrade()) else {
                return;
            };
            let mut current_value = options.text(option);
            if current_value.is_empty() {
                current_value = fallback.to_owned();
            }
            let index = list
                .iter()
                .position(|(_, value)| *value == current_value)
                .unwrap_or(0);
            combo.set_items(&labels, index as i32);
        }
    };
    show();
    options.connect_changed(show);
    combo.connect_activated({
        let options = Rc::downgrade(options);
        move |index| {
            if let Some(options) = options.upgrade() {
                options.set(option, list[index].1);
            }
        }
    });
    combo
}

pub fn selection(
    page: &Page,
    parent: &gtk4::Box,
    options: &Rc<HyprOptions>,
    choices: Vec<Choice>,
    read_current: impl Fn(&HyprOptions) -> Value + 'static,
    on_selected: impl Fn(&Rc<HyprOptions>, Value) + 'static,
) -> Rc<Selection> {
    let selection = Selection::new(&page.theme, choices, {
        let options = Rc::downgrade(options);
        move |value| {
            if let Some(options) = options.upgrade() {
                on_selected(&options, value);
            }
        }
    });
    selection.set_current(&read_current(options));
    options.connect_changed({
        let selection = Rc::downgrade(&selection);
        let options = Rc::downgrade(options);
        move || {
            if let (Some(selection), Some(options)) = (selection.upgrade(), options.upgrade()) {
                selection.set_current(&read_current(&options));
            }
        }
    });
    parent.append(&selection.root);
    page.keep(selection.clone());
    selection
}

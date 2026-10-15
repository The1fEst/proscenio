use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use super::{Choice, Page, Parent, Style};
use crate::core::config;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::{self, ComboBox, ConfigSwitch};
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::slider::{Options, Slider};
use crate::ui::widgets::input::spinbox::SpinBox;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;

const ROW_MARGIN: i32 = 8;
const ROW_SPACING: i32 = 10;
const COMBO_ROW_WIDTH: i32 = 220;
const DISABLED_OPACITY: f64 = 0.4;
const SLIDER_TRACK: f64 = 12.0;
const SLIDER_LABEL: i32 = 120;
const SLIDER_SPACING: i32 = 10;
const SLIDER_MARGIN: i32 = 8;

pub fn slider_row(
    theme: &SharedTheme,
    parent: &impl Parent,
    icon: &str,
    label: &str,
    range: (f64, f64),
) -> (Rc<Slider>, gtk4::Label) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SLIDER_SPACING);
    row.set_margin_start(SLIDER_MARGIN);
    row.set_margin_end(SLIDER_MARGIN);
    let symbol = text::symbol(icon, pixel_size::LARGER as f64);
    text::set_color(&symbol, "m3onBackground");
    row.append(&Centered::integral(&symbol));
    let name_label = text::styled(label);
    text::set_color(&name_label, "colOnSecondaryContainer");
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name_label.set_max_width_chars(1);
    name_label.set_size_request(SLIDER_LABEL, -1);
    row.append(&Centered::new(&name_label));
    let slider = new_slider(theme, range);
    row.append(&slider.area);
    parent.add(&row);
    (slider, symbol)
}

fn labeled_row(icon: &str, label: &str) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    row.set_margin_start(ROW_MARGIN);
    row.set_margin_end(ROW_MARGIN);
    if !icon.is_empty() {
        let symbol = text::symbol(icon, pixel_size::LARGER as f64);
        text::set_color(&symbol, "colOnSecondaryContainer");
        row.append(&Centered::integral(&symbol));
    }
    let name_label = text::styled(label);
    text::set_color(&name_label, "colOnSecondaryContainer");
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let name_box = Centered::filling_width(&name_label);
    name_box.set_hexpand(true);
    row.append(&name_box);
    row
}

pub fn new_slider(theme: &SharedTheme, range: (f64, f64)) -> Rc<Slider> {
    let slider = Slider::with(
        theme,
        Options {
            track: SLIDER_TRACK,
            from: range.0,
            to: range.1,
            icon: None,
            secondary: None,
            dividers: Vec::new(),
        },
    );
    slider.set_stops(vec![1.0]);
    slider.area.set_hexpand(true);
    slider
}

impl Page {
    pub fn switch(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        on_toggled: impl Fn(bool) + 'static,
    ) -> Rc<ConfigSwitch> {
        let switch = ConfigSwitch::new(&self.theme, icon, label, on_toggled);
        switch.button.set_hexpand(true);
        parent.add(&switch.button);
        self.keep(switch.clone());
        switch
    }

    pub fn config_switch(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: bool,
    ) -> Rc<ConfigSwitch> {
        let switch = self.switch(parent, icon, label, move |is_on| {
            config::store_value(pointer, Value::Bool(is_on));
        });
        switch.bind(move || config::value_bool(pointer, default));
        self.refresh_on(pointer, &switch);
        self.track(pointer, Value::Bool(default), &switch.button);
        switch
    }

    pub fn refresh_on(&self, pointer: &str, switch: &Rc<ConfigSwitch>) {
        let switch = Rc::downgrade(switch);
        self.watch(pointer, move || {
            if let Some(switch) = switch.upgrade() {
                switch.refresh();
            }
        });
    }

    pub fn spin_row(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        spin: &Rc<SpinBox>,
    ) -> gtk4::Box {
        self.keep(spin.clone());
        Self::unkept_spin_row(parent, icon, label, spin)
    }

    pub fn unkept_spin_row(
        parent: &impl Parent,
        icon: &str,
        label: &str,
        spin: &Rc<SpinBox>,
    ) -> gtk4::Box {
        let row = labeled_row(icon, label);
        row.append(&spin.root);
        parent.add(&row);
        row
    }

    pub fn combo_row(&self, parent: &impl Parent, icon: &str, label: &str) -> Rc<ComboBox> {
        let combo = ComboBox::new(&self.theme);
        combo
            .button
            .set_size_request(COMBO_ROW_WIDTH, combo.button.size_request().1);
        combo.button.set_hexpand(false);
        let row = labeled_row(icon, label);
        row.append(&combo.button);
        parent.add(&row);
        self.keep(combo.clone());
        combo
    }

    #[allow(clippy::too_many_arguments)]
    pub fn config_spin(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: i64,
        range: (i64, i64),
        step: i64,
    ) -> (gtk4::Box, Rc<SpinBox>) {
        let spin = SpinBox::new(&self.theme, range.0, range.1, step, 0);
        spin.set_value(config::value_i64(pointer, default));
        spin.connect_changed(move |value| config::store_value(pointer, Value::from(value)));
        self.watch(pointer, {
            let spin = Rc::downgrade(&spin);
            move || {
                if let Some(spin) = spin.upgrade() {
                    spin.set_value(config::value_i64(pointer, default));
                }
            }
        });
        let row = self.spin_row(parent, icon, label, &spin);
        self.track(pointer, Value::from(default), &row);
        (row, spin)
    }

    /// A spin box over a fractional setting, shown multiplied by `scale`.
    #[allow(clippy::too_many_arguments)]
    pub fn config_spin_scaled(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: f64,
        scale: f64,
        range: (i64, i64),
        step: i64,
    ) -> (gtk4::Box, Rc<SpinBox>) {
        let read_current = move || (config::value_f64(pointer, default) * scale).round() as i64;
        let spin = SpinBox::new(&self.theme, range.0, range.1, step, 0);
        spin.set_value(read_current());
        spin.connect_changed(move |value| {
            config::store_value(pointer, Value::from(value as f64 / scale));
        });
        self.watch(pointer, {
            let spin = Rc::downgrade(&spin);
            move || {
                if let Some(spin) = spin.upgrade() {
                    spin.set_value(read_current());
                }
            }
        });
        let row = self.spin_row(parent, icon, label, &spin);
        self.track(pointer, Value::from(default), &row);
        (row, spin)
    }

    /// A spin box over a setting stored `factor` times larger than shown, such as seconds shown as minutes.
    #[allow(clippy::too_many_arguments)]
    pub fn config_spin_multiple(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: i64,
        factor: i64,
        range: (i64, i64),
        step: i64,
    ) -> (gtk4::Box, Rc<SpinBox>) {
        let read_current =
            move || (config::value_i64(pointer, default) as f64 / factor as f64).round() as i64;
        let spin = SpinBox::new(&self.theme, range.0, range.1, step, 0);
        spin.set_value(read_current());
        spin.connect_changed(move |value| {
            config::store_value(pointer, Value::from(value * factor))
        });
        self.watch(pointer, {
            let spin = Rc::downgrade(&spin);
            move || {
                if let Some(spin) = spin.upgrade() {
                    spin.set_value(read_current());
                }
            }
        });
        let row = self.spin_row(parent, icon, label, &spin);
        self.track(pointer, Value::from(default), &row);
        (row, spin)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn config_slider(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: f64,
        range: (f64, f64),
        stops: Vec<f64>,
    ) -> Rc<Slider> {
        let (slider, _) = slider_row(&self.theme, parent, icon, label, range);
        slider.set_stops(stops);
        let show = move |slider: &Slider| {
            let value = config::value_f64(pointer, default);
            slider.set(value);
            slider.set_tooltip(&format!("{}", value.round()));
        };
        show(&slider);
        slider.on_moved({
            let slider = Rc::downgrade(&slider);
            move |value| {
                if let Some(slider) = slider.upgrade() {
                    slider.set_tooltip(&format!("{}", value.round()));
                }
                config::store_value(pointer, Value::from(value.round() as i64));
            }
        });
        self.watch(pointer, {
            let slider = Rc::downgrade(&slider);
            move || {
                if let Some(slider) = slider.upgrade() {
                    show(&slider);
                }
            }
        });
        if let Some(row) = slider.area.parent() {
            self.track(pointer, Value::from(default), &row);
        }
        self.keep(slider.clone());
        slider
    }

    pub fn set_spin_row_enabled(row: &gtk4::Box, spin: &SpinBox, is_enabled: bool) {
        row.set_sensitive(is_enabled);
        let opacity = if is_enabled { 1.0 } else { DISABLED_OPACITY };
        let mut child = row.first_child();
        while let Some(widget) = child {
            if &widget != spin.root.upcast_ref::<gtk4::Widget>() {
                widget.set_opacity(opacity);
            }
            child = widget.next_sibling();
        }
        spin.set_enabled(is_enabled);
    }

    pub fn selection(
        &self,
        parent: &impl Parent,
        choices: Vec<Choice>,
        pointer: &'static str,
        default: Value,
        on_selected: impl Fn(Value) + 'static,
    ) -> Rc<Selection> {
        let default_value = default.clone();
        let read_current = move || config::value(pointer).unwrap_or_else(|| default.clone());
        let selection = self.selection_of(parent, choices, &[pointer], read_current, on_selected);
        self.track(pointer, default_value, &selection.root);
        selection
    }

    pub fn selection_of(
        &self,
        parent: &impl Parent,
        choices: Vec<Choice>,
        pointers: &[&str],
        read_current: impl Fn() -> Value + 'static,
        on_selected: impl Fn(Value) + 'static,
    ) -> Rc<Selection> {
        let selection = Selection::new(&self.theme, choices, on_selected);
        selection.set_current(&read_current());
        let read_current = Rc::new(read_current);
        for pointer in pointers {
            let selection = Rc::downgrade(&selection);
            let read_current = read_current.clone();
            self.watch(pointer, move || {
                if let Some(selection) = selection.upgrade() {
                    selection.set_current(&read_current());
                }
            });
        }
        parent.add(&selection.root);
        self.keep(selection.clone());
        selection
    }

    pub fn combo(&self, parent: &impl Parent, icon: &str) -> Rc<ComboBox> {
        let combo = ComboBox::new(&self.theme);
        combo.set_icon(icon);
        combo.button.set_hexpand(true);
        parent.add(&combo.button);
        self.keep(combo.clone());
        combo
    }

    pub fn text_field(
        &self,
        parent: &impl Parent,
        style: Style,
        placeholder: &str,
        read_current: impl Fn() -> String + 'static,
        on_committed: impl Fn(String) + 'static,
    ) -> Rc<TextField> {
        let field = TextField::new(&self.theme, style, placeholder);
        field.root.set_hexpand(true);
        field.connect_finished(on_committed);
        field.bind(read_current);
        parent.add(&field.root);
        self.keep(field.clone());
        field
    }

    pub fn secret_field(&self, parent: &impl Parent, placeholder: &str) -> Rc<TextField> {
        let field = TextField::secret(&self.theme, Style::Outlined, placeholder);
        field.root.set_hexpand(true);
        parent.add(&field.root);
        self.keep(field.clone());
        field
    }

    pub fn icon_button(
        &self,
        icon: &str,
        is_filled: bool,
        label: &str,
        action: impl Fn() + 'static,
    ) -> (RippleButton, gtk4::Label) {
        let (button, name_label) = controls::icon_button(&self.theme, icon, is_filled, label);
        button.set_halign(gtk4::Align::Start);
        button.connect_clicked(move |_| action());
        (button, name_label)
    }

    pub fn config_text(
        &self,
        parent: &impl Parent,
        style: Style,
        placeholder: &str,
        pointer: &'static str,
        default: &str,
    ) -> Rc<TextField> {
        let default_value = Value::from(default);
        let default = default.to_owned();
        let read_current = move || config::value_str(pointer).unwrap_or_else(|| default.clone());
        let field = self.text_field(parent, style, placeholder, read_current, move |text| {
            config::store_value(pointer, Value::from(text));
        });
        self.refresh_text_on(pointer, &field);
        self.track(pointer, default_value, &field.root);
        field
    }

    pub fn config_list(
        &self,
        parent: &impl Parent,
        placeholder: &str,
        pointer: &'static str,
        default: &'static [&'static str],
    ) -> Rc<TextField> {
        let read_current = move || {
            config::value(pointer)
                .and_then(|value| value.as_array().cloned())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| default.iter().map(|item| (*item).to_owned()).collect())
                .join(", ")
        };
        let field = self.text_field(
            parent,
            Style::Outlined,
            placeholder,
            read_current,
            move |text| {
                let items: Vec<String> = text
                    .split(',')
                    .map(|item| item.trim().to_owned())
                    .filter(|item| !item.is_empty())
                    .collect();
                config::store_value(pointer, Value::from(items));
            },
        );
        self.refresh_text_on(pointer, &field);
        self.track(pointer, Value::from(default.to_vec()), &field.root);
        field
    }

    pub fn refresh_text_on(&self, pointer: &str, field: &Rc<TextField>) {
        let field = Rc::downgrade(field);
        self.watch(pointer, move || {
            if let Some(field) = field.upgrade() {
                field.refresh();
            }
        });
    }
}

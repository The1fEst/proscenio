use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::ui::anim::{EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Tween};
use crate::ui::theme::{SharedTheme, Theme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;

const SCALE: f64 = 0.75;
const SWITCH_WIDTH: f64 = 52.0 * SCALE;
const SWITCH_HEIGHT: f64 = 32.0 * SCALE;
const SPATIAL_MILLIS: f64 = 350.0;
const FAST_MILLIS: f64 = 200.0;
const ICON_BUTTON_HEIGHT: i32 = 35;
const ICON_BUTTON_PADDING: i32 = 10;
const ICON_BUTTON_SPACING: i32 = 5;
const COMBO_PADDING: i32 = 16;
const COMBO_SPACING: i32 = 8;
const COMBO_POPUP_GAP: i32 = 4;
const COMBO_POPUP_SHADOW: i32 = 10;
const COMBO_POPUP_LIST: i32 = 300 - 16;
const COMBO_ITEM_HEIGHT: i32 = 40;
const COMBO_ITEM_SPACING: i32 = 2;

struct Knob {
    is_checked: Cell<bool>,
    is_pressed: Cell<bool>,
    size: Cell<Tween>,
    left: Cell<Tween>,
    tint: Cell<Tween>,
    is_ticking: Cell<bool>,
    scale: Cell<f64>,
}

pub struct Switch {
    pub area: gtk4::DrawingArea,
    knob: Rc<Knob>,
}

impl Switch {
    pub fn new(theme: &SharedTheme) -> Rc<Self> {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(SWITCH_WIDTH.round() as i32);
        area.set_content_height(SWITCH_HEIGHT.round() as i32);
        area.set_valign(gtk4::Align::Center);
        area.set_cursor_from_name(Some("pointer"));
        let knob = Rc::new(Knob {
            is_checked: Cell::new(false),
            is_pressed: Cell::new(false),
            size: Cell::new(Tween::new(16.0 * SCALE, SPATIAL_MILLIS, EXPRESSIVE_FAST)),
            left: Cell::new(Tween::new(8.0 * SCALE, SPATIAL_MILLIS, EXPRESSIVE_FAST)),
            tint: Cell::new(Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
            is_ticking: Cell::new(false),
            scale: Cell::new(1.0),
        });
        area.set_draw_func({
            let theme = theme.clone();
            let knob = knob.clone();
            move |area, cr, width, height| {
                let now = area
                    .frame_clock()
                    .map(|clock| clock.frame_time())
                    .unwrap_or_else(glib::monotonic_time);
                let theme = theme.borrow();
                let tint = knob.tint.get().value(now) as f32;
                let track = blend(
                    theme.colors.col_surface_container_highest,
                    theme.colors.col_primary,
                    tint,
                );
                let border = blend(theme.m3.outline, theme.colors.col_primary, tint);
                let thumb = blend(theme.m3.outline, theme.m3.on_primary, tint);
                let (width, height) = (width as f64, height as f64);
                let scale = knob.scale.get();
                cr.translate(width / 2.0, height / 2.0);
                cr.scale(scale, scale);
                cr.translate(-width / 2.0, -height / 2.0);
                let inset = (2.0 * SCALE).round();
                trace_pill(cr, 0.0, 0.0, width, height);
                trace_pill(cr, inset, inset, width - inset * 2.0, height - inset * 2.0);
                cr.set_fill_rule(gtk4::cairo::FillRule::EvenOdd);
                set_source(cr, border);
                let _ = cr.fill();
                cr.set_fill_rule(gtk4::cairo::FillRule::Winding);
                trace_pill(cr, inset, inset, width - inset * 2.0, height - inset * 2.0);
                set_source(cr, track);
                let _ = cr.fill();
                let size = knob.size.get().value(now);
                let left = knob.left.get().value(now);
                cr.new_path();
                cr.arc(left + size / 2.0, height / 2.0, size / 2.0, 0.0, 2.0 * PI);
                set_source(cr, thumb);
                let _ = cr.fill();
            }
        });
        Rc::new(Switch { area, knob })
    }

    pub fn set(&self, is_checked: bool) {
        self.knob.is_checked.set(is_checked);
        self.update();
    }

    pub fn is_checked(&self) -> bool {
        self.knob.is_checked.get()
    }

    pub fn set_scale(&self, scale: f64) {
        self.knob.scale.set(scale);
        self.area.queue_draw();
    }

    pub fn press(&self, is_pressed: bool) {
        self.knob.is_pressed.set(is_pressed);
        self.update();
    }

    pub fn connect_clicked(self: &Rc<Self>, action: impl Fn() + 'static) {
        let click = gtk4::GestureClick::new();
        click.connect_pressed({
            let switch = Rc::downgrade(self);
            move |_, _, _, _| {
                if let Some(switch) = switch.upgrade() {
                    switch.press(true);
                }
            }
        });
        click.connect_released({
            let switch = Rc::downgrade(self);
            move |gesture, _, x, y| {
                let Some(switch) = switch.upgrade() else {
                    return;
                };
                switch.press(false);
                if gesture.widget().is_some_and(|area| area.contains(x, y)) {
                    action();
                }
            }
        });
        click.connect_cancel({
            let switch = Rc::downgrade(self);
            move |_, _| {
                if let Some(switch) = switch.upgrade() {
                    switch.press(false);
                }
            }
        });
        self.area.add_controller(click);
    }

    fn update(&self) {
        let knob = &self.knob;
        let (is_checked, is_pressed) = (knob.is_checked.get(), knob.is_pressed.get());
        let size = if is_pressed {
            28.0
        } else if is_checked {
            24.0
        } else {
            16.0
        } * SCALE;
        let left = match (is_checked, is_pressed) {
            (true, true) => 22.0,
            (true, false) => 24.0,
            (false, true) => 2.0,
            (false, false) => 8.0,
        } * SCALE;
        let should_animate = self.area.is_mapped();
        let now = self
            .area
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        for (cell, target) in [
            (&knob.size, size),
            (&knob.left, left),
            (&knob.tint, if is_checked { 1.0 } else { 0.0 }),
        ] {
            let mut tween = cell.get();
            if should_animate {
                tween.retarget(target, now);
            } else {
                tween.jump(target);
            }
            cell.set(tween);
        }
        self.area.queue_draw();
        if knob.is_ticking.replace(true) {
            return;
        }
        let knob = knob.clone();
        self.area.add_tick_callback(move |area, clock| {
            area.queue_draw();
            let now = clock.frame_time();
            if knob.size.get().is_running(now)
                || knob.left.get().is_running(now)
                || knob.tint.get().is_running(now)
            {
                return glib::ControlFlow::Continue;
            }
            knob.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn blend(from: RGBA, to: RGBA, part: f32) -> RGBA {
    RGBA::new(
        from.red() + (to.red() - from.red()) * part,
        from.green() + (to.green() - from.green()) * part,
        from.blue() + (to.blue() - from.blue()) * part,
        from.alpha() + (to.alpha() - from.alpha()) * part,
    )
}

fn set_source(cr: &gtk4::cairo::Context, color: RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

fn trace_pill(cr: &gtk4::cairo::Context, x: f64, y: f64, width: f64, height: f64) {
    let radius = height / 2.0;
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, PI / 2.0);
    cr.arc(x + radius, y + radius, radius, PI / 2.0, 1.5 * PI);
    cr.close_path();
}

pub struct ConfigSwitch {
    pub button: RippleButton,
    switch: Rc<Switch>,
    binding: RefCell<Option<Box<dyn Fn() -> bool>>>,
}

impl ConfigSwitch {
    pub fn new(
        theme: &SharedTheme,
        icon: &str,
        label: &str,
        on_toggled: impl Fn(bool) + 'static,
    ) -> Rc<Self> {
        let button = RippleButton::new(theme);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        if !icon.is_empty() {
            let symbol = text::symbol(icon, pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnSecondaryContainer");
            row.append(&Centered::integral(&symbol));
        }
        let name_label = text::styled(label);
        text::set_application_font(&name_label, pixel_size::SMALL as f64);
        text::set_color(&name_label, "colOnSecondaryContainer");
        name_label.set_xalign(0.0);
        let name_box = Centered::filling_width(&name_label);
        name_box.set_hexpand(true);
        row.append(&name_box);
        let switch = Switch::new(theme);
        row.append(&switch.area);
        button.set_content(&row, 8, 8);

        let config_switch = Rc::new(ConfigSwitch {
            button: button.clone(),
            switch,
            binding: RefCell::new(None),
        });
        button.connect_down({
            let config_switch = Rc::downgrade(&config_switch);
            move || {
                if let Some(config_switch) = config_switch.upgrade() {
                    config_switch.switch.press(true);
                }
            }
        });
        button.connect_release({
            let config_switch = Rc::downgrade(&config_switch);
            move || {
                if let Some(config_switch) = config_switch.upgrade() {
                    config_switch.switch.press(false);
                }
            }
        });
        button.connect_clicked({
            let config_switch = Rc::downgrade(&config_switch);
            move |_| {
                if let Some(config_switch) = config_switch.upgrade() {
                    let is_checked = !config_switch.switch.knob.is_checked.get();
                    on_toggled(is_checked);
                    config_switch.refresh();
                }
            }
        });
        config_switch
    }

    pub fn set(&self, is_checked: bool) {
        self.switch.set(is_checked);
    }

    pub fn bind(&self, binding: impl Fn() -> bool + 'static) {
        self.binding.replace(Some(Box::new(binding)));
        self.refresh();
    }

    pub fn refresh(&self) {
        if let Some(binding) = self.binding.borrow().as_ref() {
            self.switch.set(binding());
        }
    }

    pub fn set_enabled(&self, is_enabled: bool) {
        self.button.set_sensitive(is_enabled);
    }
}

pub struct ComboBox {
    pub button: RippleButton,
    icon: gtk4::Label,
    label: gtk4::Label,
    popover: gtk4::Popover,
    chevron: Centered,
    inline_list: RefCell<Option<gtk4::Box>>,
    scroll: gtk4::ScrolledWindow,
    list: gtk4::Box,
    theme: SharedTheme,
    items: RefCell<Vec<String>>,
    current: Cell<i32>,
    is_built: Cell<bool>,
    on_activated: RefCell<Option<Rc<dyn Fn(usize)>>>,
}

impl ComboBox {
    pub fn new(theme: &SharedTheme) -> Rc<Self> {
        let button = RippleButton::new(theme);
        button.set_ripple_enabled(false);
        button.set_size_request(-1, 40);
        button.set_radius(20.0);
        button.set_look(Look {
            background: |theme| theme.colors.col_secondary_container,
            hover: |theme| theme.colors.col_secondary_container_hover,
            ripple: |theme| theme.colors.col_secondary_container_active,
            ..Look::default()
        });

        let label = text::styled("");
        text::set_color(&label, "colOnSecondaryContainer");
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_xalign(0.0);
        let label_box = Centered::filling_width(&label);
        label_box.set_hexpand(true);
        let arrow = text::symbol("keyboard_arrow_down", pixel_size::LARGER as f64);
        text::set_color(&arrow, "colOnSecondaryContainer");
        let chevron = Centered::new(&arrow);

        let icon = text::symbol("", pixel_size::LARGER as f64);
        text::set_color(&icon, "colOnSecondaryContainer");
        icon.set_visible(false);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, COMBO_SPACING);
        row.append(&icon);
        row.append(&label_box);
        row.append(&chevron);
        button.set_content(&row, COMBO_PADDING, 0);
        button.set_natural_width_trim(2 * COMBO_PADDING + COMBO_SPACING);

        let list = gtk4::Box::new(gtk4::Orientation::Vertical, COMBO_ITEM_SPACING);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(COMBO_POPUP_LIST);
        scroll.set_child(Some(&list));
        crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);
        let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame.add_css_class("combo-popup");
        frame.append(&scroll);
        frame.set_margin_start(COMBO_POPUP_SHADOW);
        frame.set_margin_end(COMBO_POPUP_SHADOW);
        frame.set_margin_top(COMBO_POPUP_SHADOW);
        frame.set_margin_bottom(COMBO_POPUP_SHADOW);

        let popover = gtk4::Popover::new();
        popover.add_css_class("combo-popover");
        popover.set_has_arrow(false);
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_child(Some(&frame));
        popover.set_parent(&button);

        let combo = Rc::new(ComboBox {
            button: button.clone(),
            icon,
            label,
            popover: popover.clone(),
            chevron: chevron.clone(),
            inline_list: RefCell::new(None),
            scroll,
            list,
            theme: theme.clone(),
            items: RefCell::new(Vec::new()),
            current: Cell::new(-1),
            is_built: Cell::new(false),
            on_activated: RefCell::new(None),
        });
        button.connect_clicked({
            let combo = Rc::downgrade(&combo);
            let popover = popover.clone();
            let chevron = chevron.clone();
            move |button| {
                if let Some(combo) = combo.upgrade() {
                    combo.build_list();
                    let inline_list = combo.inline_list.borrow().clone();
                    if let Some(inline_list) = inline_list {
                        let should_open = !inline_list.is_visible();
                        inline_list.set_visible(should_open);
                        chevron.rotate_to(if should_open { 180.0 } else { 0.0 });
                        return;
                    }
                }
                let (width, height) = (button.width(), button.height());
                let inset = COMBO_POPUP_SHADOW - COMBO_POPUP_GAP;
                popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
                    0,
                    inset,
                    width,
                    height - 2 * inset,
                )));
                popover.set_size_request(width + 2 * COMBO_POPUP_SHADOW, -1);
                popover.popup();
                chevron.rotate_to(180.0);
            }
        });
        popover.connect_closed(move |_| chevron.rotate_to(0.0));
        combo
    }

    pub fn connect_activated(&self, action: impl Fn(usize) + 'static) {
        self.on_activated.replace(Some(Rc::new(action)));
    }

    pub fn use_inline_list(&self) -> gtk4::Box {
        let inline_list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        inline_list.set_visible(false);
        if let Some(frame) = self.scroll.parent().and_downcast::<gtk4::Box>() {
            frame.remove(&self.scroll);
        }
        inline_list.append(&self.scroll);
        self.inline_list.replace(Some(inline_list.clone()));
        inline_list
    }

    fn is_open(&self) -> bool {
        self.popover.is_visible()
            || self
                .inline_list
                .borrow()
                .as_ref()
                .is_some_and(|list| list.is_visible())
    }

    pub fn close(&self) {
        self.popover.popdown();
        if let Some(inline_list) = self.inline_list.borrow().as_ref()
            && inline_list.is_visible()
        {
            inline_list.set_visible(false);
            self.chevron.rotate_to(0.0);
        }
    }

    pub fn item(&self, index: usize) -> Option<String> {
        self.items.borrow().get(index).cloned()
    }

    pub fn set_icon(&self, icon: &str) {
        self.icon.set_text(icon);
        self.icon.set_visible(!icon.is_empty());
    }

    pub fn set_enabled(&self, is_enabled: bool) {
        self.button.set_sensitive(is_enabled);
        self.button.set_opacity(if is_enabled { 1.0 } else { 0.4 });
    }

    pub fn set_items(self: &Rc<Self>, items: &[String], current: i32) -> bool {
        if self.current.get() == current && *self.items.borrow() == items {
            return false;
        }
        self.current.set(current);
        self.label.set_text(
            items
                .get(current.max(0) as usize)
                .map(String::as_str)
                .unwrap_or(""),
        );
        self.items.replace(items.to_vec());
        self.is_built.set(false);
        if self.is_open() {
            self.build_list();
        }
        true
    }

    pub fn set_items_showing(self: &Rc<Self>, items: &[String], current: &str) {
        let index = items.iter().position(|item| item == current).unwrap_or(0);
        self.set_items(items, index as i32);
    }

    fn build_list(self: &Rc<Self>) {
        if self.is_built.replace(true) {
            return;
        }
        let current = self.current.get();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let count = self.items.borrow().len() as i32;
        let height = count * COMBO_ITEM_HEIGHT + (count - 1).max(0) * COMBO_ITEM_SPACING;
        self.scroll.set_policy(
            gtk4::PolicyType::Never,
            if height > COMBO_POPUP_LIST {
                gtk4::PolicyType::Automatic
            } else {
                gtk4::PolicyType::Never
            },
        );
        for (index, item) in self.items.borrow().iter().enumerate() {
            let is_selected = index as i32 == current;
            let entry = RippleButton::new(&self.theme);
            entry.set_ripple_enabled(false);
            entry.set_radius(rounding::SMALL as f64);
            entry.set_size_request(-1, COMBO_ITEM_HEIGHT);
            entry.set_look(if is_selected {
                Look {
                    background: |theme: &Theme| theme.colors.col_secondary_container,
                    hover: |theme: &Theme| theme.colors.col_secondary_container_hover,
                    ..Look::default()
                }
            } else {
                Look {
                    background: |theme: &Theme| transparentize(theme.colors.col_layer3, 1.0),
                    hover: |theme: &Theme| theme.colors.col_layer3_hover,
                    ..Look::default()
                }
            });
            let name_label = text::styled(item);
            text::set_color(
                &name_label,
                if is_selected {
                    "colOnSecondaryContainer"
                } else {
                    "colOnLayer3"
                },
            );
            name_label.set_xalign(0.0);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            entry.set_content(&name_label, 12, 0);
            entry.connect_clicked({
                let combo = Rc::downgrade(self);
                move |_| {
                    let Some(combo) = combo.upgrade() else {
                        return;
                    };
                    combo.close();
                    let action = combo.on_activated.borrow().clone();
                    if let Some(action) = action {
                        action(index);
                    }
                }
            });
            self.list.append(&entry);
        }
    }
}

pub fn icon_button(
    theme: &SharedTheme,
    icon: &str,
    is_filled: bool,
    label: &str,
) -> (RippleButton, gtk4::Label) {
    let button = RippleButton::new(theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_look(Look {
        background: |theme| theme.colors.col_layer2,
        ..Look::default()
    });
    button.set_size_request(-1, ICON_BUTTON_HEIGHT);
    button.set_valign(gtk4::Align::Center);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ICON_BUTTON_SPACING);
    let fill = if is_filled { 1.0 } else { 0.0 };
    let symbol = text::symbol_filled(icon, pixel_size::LARGER as f64, fill);
    text::set_color(&symbol, "colOnSecondaryContainer");
    row.append(&Centered::integral(&symbol));
    let name_label = text::styled(label);
    text::set_color(&name_label, "colOnSecondaryContainer");
    row.append(&Centered::new(&name_label));
    button.set_content(&row, ICON_BUTTON_PADDING, 0);
    (button, name_label)
}

use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::ui::anim::{EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Tween};
use crate::ui::theme::{SharedTheme, Theme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text;

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
    checked: Cell<bool>,
    pressed: Cell<bool>,
    size: Cell<Tween>,
    left: Cell<Tween>,
    tint: Cell<Tween>,
    ticking: Cell<bool>,
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
            checked: Cell::new(false),
            pressed: Cell::new(false),
            size: Cell::new(Tween::new(16.0 * SCALE, SPATIAL_MILLIS, EXPRESSIVE_FAST)),
            left: Cell::new(Tween::new(8.0 * SCALE, SPATIAL_MILLIS, EXPRESSIVE_FAST)),
            tint: Cell::new(Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
            ticking: Cell::new(false),
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
                pill(cr, 0.0, 0.0, width, height);
                pill(cr, inset, inset, width - inset * 2.0, height - inset * 2.0);
                cr.set_fill_rule(gtk4::cairo::FillRule::EvenOdd);
                source(cr, border);
                let _ = cr.fill();
                cr.set_fill_rule(gtk4::cairo::FillRule::Winding);
                pill(cr, inset, inset, width - inset * 2.0, height - inset * 2.0);
                source(cr, track);
                let _ = cr.fill();
                let size = knob.size.get().value(now);
                let left = knob.left.get().value(now);
                cr.new_path();
                cr.arc(left + size / 2.0, height / 2.0, size / 2.0, 0.0, 2.0 * PI);
                source(cr, thumb);
                let _ = cr.fill();
            }
        });
        Rc::new(Switch { area, knob })
    }

    pub fn set(&self, checked: bool) {
        self.knob.checked.set(checked);
        self.update();
    }

    pub fn checked(&self) -> bool {
        self.knob.checked.get()
    }

    pub fn set_scale(&self, scale: f64) {
        self.knob.scale.set(scale);
        self.area.queue_draw();
    }

    pub fn press(&self, pressed: bool) {
        self.knob.pressed.set(pressed);
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
        let (checked, pressed) = (knob.checked.get(), knob.pressed.get());
        let size = if pressed {
            28.0
        } else if checked {
            24.0
        } else {
            16.0
        } * SCALE;
        let left = match (checked, pressed) {
            (true, true) => 22.0,
            (true, false) => 24.0,
            (false, true) => 2.0,
            (false, false) => 8.0,
        } * SCALE;
        let animate = self.area.is_mapped();
        let now = self
            .area
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        for (cell, target) in [
            (&knob.size, size),
            (&knob.left, left),
            (&knob.tint, if checked { 1.0 } else { 0.0 }),
        ] {
            let mut tween = cell.get();
            if animate {
                tween.retarget(target, now);
            } else {
                tween.jump(target);
            }
            cell.set(tween);
        }
        self.area.queue_draw();
        if knob.ticking.replace(true) {
            return;
        }
        let knob = knob.clone();
        self.area.add_tick_callback(move |area, clock| {
            area.queue_draw();
            let now = clock.frame_time();
            if knob.size.get().running(now)
                || knob.left.get().running(now)
                || knob.tint.get().running(now)
            {
                return glib::ControlFlow::Continue;
            }
            knob.ticking.set(false);
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

fn source(cr: &gtk4::cairo::Context, colour: RGBA) {
    cr.set_source_rgba(
        colour.red() as f64,
        colour.green() as f64,
        colour.blue() as f64,
        colour.alpha() as f64,
    );
}

fn pill(cr: &gtk4::cairo::Context, x: f64, y: f64, width: f64, height: f64) {
    let radius = height / 2.0;
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, PI / 2.0);
    cr.arc(x + radius, y + radius, radius, PI / 2.0, 1.5 * PI);
    cr.close_path();
}

pub struct ConfigSwitch {
    pub button: RippleButton,
    switch: Rc<Switch>,
    bound: RefCell<Option<Box<dyn Fn() -> bool>>>,
    faded: Vec<gtk4::Widget>,
}

impl ConfigSwitch {
    pub fn new(
        theme: &SharedTheme,
        icon: &str,
        label: &str,
        toggled: impl Fn(bool) + 'static,
    ) -> Rc<Self> {
        let button = RippleButton::new(theme);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        let mut faded = Vec::new();
        if !icon.is_empty() {
            let symbol = text::symbol(icon, pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnSecondaryContainer");
            let placed = Centred::integral(&symbol);
            row.append(&placed);
            faded.push(placed.upcast());
        }
        let name = text::styled(label);
        text::set_application_font(&name, pixel_size::SMALL as f64);
        text::set_color(&name, "colOnSecondaryContainer");
        name.set_xalign(0.0);
        let label = Centred::filling_width(&name);
        label.set_hexpand(true);
        row.append(&label);
        faded.push(label.upcast());
        let switch = Switch::new(theme);
        row.append(&switch.area);
        button.set_content(&row, 8, 8);

        let made = Rc::new(ConfigSwitch {
            button: button.clone(),
            switch,
            bound: RefCell::new(None),
            faded,
        });
        button.connect_down({
            let made = Rc::downgrade(&made);
            move || {
                if let Some(made) = made.upgrade() {
                    made.switch.press(true);
                }
            }
        });
        button.connect_release({
            let made = Rc::downgrade(&made);
            move || {
                if let Some(made) = made.upgrade() {
                    made.switch.press(false);
                }
            }
        });
        button.connect_clicked({
            let made = Rc::downgrade(&made);
            move |_| {
                if let Some(made) = made.upgrade() {
                    let now = !made.switch.knob.checked.get();
                    toggled(now);
                    made.refresh();
                }
            }
        });
        made
    }

    pub fn set(&self, checked: bool) {
        self.switch.set(checked);
    }

    pub fn bind(&self, bound: impl Fn() -> bool + 'static) {
        self.bound.replace(Some(Box::new(bound)));
        self.refresh();
    }

    pub fn refresh(&self) {
        if let Some(bound) = self.bound.borrow().as_ref() {
            self.switch.set(bound());
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.button.set_sensitive(enabled);
        for widget in &self.faded {
            widget.set_opacity(if enabled { 1.0 } else { 0.4 });
        }
    }
}

pub struct ComboBox {
    pub button: RippleButton,
    icon: gtk4::Label,
    label: gtk4::Label,
    popover: gtk4::Popover,
    scroll: gtk4::ScrolledWindow,
    list: gtk4::Box,
    theme: SharedTheme,
    items: RefCell<Vec<String>>,
    current: Cell<i32>,
    built: Cell<bool>,
    activated: RefCell<Option<Rc<dyn Fn(usize)>>>,
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
        let name = Centred::filling_width(&label);
        name.set_hexpand(true);
        let arrow = text::symbol("keyboard_arrow_down", pixel_size::LARGER as f64);
        text::set_color(&arrow, "colOnSecondaryContainer");
        let chevron = Centred::new(&arrow);

        let icon = text::symbol("", pixel_size::LARGER as f64);
        text::set_color(&icon, "colOnSecondaryContainer");
        icon.set_visible(false);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, COMBO_SPACING);
        row.append(&icon);
        row.append(&name);
        row.append(&chevron);
        button.set_content(&row, COMBO_PADDING, 0);
        button.set_natural_width_trim(2 * COMBO_PADDING + COMBO_SPACING);

        let list = gtk4::Box::new(gtk4::Orientation::Vertical, COMBO_ITEM_SPACING);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(COMBO_POPUP_LIST);
        scroll.set_child(Some(&list));
        crate::ui::widgets::flickable::follow_scroll_settings(&scroll);
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
            scroll,
            list,
            theme: theme.clone(),
            items: RefCell::new(Vec::new()),
            current: Cell::new(-1),
            built: Cell::new(false),
            activated: RefCell::new(None),
        });
        button.connect_clicked({
            let combo = Rc::downgrade(&combo);
            let popover = popover.clone();
            let chevron = chevron.clone();
            move |button| {
                if let Some(combo) = combo.upgrade() {
                    combo.build_list();
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
        self.activated.replace(Some(Rc::new(action)));
    }

    pub fn item(&self, index: usize) -> Option<String> {
        self.items.borrow().get(index).cloned()
    }

    pub fn set_icon(&self, icon: &str) {
        self.icon.set_text(icon);
        self.icon.set_visible(!icon.is_empty());
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.button.set_sensitive(enabled);
        self.button.set_opacity(if enabled { 1.0 } else { 0.4 });
    }

    pub fn set_items(self: &Rc<Self>, items: &[String], current: i32) {
        if self.current.get() == current && *self.items.borrow() == items {
            return;
        }
        self.current.set(current);
        self.label.set_text(
            items
                .get(current.max(0) as usize)
                .map(String::as_str)
                .unwrap_or(""),
        );
        self.items.replace(items.to_vec());
        self.built.set(false);
        if self.popover.is_visible() {
            self.build_list();
        }
    }

    pub fn set_items_showing(self: &Rc<Self>, items: &[String], current: &str) {
        let index = items.iter().position(|item| item == current).unwrap_or(0);
        self.set_items(items, index as i32);
    }

    fn build_list(self: &Rc<Self>) {
        if self.built.replace(true) {
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
            let selected = index as i32 == current;
            let entry = RippleButton::new(&self.theme);
            entry.set_ripple_enabled(false);
            entry.set_radius(rounding::SMALL as f64);
            entry.set_size_request(-1, COMBO_ITEM_HEIGHT);
            entry.set_look(if selected {
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
            let name = text::styled(item);
            text::set_color(
                &name,
                if selected {
                    "colOnSecondaryContainer"
                } else {
                    "colOnLayer3"
                },
            );
            name.set_xalign(0.0);
            name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            entry.set_content(&name, 12, 0);
            entry.connect_clicked({
                let combo = Rc::downgrade(self);
                move |_| {
                    let Some(combo) = combo.upgrade() else {
                        return;
                    };
                    combo.popover.popdown();
                    let action = combo.activated.borrow().clone();
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
    filled: bool,
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
    let fill = if filled { 1.0 } else { 0.0 };
    let symbol = text::symbol_filled(icon, pixel_size::LARGER as f64, fill);
    text::set_color(&symbol, "colOnSecondaryContainer");
    row.append(&Centred::integral(&symbol));
    let name = text::styled(label);
    text::set_color(&name, "colOnSecondaryContainer");
    row.append(&Centred::new(&name));
    button.set_content(&row, ICON_BUTTON_PADDING, 0);
    (button, name)
}

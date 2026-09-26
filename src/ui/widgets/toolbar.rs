use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::anim::{Ease, Tween};
use crate::ui::theme::{SharedTheme, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text;
use crate::ui::widgets::tooltip::{self, Tooltip};

const SPACING: i32 = 4;
const TAB_HEIGHT: i32 = 40;
const TAB_PADDING: i32 = 10;
const TAB_ICON: f64 = 22.0;
const LEADING_MILLIS: f64 = 50.0;
const TRAILING_MILLIS: f64 = 200.0;
const FAB_SIZE: i32 = 48;
const FAB_ICON: f64 = 26.0;

const TAB: Look = Look {
    background: |theme| transparentize(theme.m3.surface_container, 1.0),
    hover: |theme| transparentize(theme.m3.on_surface, 0.95),
    toggled: |theme| transparentize(theme.m3.surface_container, 1.0),
    toggled_hover: |theme| transparentize(theme.m3.on_surface, 1.0),
    ripple: |theme| transparentize(theme.m3.on_surface, 0.95),
    ripple_toggled: |theme| transparentize(theme.m3.on_surface, 0.95),
};

const FAB: Look = Look {
    background: |theme| theme.colors.col_tertiary_container,
    hover: |theme| theme.colors.col_tertiary_container_hover,
    toggled: |theme| theme.colors.col_tertiary_container,
    toggled_hover: |theme| theme.colors.col_tertiary_container_hover,
    ripple: |theme| theme.colors.col_tertiary_container_active,
    ripple_toggled: |theme| theme.colors.col_tertiary_container_active,
};

pub fn frame() -> gtk4::Box {
    let frame = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
    frame.add_css_class("m3-toolbar");
    frame
}

pub fn separator() -> gtk4::Box {
    let line = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    line.add_css_class("m3-toolbar-separator");
    line.set_size_request(1, -1);
    line.set_margin_top(6);
    line.set_margin_bottom(6);
    line.set_margin_start(2);
    line.set_margin_end(2);
    line
}

pub fn button(theme: &SharedTheme, content: &impl IsA<gtk4::Widget>) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_look(TAB);
    button.set_radius(TAB_HEIGHT as f64 / 2.0);
    button.set_content(content, TAB_PADDING, 0);
    button.set_size_request(-1, TAB_HEIGHT);
    button.set_valign(gtk4::Align::Center);
    button.set_cursor_from_name(Some("pointer"));
    button
}

pub fn paired_fab(
    theme: &SharedTheme,
    icon: &str,
    tip: &str,
) -> (RippleButton, gtk4::Label, Rc<Tooltip>) {
    let symbol = text::symbol(icon, FAB_ICON);
    text::set_color(&symbol, "colOnTertiaryContainer");
    let button = RippleButton::new(theme);
    button.add_css_class("m3-toolbar-fab");
    button.set_look(FAB);
    button.set_radius(FAB_SIZE as f64 / 14.0 * 4.0);
    button.set_size_request(FAB_SIZE, FAB_SIZE);
    button.set_valign(gtk4::Align::Center);
    button.set_content(&Centred::integral(&symbol), 0, 0);
    button.set_cursor_from_name(Some("pointer"));
    let tip_widget = Tooltip::new(&button, theme, tooltip::Kind::Styled);
    tip_widget.set_text(tip);
    tooltip::hover_delay(&button, &tip_widget, 0);
    (button, symbol, tip_widget)
}

pub struct TabBar {
    pub widget: gtk4::Overlay,
    row: gtk4::Box,
    indicator: Paint,
    buttons: RefCell<Vec<RippleButton>>,
    current: Cell<usize>,
    bounds: Cell<[Tween; 4]>,
    placed: Cell<bool>,
    ticking: Cell<bool>,
    listener: RefCell<Option<Box<dyn Fn(usize)>>>,
}

impl TabBar {
    pub fn new(theme: &SharedTheme, tabs: &[(&str, &str)]) -> Rc<Self> {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        row.set_halign(gtk4::Align::Center);
        row.set_valign(gtk4::Align::Center);
        let indicator = Paint::new(|_, _, _| {});
        indicator.set_can_target(false);
        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&indicator));
        widget.add_overlay(&row);
        widget.set_measure_overlay(&row, true);
        widget.set_size_request(-1, TAB_HEIGHT);
        widget.set_valign(gtk4::Align::Center);

        let leading = |value: f64| Tween::new(value, LEADING_MILLIS, Ease::OutSine);
        let trailing = |value: f64| Tween::new(value, TRAILING_MILLIS, Ease::OutSine);
        let bar = Rc::new(TabBar {
            widget,
            row: row.clone(),
            indicator: indicator.clone(),
            buttons: RefCell::new(Vec::new()),
            current: Cell::new(0),
            bounds: Cell::new([leading(0.0), trailing(0.0), leading(0.0), trailing(0.0)]),
            placed: Cell::new(false),
            ticking: Cell::new(false),
            listener: RefCell::new(None),
        });

        for (index, (icon, name)) in tabs.iter().enumerate() {
            let symbol = text::symbol(icon, TAB_ICON);
            text::set_color(&symbol, "m3onBackground");
            let button = button(theme, &Centred::integral(&symbol));
            let tip = Tooltip::new(&button, theme, tooltip::Kind::Styled);
            tip.set_text(name);
            tooltip::hover_delay(&button, &tip, 0);
            button.connect_clicked({
                let bar = Rc::downgrade(&bar);
                move |_| {
                    if let Some(bar) = bar.upgrade() {
                        bar.select(index);
                    }
                }
            });
            row.append(&button);
            bar.buttons.borrow_mut().push(button);
        }

        indicator.set_draw({
            let bar = Rc::downgrade(&bar);
            let theme = theme.clone();
            move |snapshot, _, height| {
                let Some(bar) = bar.upgrade() else {
                    return;
                };
                let now = bar.now();
                let [left_lead, left_trail, right_lead, right_trail] = bar.bounds.get();
                let left = left_lead.value(now).min(left_trail.value(now)) as f32;
                let right = right_lead.value(now).max(right_trail.value(now)) as f32;
                let origin = bar
                    .row
                    .compute_bounds(&bar.widget)
                    .map(|bounds| bounds.x())
                    .unwrap_or(0.0);
                let bounds = graphene::Rect::new(origin + left, 0.0, right - left, height);
                let outline = gsk::RoundedRect::from_rect(bounds, height / 2.0);
                snapshot.push_rounded_clip(&outline);
                snapshot.append_color(&theme.borrow().colors.col_secondary_container, &bounds);
                snapshot.pop();
            }
        });

        let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        wheel.connect_scroll({
            let bar = Rc::downgrade(&bar);
            move |_, _, dy| {
                let Some(bar) = bar.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let count = bar.buttons.borrow().len();
                let current = bar.current.get();
                if dy > 0.0 && current + 1 < count {
                    bar.select(current + 1);
                } else if dy < 0.0 && current > 0 {
                    bar.select(current - 1);
                }
                glib::Propagation::Stop
            }
        });
        bar.widget.add_controller(wheel);

        bar.row.connect_realize({
            let bar = Rc::downgrade(&bar);
            move |_| {
                if let Some(bar) = bar.upgrade() {
                    bar.place(false);
                }
            }
        });
        bar
    }

    pub fn connect_selected(&self, action: impl Fn(usize) + 'static) {
        self.listener.replace(Some(Box::new(action)));
    }

    pub fn set_current(self: &Rc<Self>, index: usize) {
        self.current.set(index);
        for (position, button) in self.buttons.borrow().iter().enumerate() {
            button.set_toggled(position == index);
        }
        self.place(self.placed.get());
    }

    fn select(self: &Rc<Self>, index: usize) {
        if index == self.current.get() {
            return;
        }
        self.set_current(index);
        if let Some(listener) = self.listener.borrow().as_ref() {
            listener(index);
        }
    }

    fn now(&self) -> i64 {
        self.indicator
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn place(self: &Rc<Self>, animate: bool) {
        let buttons = self.buttons.borrow();
        let mut left = 0.0;
        for button in buttons.iter().take(self.current.get()) {
            left += (button.measure(gtk4::Orientation::Horizontal, -1).1 + SPACING) as f64;
        }
        let width = buttons
            .get(self.current.get())
            .map(|button| button.measure(gtk4::Orientation::Horizontal, -1).1 as f64)
            .unwrap_or(0.0);
        drop(buttons);
        let now = self.now();
        let mut bounds = self.bounds.get();
        for (tween, target) in bounds
            .iter_mut()
            .zip([left, left, left + width, left + width])
        {
            if animate {
                tween.retarget(target, now);
            } else {
                tween.jump(target);
            }
        }
        self.bounds.set(bounds);
        self.placed.set(true);
        self.indicator.queue_draw();
        if !animate || self.ticking.replace(true) {
            return;
        }
        let bar = Rc::downgrade(self);
        self.indicator.add_tick_callback(move |indicator, clock| {
            let Some(bar) = bar.upgrade() else {
                return glib::ControlFlow::Break;
            };
            indicator.queue_draw();
            let now = clock.frame_time();
            if bar.bounds.get().iter().any(|tween| tween.running(now)) {
                return glib::ControlFlow::Continue;
            }
            bar.ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

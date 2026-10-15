use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::ui::anim::{Ease, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;

const HEIGHT: i32 = 42;
const INSET: f32 = 3.0;
const SPACING: i32 = 1;
const ICON_GAP: i32 = 5;
const INDICATOR: f64 = 3.0;
const INDICATOR_PADDING: f64 = 8.0;
const LEADING_MILLIS: f64 = 100.0;
const TRAILING_MILLIS: f64 = 300.0;

type Changed = Box<dyn Fn(usize)>;

struct Tab {
    button: RippleButton,
    symbol: gtk4::Label,
    name_label: gtk4::Label,
    fill: Rc<dyn Fn(f64)>,
}

pub struct SecondaryTabs {
    pub widget: gtk4::Overlay,
    canvas: gtk4::DrawingArea,
    tabs: Vec<Tab>,
    current: Cell<usize>,
    leading_edge: Cell<Tween>,
    trailing_edge: Cell<Tween>,
    is_ticking: Cell<bool>,
    on_changed: RefCell<Option<Changed>>,
}

impl SecondaryTabs {
    pub fn new(theme: &SharedTheme, tabs: &[(&str, &str)]) -> Rc<Self> {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        row.set_homogeneous(true);
        let mut tab_entries = Vec::with_capacity(tabs.len());
        for (icon, name) in tabs {
            let symbol = text::symbol(icon, pixel_size::HUGE as f64);
            symbol.add_css_class("color-fade");
            let fill = text::fill_motion(&symbol, pixel_size::HUGE as f64, 0.0);
            let label = text::styled(&tr(name));
            label.add_css_class("color-fade");
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            symbol.set_margin_end(ICON_GAP);
            content.append(&Centered::new(&symbol));
            content.append(&Centered::new(&label));

            let button = RippleButton::new(theme);
            button.set_look(Look {
                background: |theme| transparentize(theme.colors.col_surface_container, 1.0),
                hover: |theme| transparentize(theme.colors.col_on_surface, 0.95),
                toggled: |theme| transparentize(theme.colors.col_surface_container, 1.0),
                toggled_hover: |theme| transparentize(theme.colors.col_on_surface, 1.0),
                ripple: |theme| transparentize(theme.colors.col_on_surface, 0.95),
                ripple_toggled: |theme| transparentize(theme.colors.col_on_surface, 0.95),
            });
            button.set_radius(rounding::NORMAL as f64);
            button.set_background_inset(INSET);
            button.set_size_request(-1, HEIGHT);
            button.set_hexpand(true);
            button.set_content(&Centered::integral(&content), 0, 0);
            row.append(&button);
            tab_entries.push(Tab {
                button,
                symbol,
                name_label: label,
                fill,
            });
        }

        let canvas = gtk4::DrawingArea::new();
        canvas.set_content_height(HEIGHT);
        canvas.set_hexpand(true);
        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&canvas));
        widget.add_overlay(&row);

        let bar = Rc::new(SecondaryTabs {
            widget,
            canvas: canvas.clone(),
            tabs: tab_entries,
            current: Cell::new(0),
            leading_edge: Cell::new(Tween::new(0.0, LEADING_MILLIS, Ease::OutSine)),
            trailing_edge: Cell::new(Tween::new(0.0, TRAILING_MILLIS, Ease::OutSine)),
            is_ticking: Cell::new(false),
            on_changed: RefCell::new(None),
        });

        canvas.set_draw_func({
            let bar = Rc::downgrade(&bar);
            let theme = theme.clone();
            move |canvas, cr, width, height| {
                let Some(bar) = bar.upgrade() else {
                    return;
                };
                let now = now(canvas);
                let theme = theme.borrow();
                let (width, height) = (width as f64, height as f64);
                cr.rectangle(0.0, height - 1.0, width, 1.0);
                set_source(cr, theme.colors.col_outline_variant);
                let _ = cr.fill();

                let base = width / bar.tabs.len() as f64;
                let (leading_position, trailing_position) = (
                    bar.leading_edge.get().value(now),
                    bar.trailing_edge.get().value(now),
                );
                let left = leading_position.min(trailing_position) * base + INDICATOR_PADDING;
                let right =
                    (leading_position.max(trailing_position) + 1.0) * base - INDICATOR_PADDING;
                let top = height - INDICATOR;
                cr.new_sub_path();
                cr.arc(left + INDICATOR, top + INDICATOR, INDICATOR, PI, 1.5 * PI);
                cr.arc(
                    right - INDICATOR,
                    top + INDICATOR,
                    INDICATOR,
                    1.5 * PI,
                    2.0 * PI,
                );
                cr.line_to(right, height);
                cr.line_to(left, height);
                cr.close_path();
                set_source(cr, theme.colors.col_primary);
                let _ = cr.fill();
            }
        });

        for (index, tab) in bar.tabs.iter().enumerate() {
            tab.button.connect_down({
                let bar = Rc::downgrade(&bar);
                move || {
                    if let Some(bar) = bar.upgrade() {
                        bar.select(index);
                    }
                }
            });
        }

        let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        wheel.connect_scroll({
            let bar = Rc::downgrade(&bar);
            move |_, _, delta| {
                let Some(bar) = bar.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if delta > 0.0 {
                    bar.step(1);
                } else if delta < 0.0 {
                    bar.step(-1);
                }
                glib::Propagation::Stop
            }
        });
        bar.widget.add_controller(wheel);

        bar.show(0, false);
        bar
    }

    pub fn connect_changed(&self, action: impl Fn(usize) + 'static) {
        self.on_changed.replace(Some(Box::new(action)));
    }

    pub fn current(&self) -> usize {
        self.current.get()
    }

    pub fn step(self: &Rc<Self>, delta: i32) {
        let last = self.tabs.len() as i32 - 1;
        let index = (self.current.get() as i32 + delta).clamp(0, last) as usize;
        self.select(index);
    }

    pub fn select(self: &Rc<Self>, index: usize) {
        if self.current.get() == index {
            return;
        }
        self.show(index, true);
        if let Some(on_changed) = self.on_changed.borrow().as_ref() {
            on_changed(index);
        }
    }

    pub fn show(self: &Rc<Self>, index: usize, should_animate: bool) {
        self.current.set(index);
        let should_animate = should_animate && self.widget.is_mapped();
        let now = now(&self.canvas);
        for cell in [&self.leading_edge, &self.trailing_edge] {
            let mut tween = cell.get();
            if should_animate {
                tween.retarget(index as f64, now);
            } else {
                tween.jump(index as f64);
            }
            cell.set(tween);
        }
        for (position, tab) in self.tabs.iter().enumerate() {
            let is_checked = position == index;
            tab.button.set_toggled(is_checked);
            let color = if is_checked {
                "colPrimary"
            } else {
                "colOnLayer1"
            };
            text::set_color(&tab.symbol, color);
            text::set_color(&tab.name_label, color);
            (tab.fill)(if is_checked { 1.0 } else { 0.0 });
        }
        self.canvas.queue_draw();
        if !should_animate || self.is_ticking.replace(true) {
            return;
        }
        let bar = Rc::downgrade(self);
        self.canvas.add_tick_callback(move |canvas, clock| {
            let Some(bar) = bar.upgrade() else {
                return glib::ControlFlow::Break;
            };
            canvas.queue_draw();
            let now = clock.frame_time();
            if bar.leading_edge.get().is_running(now) || bar.trailing_edge.get().is_running(now) {
                return glib::ControlFlow::Continue;
            }
            bar.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn set_source(cr: &gtk4::cairo::Context, color: RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

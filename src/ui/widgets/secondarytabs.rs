use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::ui::anim::{Ease, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text;

const HEIGHT: i32 = 42;
const INSET: f32 = 3.0;
const SPACING: i32 = 1;
const ICON_GAP: i32 = 5;
const INDICATOR: f64 = 3.0;
const INDICATOR_PADDING: f64 = 8.0;
const LEADING_MILLIS: f64 = 100.0;
const FOLLOWING_MILLIS: f64 = 300.0;

type Changed = Box<dyn Fn(usize)>;

struct Tab {
    button: RippleButton,
    symbol: gtk4::Label,
    name: gtk4::Label,
    fill: Rc<dyn Fn(f64)>,
}

pub struct SecondaryTabs {
    pub widget: gtk4::Overlay,
    canvas: gtk4::DrawingArea,
    tabs: Vec<Tab>,
    current: Cell<usize>,
    leading: Cell<Tween>,
    following: Cell<Tween>,
    ticking: Cell<bool>,
    changed: RefCell<Option<Changed>>,
}

impl SecondaryTabs {
    pub fn new(theme: &SharedTheme, tabs: &[(&str, &str)]) -> Rc<Self> {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        row.set_homogeneous(true);
        let mut made = Vec::with_capacity(tabs.len());
        for (icon, name) in tabs {
            let symbol = text::symbol(icon, pixel_size::HUGE as f64);
            symbol.add_css_class("color-fade");
            let fill = text::fill_motion(&symbol, pixel_size::HUGE as f64, 0.0);
            let label = text::styled(&tr(name));
            label.add_css_class("color-fade");
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            symbol.set_margin_end(ICON_GAP);
            content.append(&Centred::new(&symbol));
            content.append(&Centred::new(&label));

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
            button.set_content(&Centred::integral(&content), 0, 0);
            row.append(&button);
            made.push(Tab {
                button,
                symbol,
                name: label,
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
            tabs: made,
            current: Cell::new(0),
            leading: Cell::new(Tween::new(0.0, LEADING_MILLIS, Ease::OutSine)),
            following: Cell::new(Tween::new(0.0, FOLLOWING_MILLIS, Ease::OutSine)),
            ticking: Cell::new(false),
            changed: RefCell::new(None),
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
                source(cr, theme.colors.col_outline_variant);
                let _ = cr.fill();

                let base = width / bar.tabs.len() as f64;
                let (one, two) = (bar.leading.get().value(now), bar.following.get().value(now));
                let left = one.min(two) * base + INDICATOR_PADDING;
                let right = (one.max(two) + 1.0) * base - INDICATOR_PADDING;
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
                source(cr, theme.colors.col_primary);
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
        self.changed.replace(Some(Box::new(action)));
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
        if let Some(changed) = self.changed.borrow().as_ref() {
            changed(index);
        }
    }

    pub fn show(self: &Rc<Self>, index: usize, animate: bool) {
        self.current.set(index);
        let animate = animate && self.widget.is_mapped();
        let now = now(&self.canvas);
        for cell in [&self.leading, &self.following] {
            let mut tween = cell.get();
            if animate {
                tween.retarget(index as f64, now);
            } else {
                tween.jump(index as f64);
            }
            cell.set(tween);
        }
        for (position, tab) in self.tabs.iter().enumerate() {
            let checked = position == index;
            tab.button.set_toggled(checked);
            let colour = if checked { "colPrimary" } else { "colOnLayer1" };
            text::set_color(&tab.symbol, colour);
            text::set_color(&tab.name, colour);
            (tab.fill)(if checked { 1.0 } else { 0.0 });
        }
        self.canvas.queue_draw();
        if !animate || self.ticking.replace(true) {
            return;
        }
        let bar = Rc::downgrade(self);
        self.canvas.add_tick_callback(move |canvas, clock| {
            let Some(bar) = bar.upgrade() else {
                return glib::ControlFlow::Break;
            };
            canvas.queue_draw();
            let now = clock.frame_time();
            if bar.leading.get().running(now) || bar.following.get().running(now) {
                return glib::ControlFlow::Continue;
            }
            bar.ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn source(cr: &gtk4::cairo::Context, colour: RGBA) {
    cr.set_source_rgba(
        colour.red() as f64,
        colour.green() as f64,
        colour.blue() as f64,
        colour.alpha() as f64,
    );
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

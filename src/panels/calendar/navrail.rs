use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::panels::calendar::pinned::Pinned;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Fade, Tween};
use crate::ui::theme::{SharedTheme, Theme, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::group::pointer_cursor;
use crate::ui::widgets::text;

const BASE: f64 = 56.0;
const HIGHLIGHT: f64 = 32.0;
const ICON: f64 = 24.0;
const LABEL: i32 = 14;
const LABEL_GAP: f64 = 2.0;
const SPATIAL_MILLIS: f64 = 350.0;
const FAST_MILLIS: f64 = 200.0;

type Pressed = Box<dyn Fn(usize)>;

struct Item {
    symbol: gtk4::Label,
    fill: Cell<Tween>,
    background: Fade,
}

struct State {
    theme: SharedTheme,
    items: Vec<Item>,
    current: Cell<usize>,
    hovered: Cell<Option<usize>>,
    down: Cell<Option<usize>>,
    highlight: Cell<Tween>,
    ticking: Cell<bool>,
    pressed: RefCell<Option<Pressed>>,
}

pub struct NavRail {
    pub widget: Pinned,
    canvas: gtk4::DrawingArea,
    state: Rc<State>,
}

impl NavRail {
    pub fn new(theme: &SharedTheme, tabs: &[(&str, &str)], current: usize) -> Rc<Self> {
        let height = BASE * tabs.len() as f64;
        let widget = Pinned::new(BASE as i32, height as i32);
        let canvas = gtk4::DrawingArea::new();
        canvas.set_content_width(BASE as i32);
        canvas.set_content_height(height as i32);
        widget.put(&canvas, 0.0, 0.0);

        let top = (BASE - HIGHLIGHT) / 2.0;
        let mut items = Vec::with_capacity(tabs.len());
        for (index, (icon, name)) in tabs.iter().enumerate() {
            let y = BASE * index as f64;
            let symbol = text::symbol(icon, ICON);
            symbol.add_css_class("color-fade");
            let slot = Centred::integral(&symbol);
            slot.set_size_request(BASE as i32, HIGHLIGHT as i32);
            widget.put(&slot, 0.0, y + top);

            let label = text::styled_sized(name, LABEL);
            text::set_color(&label, "colOnLayer1");
            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            label.set_max_width_chars(1);
            label.set_size_request(BASE as i32, -1);
            label.set_xalign(0.5);
            widget.put(&label, 0.0, y + top + HIGHLIGHT + LABEL_GAP);

            items.push(Item {
                symbol,
                fill: Cell::new(Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
                background: Fade::new(),
            });
        }

        let state = Rc::new(State {
            theme: theme.clone(),
            items,
            current: Cell::new(current),
            hovered: Cell::new(None),
            down: Cell::new(None),
            highlight: Cell::new(Tween::new(
                BASE * current as f64 + top,
                SPATIAL_MILLIS,
                EXPRESSIVE_FAST,
            )),
            ticking: Cell::new(false),
            pressed: RefCell::new(None),
        });

        canvas.set_draw_func({
            let state = state.clone();
            move |canvas, cr, _, _| {
                let now = now(canvas);
                let theme = state.theme.borrow();
                pill(cr, state.highlight.get().value(now));
                source(cr, theme.colors.col_secondary_container);
                let _ = cr.fill();
                for (index, item) in state.items.iter().enumerate() {
                    let Some(colour) = item.background.value(now) else {
                        continue;
                    };
                    pill(cr, BASE * index as f64 + top);
                    source(cr, colour);
                    let _ = cr.fill();
                }
            }
        });

        let rail = Rc::new(NavRail {
            widget,
            canvas,
            state,
        });
        pointer_cursor(rail.widget.upcast_ref());

        let index_at = {
            let count = tabs.len();
            move |y: f64| {
                let index = (y / BASE).floor();
                (index >= 0.0 && (index as usize) < count).then_some(index as usize)
            }
        };
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion({
            let rail = Rc::downgrade(&rail);
            move |_, _, y| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.hovered.set(index_at(y));
                    rail.refresh(true);
                }
            }
        });
        motion.connect_leave({
            let rail = Rc::downgrade(&rail);
            move |_| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.hovered.set(None);
                    rail.refresh(true);
                }
            }
        });
        rail.widget.add_controller(motion);

        let click = gtk4::GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.connect_pressed({
            let rail = Rc::downgrade(&rail);
            move |gesture, _, _, y| {
                let Some(rail) = rail.upgrade() else {
                    return;
                };
                let Some(index) = index_at(y) else {
                    return;
                };
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                rail.state.down.set(Some(index));
                rail.set_current(index);
                if let Some(pressed) = rail.state.pressed.borrow().as_ref() {
                    pressed(index);
                }
            }
        });
        click.connect_released({
            let rail = Rc::downgrade(&rail);
            move |_, _, _, _| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.down.set(None);
                    rail.refresh(true);
                }
            }
        });
        click.connect_cancel({
            let rail = Rc::downgrade(&rail);
            move |_, _| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.down.set(None);
                    rail.refresh(true);
                }
            }
        });
        rail.widget.add_controller(click);

        rail.refresh(false);
        rail
    }

    pub fn connect_pressed(&self, action: impl Fn(usize) + 'static) {
        self.state.pressed.replace(Some(Box::new(action)));
    }

    pub fn set_current(&self, index: usize) {
        self.state.current.set(index);
        self.refresh(true);
    }

    fn refresh(&self, animate: bool) {
        let state = &self.state;
        let animate = animate && self.widget.is_mapped();
        let now = now(&self.canvas);
        let theme = state.theme.borrow();
        let current = state.current.get();
        let top = BASE * current as f64 + (BASE - HIGHLIGHT) / 2.0;
        let mut highlight = state.highlight.get();
        if animate {
            highlight.retarget(top, now);
        } else {
            highlight.jump(top);
        }
        state.highlight.set(highlight);
        for (index, item) in state.items.iter().enumerate() {
            let toggled = index == current;
            let hovered = state.hovered.get() == Some(index);
            item.background.retarget(
                background(&theme, toggled, hovered, state.down.get() == Some(index)),
                now,
                animate,
            );
            text::set_color(
                &item.symbol,
                if toggled {
                    "m3onSecondaryContainer"
                } else {
                    "colOnLayer1"
                },
            );
            let mut fill = item.fill.get();
            let target = if toggled { 1.0 } else { 0.0 };
            if animate {
                fill.retarget(target, now);
            } else {
                fill.jump(target);
            }
            item.fill.set(fill);
        }
        drop(theme);
        paint_state(state, now);
        self.canvas.queue_draw();
        if !animate || state.ticking.replace(true) {
            return;
        }
        let rail = Rc::downgrade(&self.state);
        let canvas = self.canvas.clone();
        self.canvas.add_tick_callback(move |_, clock| {
            let Some(state) = rail.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            paint_state(&state, now);
            canvas.queue_draw();
            let running = state.highlight.get().running(now)
                || state
                    .items
                    .iter()
                    .any(|item| item.fill.get().running(now) || item.background.running(now));
            if running {
                return glib::ControlFlow::Continue;
            }
            state.ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn paint_state(state: &State, now: i64) {
    for item in &state.items {
        text::set_symbol_font_weighted(&item.symbol, ICON, item.fill.get().value(now), 400.0);
    }
}

fn background(theme: &Theme, toggled: bool, hovered: bool, down: bool) -> RGBA {
    if toggled {
        return transparentize(theme.colors.col_secondary_container, 1.0);
    }
    if down {
        return theme.colors.col_layer1_active;
    }
    if hovered {
        return theme.colors.col_layer1_hover;
    }
    transparentize(theme.colors.col_layer1_hover, 1.0)
}

fn pill(cr: &gtk4::cairo::Context, y: f64) {
    let radius = HIGHLIGHT / 2.0;
    cr.new_sub_path();
    cr.arc(BASE - radius, y + radius, radius, -PI / 2.0, PI / 2.0);
    cr.arc(radius, y + radius, radius, PI / 2.0, 1.5 * PI);
    cr.close_path();
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

use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::calendar::pinned::Pinned;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Fade, Tween};
use crate::ui::theme::{SharedTheme, Theme, transparentize};
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::group::pointer_cursor;

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
    current_index: Cell<usize>,
    hovered_index: Cell<Option<usize>>,
    pressed_index: Cell<Option<usize>>,
    highlight: Cell<Tween>,
    is_ticking: Cell<bool>,
    on_pressed: RefCell<Option<Pressed>>,
}

pub struct NavRail {
    pub widget: Pinned,
    canvas: gtk4::DrawingArea,
    state: Rc<State>,
}

impl NavRail {
    pub fn new(theme: &SharedTheme, tabs: &[(&str, &str)], current_index: usize) -> Rc<Self> {
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
            let slot = Centered::integral(&symbol);
            slot.set_size_request(BASE as i32, HIGHLIGHT as i32);
            widget.put(&slot, 0.0, y + top);

            let label = text::styled_sized(&tr(name), LABEL);
            text::set_color(&label, "colOnLayer1");
            label.set_xalign(0.5);
            let width = label.measure(gtk4::Orientation::Horizontal, -1).1 as f64;
            widget.put(
                &label,
                (BASE - width) / 2.0,
                y + top + HIGHLIGHT + LABEL_GAP,
            );

            items.push(Item {
                symbol,
                fill: Cell::new(Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
                background: Fade::new(),
            });
        }

        let state = Rc::new(State {
            theme: theme.clone(),
            items,
            current_index: Cell::new(current_index),
            hovered_index: Cell::new(None),
            pressed_index: Cell::new(None),
            highlight: Cell::new(Tween::new(
                BASE * current_index as f64 + top,
                SPATIAL_MILLIS,
                EXPRESSIVE_FAST,
            )),
            is_ticking: Cell::new(false),
            on_pressed: RefCell::new(None),
        });

        canvas.set_draw_func({
            let state = state.clone();
            move |canvas, cr, _, _| {
                let now = now(canvas);
                let theme = state.theme.borrow();
                trace_pill(cr, state.highlight.get().value(now));
                set_source(cr, theme.colors.col_secondary_container);
                let _ = cr.fill();
                for (index, item) in state.items.iter().enumerate() {
                    let Some(color) = item.background.value(now) else {
                        continue;
                    };
                    trace_pill(cr, BASE * index as f64 + top);
                    set_source(cr, color);
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
        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_motion({
            let rail = Rc::downgrade(&rail);
            move |_, _, y| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.hovered_index.set(index_at(y));
                    rail.refresh(true);
                }
            }
        });
        motion_controller.connect_leave({
            let rail = Rc::downgrade(&rail);
            move |_| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.hovered_index.set(None);
                    rail.refresh(true);
                }
            }
        });
        rail.widget.add_controller(motion_controller);

        let click_gesture = gtk4::GestureClick::new();
        click_gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click_gesture.connect_pressed({
            let rail = Rc::downgrade(&rail);
            move |gesture, _, _, y| {
                let Some(rail) = rail.upgrade() else {
                    return;
                };
                let Some(index) = index_at(y) else {
                    return;
                };
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                rail.state.pressed_index.set(Some(index));
                rail.set_current(index);
                if let Some(on_pressed) = rail.state.on_pressed.borrow().as_ref() {
                    on_pressed(index);
                }
            }
        });
        click_gesture.connect_released({
            let rail = Rc::downgrade(&rail);
            move |_, _, _, _| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.pressed_index.set(None);
                    rail.refresh(true);
                }
            }
        });
        click_gesture.connect_cancel({
            let rail = Rc::downgrade(&rail);
            move |_, _| {
                if let Some(rail) = rail.upgrade() {
                    rail.state.pressed_index.set(None);
                    rail.refresh(true);
                }
            }
        });
        rail.widget.add_controller(click_gesture);

        rail.refresh(false);
        rail
    }

    pub fn connect_pressed(&self, action: impl Fn(usize) + 'static) {
        self.state.on_pressed.replace(Some(Box::new(action)));
    }

    pub fn set_current(&self, index: usize) {
        self.state.current_index.set(index);
        self.refresh(true);
    }

    fn refresh(&self, should_animate: bool) {
        let state = &self.state;
        let should_animate = should_animate && self.widget.is_mapped();
        let now = now(&self.canvas);
        let theme = state.theme.borrow();
        let current_index = state.current_index.get();
        let top = BASE * current_index as f64 + (BASE - HIGHLIGHT) / 2.0;
        let mut highlight = state.highlight.get();
        if should_animate {
            highlight.retarget(top, now);
        } else {
            highlight.jump(top);
        }
        state.highlight.set(highlight);
        for (index, item) in state.items.iter().enumerate() {
            let is_toggled = index == current_index;
            let is_hovered = state.hovered_index.get() == Some(index);
            item.background.retarget(
                background(
                    &theme,
                    is_toggled,
                    is_hovered,
                    state.pressed_index.get() == Some(index),
                ),
                now,
                should_animate,
            );
            text::set_color(
                &item.symbol,
                if is_toggled {
                    "m3onSecondaryContainer"
                } else {
                    "colOnLayer1"
                },
            );
            let mut fill = item.fill.get();
            let target = if is_toggled { 1.0 } else { 0.0 };
            if should_animate {
                fill.retarget(target, now);
            } else {
                fill.jump(target);
            }
            item.fill.set(fill);
        }
        drop(theme);
        paint_state(state, now);
        self.canvas.queue_draw();
        if !should_animate || state.is_ticking.replace(true) {
            return;
        }
        let state = Rc::downgrade(&self.state);
        let canvas = self.canvas.clone();
        self.canvas.add_tick_callback(move |_, clock| {
            let Some(state) = state.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            paint_state(&state, now);
            canvas.queue_draw();
            let is_running = state.highlight.get().is_running(now)
                || state
                    .items
                    .iter()
                    .any(|item| item.fill.get().is_running(now) || item.background.is_running(now));
            if is_running {
                return glib::ControlFlow::Continue;
            }
            state.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn paint_state(state: &State, now: i64) {
    for item in &state.items {
        text::set_symbol_font_weighted(&item.symbol, ICON, item.fill.get().value(now), 400.0);
    }
}

fn background(theme: &Theme, is_toggled: bool, is_hovered: bool, is_pressed: bool) -> RGBA {
    if is_toggled {
        return transparentize(theme.colors.col_secondary_container, 1.0);
    }
    if is_pressed {
        return theme.colors.col_layer1_active;
    }
    if is_hovered {
        return theme.colors.col_layer1_hover;
    }
    transparentize(theme.colors.col_layer1_hover, 1.0)
}

fn trace_pill(cr: &gtk4::cairo::Context, y: f64) {
    let radius = HIGHLIGHT / 2.0;
    cr.new_sub_path();
    cr.arc(BASE - radius, y + radius, radius, -PI / 2.0, PI / 2.0);
    cr.arc(radius, y + radius, radius, PI / 2.0, 1.5 * PI);
    cr.close_path();
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

use gtk4::cairo;
use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::ui::anim::{EXPRESSIVE_EFFECTS, Fade, Tween};
use crate::ui::theme::{SharedTheme, Theme, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text::{self, Family};
use crate::ui::widgets::tooltip::{self, Tooltip};

const PADDING: f64 = 6.0;
const TRACK: f64 = 30.0;
const TRACK_RADIUS: f64 = 9.0;
const HANDLE_WIDTH: f64 = 3.0;
const HANDLE_PRESSED: f64 = 1.5;
const HANDLE_MARGIN: f64 = 4.0;
const DIVIDER_MARGIN: f64 = 2.0;
const DOT: f64 = 3.0;
const ICON: f64 = 20.0;
const ICON_MARGIN: f64 = 8.0;
const ICON_ON_HANDLE: f64 = 14.0;
const NEAR_FULL: f64 = 0.9;
const FAST: f64 = 200.0;
const WAVY_HANDLE: f64 = 24.0;
const WAVE_AMPLITUDE: f64 = 0.5;
const WAVE_FREQUENCY: f64 = 6.0;
const WAVE_PERIOD_MILLIS: f64 = 400.0;

type Handler = Box<dyn Fn(f64)>;

#[derive(Clone, Copy)]
pub struct Colours {
    pub highlight: RGBA,
    pub track: RGBA,
    pub handle: RGBA,
}

struct State {
    theme: SharedTheme,
    value: Cell<Tween>,
    pressed: Cell<bool>,
    icon: Option<&'static str>,
    track: f64,
    radius: f64,
    handle_height: f64,
    from: f64,
    to: f64,
    secondary: Option<(&'static str, f64)>,
    dividers: Vec<f64>,
    stops: RefCell<Vec<f64>>,
    handle: Cell<Tween>,
    margin: Cell<Tween>,
    near_full: Cell<bool>,
    icon_fade: Fade,
    secondary_fade: Fade,
    ticking: Cell<bool>,
    tip_text: RefCell<Option<String>>,
    moved: RefCell<Option<Handler>>,
    released: RefCell<Option<Handler>>,
    wavy: bool,
    colours: Cell<Option<Colours>>,
}

pub struct Slider {
    pub area: gtk4::Widget,
    canvas: gtk4::DrawingArea,
    state: Rc<State>,
    tooltip: Rc<Tooltip>,
}

pub struct Options {
    pub track: f64,
    pub from: f64,
    pub to: f64,
    pub icon: Option<&'static str>,
    pub secondary: Option<(&'static str, f64)>,
    pub dividers: Vec<f64>,
}

pub const SMALL: f64 = 18.0;
pub const WAVY: f64 = 4.0;

impl Slider {
    pub fn new(
        theme: &SharedTheme,
        icon: &'static str,
        secondary: Option<(&'static str, f64)>,
        dividers: Vec<f64>,
    ) -> Rc<Self> {
        Self::with(
            theme,
            Options {
                track: TRACK,
                from: 0.0,
                to: 1.0,
                icon: Some(icon),
                secondary,
                dividers,
            },
        )
    }

    pub fn with(theme: &SharedTheme, options: Options) -> Rc<Self> {
        let Options {
            track,
            from,
            to,
            icon,
            secondary,
            dividers,
        } = options;
        let radius = if track >= 72.0 {
            21.0
        } else if track >= 42.0 {
            12.0
        } else if track >= 30.0 {
            TRACK_RADIUS
        } else if track >= 18.0 {
            6.0
        } else {
            track / 2.0
        };
        let wavy = track == WAVY;
        let handle_height = if wavy {
            WAVY_HANDLE
        } else {
            (track + 9.0).max(33.0)
        };
        let canvas = gtk4::DrawingArea::new();
        canvas.set_content_height((handle_height + PADDING * 2.0) as i32);
        canvas.set_hexpand(true);
        canvas.set_cursor_from_name(Some("pointer"));

        let state = Rc::new(State {
            theme: theme.clone(),
            value: Cell::new(Tween::new(0.0, FAST, EXPRESSIVE_EFFECTS)),
            pressed: Cell::new(false),
            icon,
            track,
            radius,
            handle_height,
            from,
            to,
            secondary,
            dividers,
            stops: RefCell::new(Vec::new()),
            handle: Cell::new(Tween::new(HANDLE_WIDTH, FAST, EXPRESSIVE_EFFECTS)),
            margin: Cell::new(Tween::new(ICON_MARGIN, FAST, EXPRESSIVE_EFFECTS)),
            near_full: Cell::new(false),
            icon_fade: Fade::new(),
            secondary_fade: Fade::new(),
            ticking: Cell::new(false),
            tip_text: RefCell::new(None),
            moved: RefCell::new(None),
            released: RefCell::new(None),
            wavy,
            colours: Cell::new(None),
        });
        if wavy {
            canvas.add_tick_callback(|canvas, _| {
                canvas.queue_draw();
                glib::ControlFlow::Continue
            });
        }

        let holder = Centred::filling_width(&canvas);
        holder.set_hexpand(true);
        let tooltip = Tooltip::new(&holder, theme, tooltip::Kind::Styled);
        tooltip.place_like_qt();

        let slider = Rc::new(Slider {
            area: holder.upcast(),
            canvas: canvas.clone(),
            state: state.clone(),
            tooltip,
        });

        canvas.set_draw_func({
            let state = state.clone();
            move |canvas, cr, width, height| {
                let now = canvas
                    .frame_clock()
                    .map(|clock| clock.frame_time())
                    .unwrap_or_else(glib::monotonic_time);
                draw(cr, canvas, width as f64, height as f64, &state, now);
            }
        });

        let drag = gtk4::GestureDrag::new();
        drag.connect_drag_begin({
            let slider = Rc::downgrade(&slider);
            move |_, x, _| {
                let Some(slider) = slider.upgrade() else {
                    return;
                };
                slider.press(true);
                slider.slide(x);
            }
        });
        drag.connect_drag_update({
            let slider = Rc::downgrade(&slider);
            move |gesture, offset, _| {
                let Some(slider) = slider.upgrade() else {
                    return;
                };
                if let Some((start, _)) = gesture.start_point() {
                    slider.slide(start + offset);
                }
            }
        });
        drag.connect_drag_end({
            let slider = Rc::downgrade(&slider);
            move |_, _, _| {
                if let Some(slider) = slider.upgrade() {
                    slider.press(false);
                }
            }
        });
        canvas.add_controller(drag);

        slider
    }

    pub fn set(&self, value: f64) {
        let state = &self.state;
        if state.pressed.get() {
            return;
        }
        let position = ((value - state.from) / (state.to - state.from)).clamp(0.0, 1.0);
        let mut shown = state.value.get();
        if self.canvas.is_mapped() {
            shown.retarget(position, self.now());
        } else {
            shown.jump(position);
        }
        state.value.set(shown);
        self.refresh();
    }

    pub fn set_stops(&self, values: Vec<f64>) {
        self.state.stops.replace(values);
        self.canvas.queue_draw();
    }

    pub fn set_tooltip(&self, text: &str) {
        self.state.tip_text.replace(Some(text.to_owned()));
        self.tooltip.set_text(text);
    }

    pub fn on_moved(&self, handler: impl Fn(f64) + 'static) {
        self.state.moved.replace(Some(Box::new(handler)));
    }

    pub fn on_released(&self, handler: impl Fn(f64) + 'static) {
        self.state.released.replace(Some(Box::new(handler)));
    }

    pub fn set_colours(&self, colours: Colours) {
        self.state.colours.set(Some(colours));
        self.canvas.queue_draw();
    }

    fn press(&self, pressed: bool) {
        let state = &self.state;
        state.pressed.set(pressed);
        self.canvas
            .set_cursor_from_name(Some(if pressed { "grabbing" } else { "pointer" }));
        let mut handle = state.handle.get();
        handle.retarget(
            if pressed {
                HANDLE_PRESSED
            } else {
                HANDLE_WIDTH
            },
            self.now(),
        );
        state.handle.set(handle);
        self.refresh();
        if pressed {
            self.place_tooltip();
            self.tooltip.show(true);
            return;
        }
        self.tooltip.show(false);
        let value = state.from + state.value.get().target() * (state.to - state.from);
        if let Some(handler) = state.released.borrow().as_ref() {
            handler(value);
        }
    }

    fn slide(&self, x: f64) {
        let reach = self.canvas.width() as f64 - HANDLE_MARGIN * 2.0;
        let handle = self.state.handle.get().value(self.now());
        if reach - handle <= 0.0 {
            return;
        }
        let position = ((x - HANDLE_MARGIN - handle / 2.0) / (reach - handle)).clamp(0.0, 1.0);
        let state = &self.state;
        let mut shown = state.value.get();
        shown.jump(position);
        state.value.set(shown);
        self.refresh();
        let value = state.from + position * (state.to - state.from);
        if let Some(handler) = state.moved.borrow().as_ref() {
            handler(value);
        }
        self.place_tooltip();
    }

    fn place_tooltip(&self) {
        let reach = self.canvas.width() as f64 - HANDLE_MARGIN * 2.0;
        let handle = self.state.handle.get().value(self.now());
        let at = self.state.value.get().target();
        let left = HANDLE_MARGIN + at * reach - handle / 2.0;
        let custom = self.state.tip_text.borrow().is_some();
        if !custom {
            self.tooltip.set_text(&format!("{}%", (at * 100.0).round()));
        }
        self.tooltip.point_at(
            left.round() as i32,
            PADDING as i32,
            handle.round().max(1.0) as i32,
            self.state.handle_height as i32,
        );
    }

    fn now(&self) -> i64 {
        self.canvas
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn refresh(&self) {
        let state = &self.state;
        let now = self.now();
        let animate = self.canvas.is_mapped();
        let at = state.value.get().target();
        let near = at >= NEAR_FULL;
        if state.near_full.replace(near) != near {
            let mut margin = state.margin.get();
            let target = if near { ICON_ON_HANDLE } else { ICON_MARGIN };
            if animate {
                margin.jump(if near { ICON_MARGIN } else { ICON_ON_HANDLE });
                margin.retarget(target, now);
            } else {
                margin.jump(target);
            }
            state.margin.set(margin);
        }
        {
            let theme = state.theme.borrow();
            state
                .icon_fade
                .retarget(icon_colour(&theme, near), now, animate);
            if let Some((_, place)) = state.secondary {
                state
                    .secondary_fade
                    .retarget(icon_colour(&theme, at >= place - 0.1), now, animate);
            }
        }
        self.canvas.queue_draw();
        if state.ticking.replace(true) {
            return;
        }
        let state = self.state.clone();
        self.canvas.add_tick_callback(move |canvas, clock| {
            canvas.queue_draw();
            let now = clock.frame_time();
            let busy = state.value.get().running(now)
                || state.handle.get().running(now)
                || state.margin.get().running(now)
                || state.icon_fade.running(now)
                || state.secondary_fade.running(now);
            if busy {
                return glib::ControlFlow::Continue;
            }
            state.ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn centre(outer: f64, inner: f64) -> f64 {
    let half = |size: f64| (size / 2.0 + 0.5).floor();
    half(outer) - half(inner)
}

fn draw(
    cr: &cairo::Context,
    canvas: &gtk4::DrawingArea,
    width: f64,
    height: f64,
    state: &State,
    now: i64,
) {
    let theme = state.theme.borrow();
    let colours = state.colours.get().unwrap_or(Colours {
        highlight: theme.colors.col_primary,
        track: theme.colors.col_secondary_container,
        handle: theme.colors.col_primary,
    });
    let reach = width - HANDLE_MARGIN * 2.0;
    let handle = state.handle.get().value(now);
    let at = state.value.get().value(now);
    let track = state.track;
    let track_radius = state.radius;
    let top = centre(height, track);

    let clear: Vec<f64> = state
        .dividers
        .iter()
        .copied()
        .filter(|value| (value - at).abs() * reach > HANDLE_MARGIN + handle / 2.0 - DIVIDER_MARGIN)
        .collect();

    let mut left = vec![0.0];
    left.extend(clear.iter().copied().filter(|value| *value < at));
    left.push(at);

    let mut right = vec![at];
    right.extend(clear.iter().copied().filter(|value| *value > at));
    right.push(1.0);

    let unsharpen = rounding::UNSHARPEN as f64;
    let last = left.len() - 2;
    for index in 0..=last {
        let start = if index > 0 { DIVIDER_MARGIN } else { 0.0 };
        let end = if index < last {
            DIVIDER_MARGIN
        } else {
            HANDLE_MARGIN
        };
        let x = left[index] * reach + start + if index > 0 { HANDLE_MARGIN } else { 0.0 };
        let span = (left[index + 1] - left[index]) * reach
            - start
            - end
            - if index == last { handle / 2.0 } else { 0.0 }
            + if index == 0 { HANDLE_MARGIN } else { 0.0 };
        if state.wavy {
            wave(cr, colours.highlight, x, span, height, width, track);
            continue;
        }
        let outer = if index == 0 { track_radius } else { unsharpen };
        fill(
            cr,
            colours.highlight,
            x,
            top,
            span,
            track,
            [outer, unsharpen, unsharpen, outer],
        );
    }

    let last = right.len() - 2;
    for index in 0..=last {
        let start = if index > 0 {
            DIVIDER_MARGIN
        } else {
            HANDLE_MARGIN
        };
        let end = if index < last { DIVIDER_MARGIN } else { 0.0 };
        let x = right[index] * reach
            + start
            + if index == 0 { handle / 2.0 } else { 0.0 }
            + HANDLE_MARGIN;
        let span = (right[index + 1] - right[index]) * reach
            - start
            - end
            - if index == 0 { handle / 2.0 } else { 0.0 }
            + if index == last { HANDLE_MARGIN } else { 0.0 };
        let outer = if index == last {
            track_radius
        } else {
            unsharpen
        };
        fill(
            cr,
            colours.track,
            x,
            top,
            span,
            track,
            [unsharpen, outer, outer, unsharpen],
        );
    }

    for value in state.stops.borrow().iter() {
        let stop = (value - state.from) / (state.to - state.from);
        let colour = if stop > at {
            theme.m3.on_secondary_container
        } else {
            theme.m3.on_primary
        };
        fill(
            cr,
            colour,
            HANDLE_MARGIN + stop * reach - DOT / 2.0,
            top + centre(track, DOT),
            DOT,
            DOT,
            [DOT / 2.0; 4],
        );
    }

    let edge = HANDLE_MARGIN + at * reach + handle / 2.0;
    fill(
        cr,
        colours.handle,
        edge - handle,
        centre(height, state.handle_height),
        handle,
        state.handle_height,
        [handle / 2.0; 4],
    );

    let near_full = at >= NEAR_FULL;
    let anchor = if near_full { edge } else { width };
    if let Some(icon) = state.icon {
        glyph(
            cr,
            canvas,
            icon,
            anchor - state.margin.get().value(now),
            height,
            state
                .icon_fade
                .value(now)
                .unwrap_or_else(|| icon_colour(&theme, near_full)),
        );
    }

    if let Some((mark, place)) = state.secondary {
        let distance = place - at;
        let near = distance <= 0.1 && distance > (handle + ICON_MARGIN - ICON_ON_HANDLE) / reach;
        let right = if near {
            edge - ICON_ON_HANDLE
        } else {
            width - ((1.0 - place) * reach + HANDLE_MARGIN + ICON_MARGIN)
        };
        glyph(
            cr,
            canvas,
            mark,
            right,
            height,
            state
                .secondary_fade
                .value(now)
                .unwrap_or_else(|| icon_colour(&theme, at >= place - 0.1)),
        );
    }
}

fn icon_colour(theme: &Theme, on_fill: bool) -> RGBA {
    if on_fill {
        theme.colors.col_on_primary
    } else {
        theme.colors.col_on_secondary_container
    }
}

pub fn wave(
    cr: &cairo::Context,
    colour: RGBA,
    x: f64,
    span: f64,
    height: f64,
    full_length: f64,
    line_width: f64,
) {
    let amplitude = line_width * WAVE_AMPLITUDE;
    let phase = glib::real_time() as f64 / 1000.0 / WAVE_PERIOD_MILLIS;
    let centre = height / 2.0;
    cr.save().ok();
    cr.rectangle(x, 0.0, span.max(0.0), height);
    cr.clip();
    cr.new_path();
    let mut local = line_width / 2.0;
    while local <= span - line_width / 2.0 {
        let y =
            centre + amplitude * (WAVE_FREQUENCY * 2.0 * PI * local / full_length + phase).sin();
        cr.line_to(x + local, y);
        local += 1.0;
    }
    set_source(cr, colour);
    cr.set_line_width(line_width);
    cr.set_line_cap(cairo::LineCap::Round);
    let _ = cr.stroke();
    cr.restore().ok();
}

fn fill(
    cr: &cairo::Context,
    colour: RGBA,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    corners: [f64; 4],
) {
    if width <= 0.0 {
        return;
    }
    set_source(cr, colour);
    rounded(cr, x, y, width, height, corners);
    let _ = cr.fill();
}

fn rounded(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, corners: [f64; 4]) {
    let limit = (width / 2.0).min(height / 2.0);
    let [top_left, top_right, bottom_right, bottom_left] = corners.map(|radius| radius.min(limit));
    cr.new_sub_path();
    cr.arc(
        x + width - top_right,
        y + top_right,
        top_right,
        -PI / 2.0,
        0.0,
    );
    cr.arc(
        x + width - bottom_right,
        y + height - bottom_right,
        bottom_right,
        0.0,
        PI / 2.0,
    );
    cr.arc(
        x + bottom_left,
        y + height - bottom_left,
        bottom_left,
        PI / 2.0,
        PI,
    );
    cr.arc(x + top_left, y + top_left, top_left, PI, 1.5 * PI);
    cr.close_path();
}

fn glyph(
    cr: &cairo::Context,
    canvas: &gtk4::DrawingArea,
    name: &str,
    right: f64,
    height: f64,
    colour: RGBA,
) {
    let layout = canvas.create_pango_layout(Some(name));
    layout.set_font_description(Some(&text::font(
        Family::Material,
        ICON,
        &format!("FILL=0,opsz={ICON},wght=400"),
    )));
    let (glyph_width, glyph_height) = layout.pixel_size();
    set_source(cr, colour);
    cr.move_to(
        right - glyph_width as f64,
        centre(height, glyph_height as f64),
    );
    pangocairo::functions::show_layout(cr, &layout);
}

fn set_source(cr: &cairo::Context, colour: RGBA) {
    cr.set_source_rgba(
        colour.red() as f64,
        colour.green() as f64,
        colour.blue() as f64,
        colour.alpha() as f64,
    );
}

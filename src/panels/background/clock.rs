use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use super::cookie::{Cookie, Look, Moment};
use crate::core::config;
use crate::core::i18n::tr;
use crate::core::watch;
use crate::panels::bar::clock::strftime_from_qt;
use crate::ui::anim::{EMPHASIZED, EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, is_dark, pixel_size, with_lightness};
use crate::ui::widgets::column::Column;
use crate::ui::widgets::text::{self, Family};

const SPACING: i32 = 10;
pub(super) const PLACEMENT_PADDING: f64 = 200.0;
pub(super) const MOVE_MILLIS: f64 = 500.0;
pub(super) const FADE_MILLIS: f64 = 200.0;
pub(super) const RESIZE_MILLIS: f64 = 300.0;
pub(super) const DRAG_SCALE: f64 = 1.05;
const STATUS_PADDING: i32 = 5;
const STATUS_SPACING: i32 = 14;
const STATUS_TEXT_SPACING: i32 = 4;
const DIGITAL_SPACING: i32 = 4;
const SECOND_LINE_OFFSET: i32 = -40;
const DATE_OFFSET: i32 = -20;
const CLOCK_TEXT_SIZE: f64 = 20.0;

#[derive(Clone, PartialEq)]
struct Settings {
    enable: bool,
    only_locked: bool,
    free: bool,
    x: f64,
    y: f64,
    style: String,
    style_locked: String,
    quote: String,
    adaptive: bool,
    show_date: bool,
    animate: bool,
    vertical: bool,
    family: String,
    weight: f64,
    width: f64,
    size: f64,
    roundness: f64,
    centre: bool,
    locked_text: bool,
    blur: bool,
    time_format: String,
    date_format: String,
    parallax: f64,
    zoom: f64,
}

impl Settings {
    fn read(root: &Value) -> Self {
        let at = |pointer: &str| root.pointer(pointer);
        let text = |pointer: &str, default: &str| {
            at(pointer)
                .and_then(Value::as_str)
                .unwrap_or(default)
                .to_owned()
        };
        let flag =
            |pointer: &str, default: bool| at(pointer).and_then(Value::as_bool).unwrap_or(default);
        let number =
            |pointer: &str, default: f64| at(pointer).and_then(Value::as_f64).unwrap_or(default);
        let clock = "/background/widgets/clock";
        let quote = if flag(&format!("{clock}/quote/enable"), false) {
            text(&format!("{clock}/quote/text"), "")
        } else {
            String::new()
        };
        Settings {
            enable: flag(&format!("{clock}/enable"), true),
            only_locked: flag(&format!("{clock}/showOnlyWhenLocked"), false),
            free: text(&format!("{clock}/placementStrategy"), "random") == "free",
            x: number(&format!("{clock}/x"), 100.0),
            y: number(&format!("{clock}/y"), 100.0),
            style: text(&format!("{clock}/style"), "cookie"),
            style_locked: text(&format!("{clock}/styleLocked"), "cookie"),
            quote,
            adaptive: flag(&format!("{clock}/digital/adaptiveAlignment"), true),
            show_date: flag(&format!("{clock}/digital/showDate"), true),
            animate: flag(&format!("{clock}/digital/animateChange"), true),
            vertical: flag(&format!("{clock}/digital/vertical"), false),
            family: text(&format!("{clock}/digital/font/family"), "Google Sans Flex"),
            weight: number(&format!("{clock}/digital/font/weight"), 350.0),
            width: number(&format!("{clock}/digital/font/width"), 100.0),
            size: number(&format!("{clock}/digital/font/size"), 90.0),
            roundness: number(&format!("{clock}/digital/font/roundness"), 0.0),
            centre: flag("/lock/centerClock", true),
            locked_text: flag("/lock/showLockedText", true),
            blur: flag("/lock/blur/enable", true),
            time_format: text("/time/format", "hh:mm"),
            date_format: text("/time/dateFormat", "ddd, dd/MM"),
            parallax: number("/background/parallax/widgetsFactor", 1.2),
            zoom: number("/background/parallax/workspaceZoom", 1.07),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Centre,
    Right,
}

impl Align {
    fn xalign(self) -> f32 {
        match self {
            Align::Left => 0.0,
            Align::Centre => 0.5,
            Align::Right => 1.0,
        }
    }
}

pub struct Clock {
    pub root: gtk4::Widget,
    scaled: Scaled,
    cookie: Cookie,
    cookie_section: Column,
    quote: gtk4::Box,
    quote_label: gtk4::Label,
    digital: Digital,
    time: gtk4::Label,
    set_time: Rc<dyn Fn(&str)>,
    minutes: gtk4::Label,
    date: gtk4::Label,
    digital_quote: gtk4::Label,
    badge: Badge,
    safety_row: gtk4::Box,
    safety_labels: [gtk4::Label; 2],
    lock_row: gtk4::Box,
    lock_labels: [gtk4::Label; 2],
    theme: SharedTheme,
    screen: (f64, f64),
    settings: RefCell<Option<Settings>>,
    locked: Cell<bool>,
    safety: Cell<bool>,
    random: Cell<Option<(f64, f64)>>,
    dragged: Cell<Option<(f64, f64)>>,
    x: Cell<Tween>,
    y: Cell<Tween>,
    placed: Cell<bool>,
    opacity: Cell<Tween>,
    safety_fade: Cell<Tween>,
    lock_fade: Cell<Tween>,
    scale: Cell<Tween>,
    ticking: Cell<bool>,
    ticker: RefCell<Option<glib::SourceId>>,
    watches: RefCell<Vec<watch::Watch>>,
    moved: RefCell<Option<Box<dyn Fn()>>>,
    me: RefCell<Weak<Clock>>,
}

impl Clock {
    pub fn new(theme: &SharedTheme, screen: (f64, f64)) -> Rc<Self> {
        let cookie = Cookie::new(theme);
        let quote_label = gtk4::Label::new(None);
        quote_label.set_xalign(0.0);
        text::set_font(
            &quote_label,
            Family::Reading,
            pixel_size::LARGE as f64,
            "wght=400",
        );
        text::set_color(&quote_label, "colOnSecondaryContainer");
        let quote_icon = text::symbol("format_quote", pixel_size::HUGE as f64);
        quote_icon.set_valign(gtk4::Align::Start);
        text::set_color(&quote_icon, "colOnSecondaryContainer");
        let quote = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        quote.add_css_class("clock-quote");
        quote.append(&quote_icon);
        quote.append(&quote_label);
        let cookie_section = Column::new(SPACING, true);
        cookie_section.append(&cookie);
        cookie_section.append(&quote);

        let time = clock_text();
        let (time_holder, set_time) = text::animate_change(&time);
        let minutes = clock_text();
        let date = clock_text();
        let digital_quote = clock_text();
        text::set_font(
            &digital_quote,
            Family::Expressive,
            pixel_size::NORMAL as f64,
            "wght=350",
        );
        let digital = Digital::new(&[
            (time_holder.clone(), 0),
            (minutes.clone().upcast(), SECOND_LINE_OFFSET),
            (date.clone().upcast(), DATE_OFFSET),
            (digital_quote.clone().upcast(), 0),
        ]);

        let (safety_row, safety_labels) =
            status_row("hide_image", &tr("Wallpaper safety enforced"));
        let (lock_row, lock_labels) = status_row("lock", &tr("Locked"));
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, STATUS_SPACING);
        let filler = || {
            let filler = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            filler.set_size_request(1, -1);
            filler
        };
        row.append(&filler());
        row.append(&safety_row);
        row.append(&lock_row);
        row.append(&filler());
        let badge = Badge::new(&row);

        let column = Column::new(SPACING, true);
        column.append(&cookie_section);
        column.append(&digital);
        column.append(&badge);
        let scaled = Scaled::new(&column);

        let clock = Rc::new(Clock {
            root: scaled.clone().upcast(),
            scaled,
            cookie,
            cookie_section,
            quote,
            quote_label,
            digital,
            time,
            set_time,
            minutes,
            date,
            digital_quote,
            badge,
            safety_row,
            safety_labels,
            lock_row,
            lock_labels,
            theme: theme.clone(),
            screen,
            settings: RefCell::new(None),
            locked: Cell::new(false),
            safety: Cell::new(false),
            random: Cell::new(None),
            dragged: Cell::new(None),
            x: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
            y: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
            placed: Cell::new(false),
            opacity: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            safety_fade: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            lock_fade: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            scale: Cell::new(Tween::new(1.0, RESIZE_MILLIS, EMPHASIZED)),
            ticking: Cell::new(false),
            ticker: RefCell::new(None),
            watches: RefCell::new(Vec::new()),
            moved: RefCell::new(None),
            me: RefCell::new(Weak::new()),
        });
        clock.me.replace(Rc::downgrade(&clock));

        let drag = gtk4::GestureDrag::new();
        let start = Rc::new(Cell::new((0.0, 0.0)));
        drag.connect_drag_begin({
            let clock = Rc::downgrade(&clock);
            let start = start.clone();
            move |gesture, x, y| {
                if let Some(clock) = clock.upgrade() {
                    start.set(in_window(&clock.root, x, y));
                    clock.press(gesture);
                }
            }
        });
        drag.connect_drag_update({
            let clock = Rc::downgrade(&clock);
            let start = start.clone();
            move |gesture, _, _| {
                if let Some(clock) = clock.upgrade() {
                    let (dx, dy) = travelled(gesture, &clock.root, start.get());
                    clock.follow(dx, dy);
                }
            }
        });
        drag.connect_drag_end({
            let clock = Rc::downgrade(&clock);
            move |_, _, _| {
                if let Some(clock) = clock.upgrade() {
                    clock.release();
                }
            }
        });
        clock.root.add_controller(drag);

        for pointer in [
            "/background/widgets/clock",
            "/lock",
            "/time",
            "/background/parallax",
        ] {
            let weak = Rc::downgrade(&clock);
            clock
                .watches
                .borrow_mut()
                .push(watch::config(pointer, move || {
                    if let Some(clock) = weak.upgrade() {
                        clock.reload();
                    }
                }));
        }
        clock.reload();
        clock
    }

    pub fn connect_moved(&self, action: impl Fn() + 'static) {
        self.moved.replace(Some(Box::new(action)));
    }

    fn now(&self) -> i64 {
        self.root
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn reload(self: &Rc<Self>) {
        let root = config::root().unwrap_or(Value::Null);
        let settings = Settings::read(&root);
        self.cookie.set_look(Look::read(&root));
        let placement_changed = self
            .settings
            .borrow()
            .as_ref()
            .is_none_or(|old| old.free != settings.free);
        self.settings.replace(Some(settings));
        if placement_changed {
            self.random.set(None);
        }
        self.refresh();
    }

    pub fn set_locked(self: &Rc<Self>, locked: bool) {
        if self.locked.replace(locked) != locked {
            self.refresh();
        }
    }

    pub fn set_safety(self: &Rc<Self>, safety: bool) {
        if self.safety.replace(safety) != safety {
            self.refresh();
        }
    }

    pub fn wallpaper_changed(self: &Rc<Self>) {
        self.random.set(None);
        self.moved();
    }

    fn moved(&self) {
        if let Some(moved) = self.moved.borrow().as_ref() {
            moved();
        }
    }

    fn refresh(self: &Rc<Self>) {
        let Some(settings) = self.settings.borrow().clone() else {
            return;
        };
        let locked = self.locked.get();
        let style = if locked {
            &settings.style_locked
        } else {
            &settings.style
        };
        let cookie = style == "cookie";
        let showing = !settings.only_locked || locked;
        self.cookie_section.set_visible(cookie && showing);
        self.quote.set_visible(!settings.quote.is_empty());
        self.quote_label.set_text(&settings.quote);
        self.digital.set_visible(style == "digital" && showing);
        self.minutes.set_visible(settings.vertical);
        self.date.set_visible(settings.show_date);
        self.digital_quote.set_visible(!settings.quote.is_empty());
        self.digital_quote.set_text(&settings.quote);

        let colour = self.text_colour(&settings);
        let digital_font = {
            let mut font = pango::FontDescription::new();
            font.set_family(&settings.family);
            font.set_absolute_size(settings.size * pango::SCALE as f64);
            font.set_variations(Some(&format!(
                "wght={},wdth={},ROND={}",
                settings.weight, settings.width, settings.roundness
            )));
            font
        };
        for label in [&self.time, &self.minutes] {
            let attributes = pango::AttrList::new();
            attributes.insert(pango::AttrFontDesc::new(&digital_font));
            label.set_attributes(Some(&attributes));
        }
        for label in [&self.time, &self.minutes, &self.date, &self.digital_quote] {
            paint_label(label, colour);
        }
        let status_colour = if cookie {
            self.theme.borrow().colors.col_on_secondary_container
        } else {
            colour
        };
        for label in self.safety_labels.iter().chain(&self.lock_labels) {
            paint_label(label, status_colour);
        }
        if cookie {
            self.badge.add_css_class("cookie");
        } else {
            self.badge.remove_css_class("cookie");
        }

        let now = self.now();
        let animate = self.root.is_mapped();
        let mut opacity = self.opacity.get();
        steer(
            &mut opacity,
            if settings.enable { 1.0 } else { 0.0 },
            now,
            animate,
        );
        self.opacity.set(opacity);
        let mut safety = self.safety_fade.get();
        steer(
            &mut safety,
            if self.safety.get() { 1.0 } else { 0.0 },
            now,
            animate,
        );
        self.safety_fade.set(safety);
        let mut lock = self.lock_fade.get();
        let lock_shown = locked && settings.locked_text;
        steer(&mut lock, if lock_shown { 1.0 } else { 0.0 }, now, animate);
        self.lock_fade.set(lock);
        let status = self.safety.get() || lock_shown;
        self.badge.set_shown(status, now, animate);

        self.show_time();
        self.arm();
        self.run();
        self.moved();
    }

    fn text_colour(&self, settings: &Settings) -> RGBA {
        let theme = self.theme.borrow();
        if self.locked.get() && settings.blur {
            return theme.colors.col_on_layer0;
        }
        let primary = theme.colors.col_primary;
        with_lightness(primary, if is_dark(primary) { 0.8 } else { 0.12 })
    }

    fn show_time(&self) {
        let Some(settings) = self.settings.borrow().clone() else {
            return;
        };
        let Ok(now) = glib::DateTime::now_local() else {
            return;
        };
        let format = |pattern: &str| {
            now.format(&strftime_from_qt(pattern))
                .map(|text| text.to_string())
                .unwrap_or_default()
        };
        let time = format(&settings.time_format);
        let numbers: Vec<String> = time
            .split([':', ' '])
            .map(|part| format!("{part:0>2}"))
            .collect();
        let hour = numbers
            .first()
            .and_then(|hour| hour.parse::<i32>().ok())
            .unwrap_or(0)
            % 12;
        self.cookie.set_moment(Moment {
            hour,
            minute: now.minute(),
            second: now.second(),
            numbers: numbers.clone(),
            day: format("d"),
            day_padded: format("dd"),
            month: format("MM"),
            weekday_day: format("ddd dd"),
        });
        let top = if settings.vertical {
            numbers.first().cloned().unwrap_or_default()
        } else {
            time.clone()
        };
        if settings.animate {
            (self.set_time)(&top);
        } else {
            self.time.set_text(&top);
        }
        let minute = time
            .split(':')
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .map(|part| format!("{part:0>2}"))
            .unwrap_or_default();
        self.minutes.set_text(&minute);
        self.date.set_text(&format(&settings.date_format));
    }

    fn arm(self: &Rc<Self>) {
        if let Some(ticker) = self.ticker.take() {
            ticker.remove();
        }
        let Some(settings) = self.settings.borrow().clone() else {
            return;
        };
        let seconds = self.locked.get()
            || config::value_bool("/time/secondPrecision", false)
            || settings.time_format.contains('s');
        let Ok(now) = glib::DateTime::now_local() else {
            return;
        };
        let into_second = now.microsecond() as u64;
        let wait = if seconds {
            1_000_000 - into_second
        } else {
            (60 - now.second() as u64) * 1_000_000 - into_second
        };
        let weak: Weak<Clock> = Rc::downgrade(self);
        let ticker = glib::timeout_add_local_once(Duration::from_micros(wait + 1000), move || {
            if let Some(clock) = weak.upgrade() {
                clock.ticker.take();
                clock.show_time();
                clock.arm();
            }
        });
        self.ticker.replace(Some(ticker));
    }

    pub fn position(&self, width: f64, height: f64) -> (f64, f64) {
        let Some(settings) = self.settings.borrow().clone() else {
            return (0.0, 0.0);
        };
        let (screen_width, screen_height) = self.screen;
        let target = if self.locked.get() && settings.centre {
            ((screen_width - width) / 2.0, (screen_height - height) / 2.0)
        } else if let Some(dragged) = self.dragged.get() {
            dragged
        } else if settings.free {
            (
                settings.x.min(screen_width - width).max(0.0),
                settings.y.min(screen_height - height).max(0.0),
            )
        } else {
            let random = self.random.get().unwrap_or_else(|| {
                let pick = |screen: f64, size: f64| {
                    let most = screen - size - PLACEMENT_PADDING;
                    (PLACEMENT_PADDING
                        + glib::random_double() * (most - PLACEMENT_PADDING).max(0.0))
                    .max(0.0)
                };
                let spot = (pick(screen_width, width), pick(screen_height, height));
                self.random.set(Some(spot));
                spot
            });
            random
        };
        let now = self.now();
        let animate = self.placed.replace(true) && self.dragged.get().is_none();
        let mut x = self.x.get();
        let mut y = self.y.get();
        steer(&mut x, target.0, now, animate);
        steer(&mut y, target.1, now, animate);
        self.x.set(x);
        self.y.set(y);
        if x.running(now) || y.running(now) {
            self.wake();
        }
        (x.value(now), y.value(now))
    }

    pub fn parallax(&self) -> f64 {
        if self.locked.get() {
            return 0.0;
        }
        self.settings
            .borrow()
            .as_ref()
            .map_or(0.0, |settings| settings.parallax / settings.zoom)
    }

    pub fn draggable(&self) -> bool {
        !self.locked.get()
            && self
                .settings
                .borrow()
                .as_ref()
                .is_some_and(|settings| settings.free && settings.enable)
    }

    fn press(&self, gesture: &gtk4::GestureDrag) {
        if !self.draggable() {
            gesture.set_state(gtk4::EventSequenceState::Denied);
            return;
        }
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let now = self.now();
        self.dragged
            .set(Some((self.x.get().value(now), self.y.get().value(now))));
        self.root.set_cursor_from_name(Some("grabbing"));
        let mut scale = self.scale.get();
        scale.retarget(DRAG_SCALE, now);
        self.scale.set(scale);
        self.apply(now);
        self.wake();
    }

    fn follow(&self, dx: f64, dy: f64) {
        if self.dragged.get().is_none() {
            return;
        }
        let origin = self.drag_origin();
        let spot = (origin.0 + dx, origin.1 + dy);
        let mut x = self.x.get();
        let mut y = self.y.get();
        x.jump(spot.0);
        y.jump(spot.1);
        self.x.set(x);
        self.y.set(y);
        self.dragged.set(Some(spot));
        self.moved();
    }

    fn drag_origin(&self) -> (f64, f64) {
        let settings = self.settings.borrow().clone();
        let (screen_width, screen_height) = self.screen;
        let width = self.root.width() as f64;
        let height = self.root.height() as f64;
        settings.map_or((0.0, 0.0), |settings| {
            (
                settings.x.min(screen_width - width).max(0.0),
                settings.y.min(screen_height - height).max(0.0),
            )
        })
    }

    fn release(self: &Rc<Self>) {
        let Some((x, y)) = self.dragged.get() else {
            return;
        };
        let now = self.now();
        config::store_value("/background/widgets/clock/x", Value::from(x));
        config::store_value("/background/widgets/clock/y", Value::from(y));
        if let Some(settings) = self.settings.borrow_mut().as_mut() {
            settings.x = x;
            settings.y = y;
        }
        self.dragged.set(None);
        self.root.set_cursor_from_name(Some("grab"));
        let mut scale = self.scale.get();
        scale.retarget(1.0, now);
        self.scale.set(scale);
        self.run();
        self.moved();
    }

    fn run(&self) {
        self.apply(self.now());
        self.wake();
    }

    fn wake(&self) {
        if self.ticking.replace(true) {
            return;
        }
        let weak = self.me.borrow().clone();
        self.root.add_tick_callback(move |_, clock| {
            let Some(me) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            me.apply(now);
            me.moved();
            let running = [
                me.x.get(),
                me.y.get(),
                me.opacity.get(),
                me.safety_fade.get(),
                me.lock_fade.get(),
                me.scale.get(),
            ]
            .iter()
            .any(|tween| tween.running(now))
                || me.badge.running(now);
            if running {
                return glib::ControlFlow::Continue;
            }
            me.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn apply(&self, now: i64) {
        self.root.set_opacity(self.opacity.get().value(now));
        let safety = self.safety_fade.get().value(now);
        self.safety_row.set_opacity(safety);
        self.safety_row.set_visible(safety > 0.0);
        let lock = self.lock_fade.get().value(now);
        self.lock_row.set_opacity(lock);
        self.lock_row.set_visible(lock > 0.0);
        self.scaled.set_scale(self.scale.get().value(now));
        self.badge.step(now);
        let cursor = if self.draggable() { "grab" } else { "default" };
        if self.dragged.get().is_none() {
            self.root.set_cursor_from_name(Some(cursor));
        }
        if let Some(settings) = self.settings.borrow().as_ref() {
            let align = self.align(settings);
            for label in [&self.minutes, &self.date, &self.digital_quote] {
                label.set_xalign(align.xalign());
            }
        }
    }

    fn align(&self, settings: &Settings) -> Align {
        if !settings.adaptive || (self.locked.get() && settings.centre) || settings.vertical {
            return Align::Centre;
        }
        let x = self.x.get().target();
        let width = self.screen.0;
        if x < width / 3.0 {
            Align::Left
        } else if x > width * 2.0 / 3.0 {
            Align::Right
        } else {
            Align::Centre
        }
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        if let Some(ticker) = self.ticker.take() {
            ticker.remove();
        }
    }
}

pub(super) fn steer(tween: &mut Tween, target: f64, now: i64, animate: bool) {
    if animate {
        tween.retarget(target, now);
    } else {
        tween.jump(target);
    }
}

pub(super) fn in_window(widget: &gtk4::Widget, x: f64, y: f64) -> (f64, f64) {
    widget
        .root()
        .and_then(|root| {
            widget.compute_point(&root, &gtk4::graphene::Point::new(x as f32, y as f32))
        })
        .map_or((x, y), |point| (point.x() as f64, point.y() as f64))
}

/// The pointer's travel since `start`, measured in the window, because the clock moves under
/// the pointer and the gesture's own offsets are in the clock's shifting coordinates.
pub(super) fn travelled(
    gesture: &gtk4::GestureDrag,
    widget: &gtk4::Widget,
    start: (f64, f64),
) -> (f64, f64) {
    let Some((x, y)) = gesture.point(gesture.current_sequence().as_ref()) else {
        return (0.0, 0.0);
    };
    let (x, y) = in_window(widget, x, y);
    (x - start.0, y - start.1)
}

fn clock_text() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.add_css_class("raised");
    text::set_font(&label, Family::Expressive, CLOCK_TEXT_SIZE, "wght=350");
    label
}

fn paint_label(label: &gtk4::Label, colour: RGBA) {
    let attributes = label.attributes().unwrap_or_default();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
    attributes.change(pango::AttrColor::new_foreground(
        channel(colour.red()),
        channel(colour.green()),
        channel(colour.blue()),
    ));
    attributes.change(pango::AttrInt::new_foreground_alpha(
        channel(colour.alpha()).max(1),
    ));
    label.set_attributes(Some(&attributes));
}

fn status_row(icon: &str, label: &str) -> (gtk4::Box, [gtk4::Label; 2]) {
    let symbol = text::symbol(icon, pixel_size::HUGE as f64);
    symbol.add_css_class("raised");
    symbol.set_valign(gtk4::Align::Center);
    let words = clock_text();
    text::set_font(
        &words,
        Family::Expressive,
        pixel_size::LARGE as f64,
        "wght=400",
    );
    words.set_text(label);
    words.set_valign(gtk4::Align::Center);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, STATUS_TEXT_SPACING);
    row.append(&symbol);
    row.append(&words);
    row.set_visible(false);
    (row, [symbol, words])
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Digital {
        pub offsets: RefCell<Vec<i32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Digital {
        const NAME: &'static str = "ProscenioDigitalClock";
        type Type = super::Digital;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Digital {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Digital {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let rows = self.rows();
            let size = match orientation {
                gtk4::Orientation::Horizontal => rows
                    .iter()
                    .map(|(child, _)| child.measure(orientation, -1).1)
                    .max()
                    .unwrap_or(0),
                _ => rows
                    .iter()
                    .enumerate()
                    .map(|(index, (child, offset))| {
                        let gap = if index == 0 {
                            0
                        } else {
                            DIGITAL_SPACING + offset
                        };
                        child.measure(orientation, -1).1 + gap
                    })
                    .sum::<i32>()
                    .max(0),
            };
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let mut y = 0;
            for (index, (child, offset)) in self.rows().into_iter().enumerate() {
                if index > 0 {
                    y += DIGITAL_SPACING + offset;
                }
                let height = child.measure(gtk4::Orientation::Vertical, width).1;
                let place = gtk4::gsk::Transform::new()
                    .translate(&gtk4::graphene::Point::new(0.0, y as f32));
                child.allocate(width, height, -1, Some(place));
                y += height;
            }
        }
    }

    impl Digital {
        fn rows(&self) -> Vec<(gtk4::Widget, i32)> {
            let offsets = self.offsets.borrow();
            let mut rows = Vec::new();
            let mut child = self.obj().first_child();
            let mut index = 0;
            while let Some(widget) = child {
                if widget.is_visible() {
                    rows.push((widget.clone(), offsets.get(index).copied().unwrap_or(0)));
                }
                index += 1;
                child = widget.next_sibling();
            }
            rows
        }
    }

    pub struct Badge {
        pub width: Cell<Tween>,
        pub height: Cell<Tween>,
        pub opacity: Cell<Tween>,
        pub sized: Cell<bool>,
    }

    impl Default for Badge {
        fn default() -> Self {
            Badge {
                width: Cell::new(Tween::new(0.0, RESIZE_MILLIS, EMPHASIZED)),
                height: Cell::new(Tween::new(0.0, RESIZE_MILLIS, EMPHASIZED)),
                opacity: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
                sized: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Badge {
        const NAME: &'static str = "ProscenioClockBadge";
        type Type = super::Badge;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Badge {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.add_css_class("clock-status");
            obj.set_overflow(gtk4::Overflow::Hidden);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Badge {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let obj = self.obj();
            let now = obj
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time);
            let Some(child) = obj.first_child() else {
                return (0, 0, -1, -1);
            };
            let natural = child.measure(orientation, -1).1 + STATUS_PADDING * 2;
            let tween = match orientation {
                gtk4::Orientation::Horizontal => &self.width,
                _ => &self.height,
            };
            let mut value = tween.get();
            if self.sized.get() && obj.is_mapped() {
                value.retarget(natural as f64, now);
            } else {
                value.jump(natural as f64);
            }
            tween.set(value);
            let size = value.value(now).round() as i32;
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            self.sized.set(true);
            let Some(child) = self.obj().first_child() else {
                return;
            };
            let natural_width = child.measure(gtk4::Orientation::Horizontal, -1).1;
            let natural_height = child.measure(gtk4::Orientation::Vertical, -1).1;
            let place = gtk4::gsk::Transform::new().translate(&gtk4::graphene::Point::new(
                ((width - natural_width) / 2) as f32,
                ((height - natural_height) / 2) as f32,
            ));
            child.allocate(natural_width, natural_height, -1, Some(place));
        }
    }

    #[derive(Default)]
    pub struct Scaled {
        pub scale: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Scaled {
        const NAME: &'static str = "ProscenioScaled";
        type Type = super::Scaled;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Scaled {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Scaled {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().first_child() {
                Some(child) => child.measure(orientation, for_size),
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let Some(child) = self.obj().first_child() else {
                return;
            };
            let scale = self.scale.get() as f32;
            let (half_width, half_height) = (width as f32 / 2.0, height as f32 / 2.0);
            let place = gtk4::gsk::Transform::new()
                .translate(&gtk4::graphene::Point::new(half_width, half_height))
                .scale(scale, scale)
                .translate(&gtk4::graphene::Point::new(-half_width, -half_height));
            child.allocate(width, height, baseline, Some(place));
        }
    }
}

glib::wrapper! {
    pub struct Digital(ObjectSubclass<imp::Digital>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Digital {
    fn new(rows: &[(gtk4::Widget, i32)]) -> Self {
        let digital: Self = glib::Object::new();
        for (row, _) in rows {
            row.set_parent(&digital);
        }
        digital
            .imp()
            .offsets
            .replace(rows.iter().map(|(_, offset)| *offset).collect());
        digital
    }
}

glib::wrapper! {
    pub struct Badge(ObjectSubclass<imp::Badge>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Badge {
    fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let badge: Self = glib::Object::new();
        child.set_parent(&badge);
        badge
    }

    fn set_shown(&self, shown: bool, now: i64, animate: bool) {
        let imp = self.imp();
        let mut opacity = imp.opacity.get();
        steer(&mut opacity, if shown { 1.0 } else { 0.0 }, now, animate);
        imp.opacity.set(opacity);
        self.set_opacity(opacity.value(now));
    }

    fn running(&self, now: i64) -> bool {
        let imp = self.imp();
        imp.width.get().running(now)
            || imp.height.get().running(now)
            || imp.opacity.get().running(now)
    }

    fn step(&self, now: i64) {
        let imp = self.imp();
        if imp.width.get().running(now) || imp.height.get().running(now) {
            self.queue_resize();
        }
        self.set_opacity(imp.opacity.get().value(now));
    }
}

glib::wrapper! {
    pub struct Scaled(ObjectSubclass<imp::Scaled>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Scaled {
    pub(super) fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let scaled: Self = glib::Object::new();
        scaled.imp().scale.set(1.0);
        child.set_parent(&scaled);
        scaled
    }

    pub(super) fn set_scale(&self, scale: f64) {
        if (self.imp().scale.replace(scale) - scale).abs() > f64::EPSILON {
            self.queue_allocate();
        }
    }
}

mod settings;
pub(super) mod widgets;

use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use super::cookie::Cookie;
use super::cookie::state::{Look, Moment};
use crate::core::config;
use crate::core::i18n::tr;
use crate::core::watch;
use crate::panels::bar::clock::{strftime_from_qt, time_to_next_tick};
use crate::ui::anim::{EMPHASIZED, EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, is_dark, pixel_size, with_lightness};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::layout::column::Column;
use settings::Settings;
use widgets::{Badge, Digital, Scaled, clock_text, paint_label, status_row};

const SPACING: i32 = 10;
pub(super) const PLACEMENT_PADDING: f64 = 200.0;
pub(super) const MOVE_MILLIS: f64 = 500.0;
pub(super) const FADE_MILLIS: f64 = 200.0;
pub(super) const RESIZE_MILLIS: f64 = 300.0;
pub(super) const DRAG_SCALE: f64 = 1.05;
const STATUS_SPACING: i32 = 14;
const SECOND_LINE_OFFSET: i32 = -40;
const DATE_OFFSET: i32 = -20;

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    fn xalign(self) -> f32 {
        match self {
            Align::Left => 0.0,
            Align::Center => 0.5,
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
    is_locked: Cell<bool>,
    is_safety_enforced: Cell<bool>,
    random: Cell<Option<(f64, f64)>>,
    drag_position: Cell<Option<(f64, f64)>>,
    x: Cell<Tween>,
    y: Cell<Tween>,
    is_placed: Cell<bool>,
    opacity: Cell<Tween>,
    safety_fade: Cell<Tween>,
    lock_fade: Cell<Tween>,
    scale: Cell<Tween>,
    is_ticking: Cell<bool>,
    ticker: RefCell<Option<glib::SourceId>>,
    watches: RefCell<Vec<watch::Watch>>,
    on_moved: RefCell<Option<Box<dyn Fn()>>>,
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
            is_locked: Cell::new(false),
            is_safety_enforced: Cell::new(false),
            random: Cell::new(None),
            drag_position: Cell::new(None),
            x: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
            y: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
            is_placed: Cell::new(false),
            opacity: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            safety_fade: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            lock_fade: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            scale: Cell::new(Tween::new(1.0, RESIZE_MILLIS, EMPHASIZED)),
            is_ticking: Cell::new(false),
            ticker: RefCell::new(None),
            watches: RefCell::new(Vec::new()),
            on_moved: RefCell::new(None),
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
                    start.set(map_to_window(&clock.root, x, y));
                    clock.press(gesture);
                }
            }
        });
        drag.connect_drag_update({
            let clock = Rc::downgrade(&clock);
            let start = start.clone();
            move |gesture, _, _| {
                if let Some(clock) = clock.upgrade() {
                    let (dx, dy) = measure_travel(gesture, &clock.root, start.get());
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
        self.on_moved.replace(Some(Box::new(action)));
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
        let has_placement_changed = self
            .settings
            .borrow()
            .as_ref()
            .is_none_or(|old| old.free != settings.free);
        self.settings.replace(Some(settings));
        if has_placement_changed {
            self.random.set(None);
        }
        self.refresh();
    }

    pub fn set_locked(self: &Rc<Self>, is_locked: bool) {
        if self.is_locked.replace(is_locked) != is_locked {
            self.refresh();
        }
    }

    pub fn set_safety_enforced(self: &Rc<Self>, is_enforced: bool) {
        if self.is_safety_enforced.replace(is_enforced) != is_enforced {
            self.refresh();
        }
    }

    pub fn on_wallpaper_changed(self: &Rc<Self>) {
        self.random.set(None);
        self.notify_moved();
    }

    fn notify_moved(&self) {
        if let Some(on_moved) = self.on_moved.borrow().as_ref() {
            on_moved();
        }
    }

    fn refresh(self: &Rc<Self>) {
        let Some(settings) = self.settings.borrow().clone() else {
            return;
        };
        let is_locked = self.is_locked.get();
        let style = settings.style(is_locked);
        let is_cookie = style == "cookie";
        let is_shown = settings.is_shown(is_locked);
        self.cookie_section.set_visible(is_cookie && is_shown);
        self.quote.set_visible(!settings.quote.is_empty());
        self.quote_label.set_text(&settings.quote);
        self.digital.set_visible(style == "digital" && is_shown);
        self.minutes.set_visible(settings.vertical);
        self.date.set_visible(settings.show_date);
        self.digital_quote.set_visible(!settings.quote.is_empty());
        self.digital_quote.set_text(&settings.quote);

        let color = self.text_color(&settings);
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
            paint_label(label, color);
        }
        let status_color = if is_cookie {
            self.theme.borrow().colors.col_on_secondary_container
        } else {
            color
        };
        for label in self.safety_labels.iter().chain(&self.lock_labels) {
            paint_label(label, status_color);
        }
        if is_cookie {
            self.badge.add_css_class("cookie");
        } else {
            self.badge.remove_css_class("cookie");
        }

        let now = self.now();
        let should_animate = self.root.is_mapped();
        let mut opacity = self.opacity.get();
        steer(
            &mut opacity,
            if settings.enable { 1.0 } else { 0.0 },
            now,
            should_animate,
        );
        self.opacity.set(opacity);
        let mut safety = self.safety_fade.get();
        steer(
            &mut safety,
            if self.is_safety_enforced.get() {
                1.0
            } else {
                0.0
            },
            now,
            should_animate,
        );
        self.safety_fade.set(safety);
        let mut lock = self.lock_fade.get();
        let shows_lock_text = is_locked && settings.locked_text;
        steer(
            &mut lock,
            if shows_lock_text { 1.0 } else { 0.0 },
            now,
            should_animate,
        );
        self.lock_fade.set(lock);
        let shows_status = self.is_safety_enforced.get() || shows_lock_text;
        self.badge.set_shown(shows_status, now, should_animate);

        self.show_time();
        self.arm();
        self.run();
        self.notify_moved();
    }

    fn text_color(&self, settings: &Settings) -> RGBA {
        let theme = self.theme.borrow();
        if self.is_locked.get() && settings.blur {
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
            local_seconds: now.to_unix() + now.utc_offset().as_seconds(),
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
        let Some(period_seconds) = settings.period_seconds(
            self.is_locked.get(),
            config::current().time_second_precision,
        ) else {
            return;
        };
        let Ok(now) = glib::DateTime::now_local() else {
            return;
        };
        let delay = time_to_next_tick(
            now.second() as u64,
            now.microsecond() as u64,
            period_seconds,
        );
        let weak: Weak<Clock> = Rc::downgrade(self);
        let ticker = glib::timeout_add_local_once(delay + Duration::from_millis(1), move || {
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
        let target = if self.is_locked.get() && settings.center {
            ((screen_width - width) / 2.0, (screen_height - height) / 2.0)
        } else if let Some(drag_position) = self.drag_position.get() {
            drag_position
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
        let should_animate = self.is_placed.replace(true) && self.drag_position.get().is_none();
        let mut x = self.x.get();
        let mut y = self.y.get();
        steer(&mut x, target.0, now, should_animate);
        steer(&mut y, target.1, now, should_animate);
        self.x.set(x);
        self.y.set(y);
        if x.is_running(now) || y.is_running(now) {
            self.wake();
        }
        (x.value(now), y.value(now))
    }

    pub fn parallax(&self) -> f64 {
        if self.is_locked.get() {
            return 0.0;
        }
        self.settings
            .borrow()
            .as_ref()
            .map_or(0.0, |settings| settings.parallax / settings.zoom)
    }

    pub fn is_draggable(&self) -> bool {
        !self.is_locked.get()
            && self
                .settings
                .borrow()
                .as_ref()
                .is_some_and(|settings| settings.free && settings.enable)
    }

    fn press(&self, gesture: &gtk4::GestureDrag) {
        if !self.is_draggable() {
            gesture.set_state(gtk4::EventSequenceState::Denied);
            return;
        }
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let now = self.now();
        self.drag_position
            .set(Some((self.x.get().value(now), self.y.get().value(now))));
        self.root.set_cursor_from_name(Some("grabbing"));
        let mut scale = self.scale.get();
        scale.retarget(DRAG_SCALE, now);
        self.scale.set(scale);
        self.apply(now);
        self.wake();
    }

    fn follow(&self, dx: f64, dy: f64) {
        if self.drag_position.get().is_none() {
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
        self.drag_position.set(Some(spot));
        self.notify_moved();
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
        let Some((x, y)) = self.drag_position.get() else {
            return;
        };
        let now = self.now();
        config::store_value("/background/widgets/clock/x", Value::from(x));
        config::store_value("/background/widgets/clock/y", Value::from(y));
        if let Some(settings) = self.settings.borrow_mut().as_mut() {
            settings.x = x;
            settings.y = y;
        }
        self.drag_position.set(None);
        self.root.set_cursor_from_name(Some("grab"));
        let mut scale = self.scale.get();
        scale.retarget(1.0, now);
        self.scale.set(scale);
        self.run();
        self.notify_moved();
    }

    fn run(&self) {
        self.apply(self.now());
        self.wake();
    }

    fn wake(&self) {
        if self.is_ticking.replace(true) {
            return;
        }
        let weak = self.me.borrow().clone();
        self.root.add_tick_callback(move |_, clock| {
            let Some(me) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            me.apply(now);
            me.notify_moved();
            let is_animating = [
                me.x.get(),
                me.y.get(),
                me.opacity.get(),
                me.safety_fade.get(),
                me.lock_fade.get(),
                me.scale.get(),
            ]
            .iter()
            .any(|tween| tween.is_running(now))
                || me.badge.is_running(now);
            if is_animating {
                return glib::ControlFlow::Continue;
            }
            me.is_ticking.set(false);
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
        let cursor = if self.is_draggable() {
            "grab"
        } else {
            "default"
        };
        if self.drag_position.get().is_none() {
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
        if !settings.adaptive || (self.is_locked.get() && settings.center) || settings.vertical {
            return Align::Center;
        }
        let x = self.x.get().target();
        let width = self.screen.0;
        if x < width / 3.0 {
            Align::Left
        } else if x > width * 2.0 / 3.0 {
            Align::Right
        } else {
            Align::Center
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

pub(super) fn steer(tween: &mut Tween, target: f64, now: i64, should_animate: bool) {
    if should_animate {
        tween.retarget(target, now);
    } else {
        tween.jump(target);
    }
}

pub(super) fn map_to_window(widget: &gtk4::Widget, x: f64, y: f64) -> (f64, f64) {
    widget
        .root()
        .and_then(|root| {
            widget.compute_point(&root, &gtk4::graphene::Point::new(x as f32, y as f32))
        })
        .map_or((x, y), |point| (point.x() as f64, point.y() as f64))
}

/// The pointer's travel since `start`, measured in the window, because the clock moves under
/// the pointer and the gesture's own offsets are in the clock's shifting coordinates.
pub(super) fn measure_travel(
    gesture: &gtk4::GestureDrag,
    widget: &gtk4::Widget,
    start: (f64, f64),
) -> (f64, f64) {
    let Some((x, y)) = gesture.point(gesture.current_sequence().as_ref()) else {
        return (0.0, 0.0);
    };
    let (x, y) = map_to_window(widget, x, y);
    (x - start.0, y - start.1)
}

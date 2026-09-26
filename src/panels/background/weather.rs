use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::clock::{
    DRAG_SCALE, FADE_MILLIS, MOVE_MILLIS, PLACEMENT_PADDING, RESIZE_MILLIS, Scaled, in_window,
    steer, travelled,
};
use crate::core::config;
use crate::core::listeners::Subscription;
use crate::core::watch;
use crate::services::weather::{self, Weather};
use crate::ui::anim::{EMPHASIZED, EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::text::{self, Family};

const ENTRY: &str = "/background/widgets/weather";
const SIZE: i32 = 200;
const GLYPH_SIZE: f64 = 80.0;
const SIDE_MARGIN: i32 = 16;
const EDGE_MARGIN: i32 = 20;
const SHADOW_RADIUS: f64 = 8.0;

#[derive(Clone, Copy, PartialEq)]
struct Settings {
    enable: bool,
    free: bool,
    x: f64,
    y: f64,
}

impl Settings {
    fn read() -> Self {
        let root = config::root().unwrap_or(Value::Null);
        let at = |key: &str| root.pointer(&format!("{ENTRY}/{key}"));
        Settings {
            enable: at("enable").and_then(Value::as_bool).unwrap_or(false),
            free: at("placementStrategy")
                .and_then(Value::as_str)
                .unwrap_or("free")
                == "free",
            x: at("x").and_then(Value::as_f64).unwrap_or(400.0),
            y: at("y").and_then(Value::as_f64).unwrap_or(100.0),
        }
    }
}

pub struct WeatherWidget {
    pub root: gtk4::Widget,
    scaled: Scaled,
    screen: (f64, f64),
    settings: Cell<Settings>,
    locked: Cell<bool>,
    random: Cell<Option<(f64, f64)>>,
    dragged: Cell<Option<(f64, f64)>>,
    x: Cell<Tween>,
    y: Cell<Tween>,
    placed: Cell<bool>,
    opacity: Cell<Tween>,
    scale: Cell<Tween>,
    ticking: Cell<bool>,
    watch: RefCell<Option<watch::Watch>>,
    report: RefCell<Option<Subscription>>,
    moved: RefCell<Option<Box<dyn Fn()>>>,
    me: RefCell<Weak<WeatherWidget>>,
}

impl WeatherWidget {
    pub fn new(theme: &SharedTheme, weather: &Weather, screen: (f64, f64)) -> Rc<Self> {
        let shape = Paint::new({
            let theme = theme.clone();
            move |snapshot, width, height| {
                let size = width.min(height);
                let (shade, fill) = {
                    let theme = theme.borrow();
                    (theme.colors.col_shadow, theme.colors.col_primary_container)
                };
                let deviation = (SHADOW_RADIUS + 1.0) / 3.3333;
                let shadow = gsk::Shadow::new(shade, 0.0, 0.0, (deviation * 2.0) as f32);
                snapshot.push_shadow(&[shadow]);
                let cr = snapshot.append_cairo(&graphene::Rect::new(0.0, 0.0, size, size));
                cr.set_source_rgba(
                    fill.red() as f64,
                    fill.green() as f64,
                    fill.blue() as f64,
                    fill.alpha() as f64,
                );
                shapes::polygon(Shape::Pill).trace(&cr, 0.0, 0.0, size as f64);
                let _ = cr.fill();
                drop(cr);
                snapshot.pop();
            }
        });
        shape.set_size_request(SIZE, SIZE);

        let degrees = gtk4::Label::new(Some("--°"));
        text::set_font(&degrees, Family::Expressive, GLYPH_SIZE, "wght=500");
        text::set_color(&degrees, "colPrimary");
        degrees.set_halign(gtk4::Align::End);
        degrees.set_valign(gtk4::Align::Start);
        degrees.set_margin_end(SIDE_MARGIN);
        degrees.set_margin_top(EDGE_MARGIN);

        let symbol = text::symbol("cloud", GLYPH_SIZE);
        text::set_color(&symbol, "colOnPrimaryContainer");
        symbol.set_halign(gtk4::Align::Start);
        symbol.set_valign(gtk4::Align::End);
        symbol.set_margin_start(SIDE_MARGIN);
        symbol.set_margin_bottom(EDGE_MARGIN);

        let card = gtk4::Overlay::new();
        card.set_child(Some(&shape));
        card.add_overlay(&degrees);
        card.add_overlay(&symbol);
        let scaled = Scaled::new(&card);

        let widget = Rc::new(WeatherWidget {
            root: scaled.clone().upcast(),
            scaled,
            screen,
            settings: Cell::new(Settings::read()),
            locked: Cell::new(false),
            random: Cell::new(None),
            dragged: Cell::new(None),
            x: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
            y: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
            placed: Cell::new(false),
            opacity: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            scale: Cell::new(Tween::new(1.0, RESIZE_MILLIS, EMPHASIZED)),
            ticking: Cell::new(false),
            watch: RefCell::new(None),
            report: RefCell::new(None),
            moved: RefCell::new(None),
            me: RefCell::new(Weak::new()),
        });
        widget.me.replace(Rc::downgrade(&widget));

        let show = {
            let weather = weather.clone();
            move || {
                let report = weather.data.borrow();
                symbol.set_text(weather::symbol(&report.code));
                let mut shown = report.temperature.clone();
                shown.pop();
                degrees.set_text(if shown.is_empty() { "--°" } else { &shown });
            }
        };
        show();
        widget.report.replace(Some(weather.subscribe(show)));

        let drag = gtk4::GestureDrag::new();
        let start = Rc::new(Cell::new((0.0, 0.0)));
        drag.connect_drag_begin({
            let widget = Rc::downgrade(&widget);
            let start = start.clone();
            move |gesture, x, y| {
                if let Some(widget) = widget.upgrade() {
                    start.set(in_window(&widget.root, x, y));
                    widget.press(gesture);
                }
            }
        });
        drag.connect_drag_update({
            let widget = Rc::downgrade(&widget);
            move |gesture, _, _| {
                if let Some(widget) = widget.upgrade() {
                    let (dx, dy) = travelled(gesture, &widget.root, start.get());
                    widget.follow(dx, dy);
                }
            }
        });
        drag.connect_drag_end({
            let widget = Rc::downgrade(&widget);
            move |_, _, _| {
                if let Some(widget) = widget.upgrade() {
                    widget.release();
                }
            }
        });
        widget.root.add_controller(drag);

        widget.watch.replace(Some(watch::config(ENTRY, {
            let widget = Rc::downgrade(&widget);
            move || {
                if let Some(widget) = widget.upgrade() {
                    widget.reload();
                }
            }
        })));
        widget.refresh();
        widget
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

    fn moved(&self) {
        if let Some(moved) = self.moved.borrow().as_ref() {
            moved();
        }
    }

    fn reload(&self) {
        let settings = Settings::read();
        if self.settings.replace(settings).free != settings.free {
            self.random.set(None);
        }
        self.refresh();
    }

    pub fn set_locked(&self, locked: bool) {
        if self.locked.replace(locked) != locked {
            self.refresh();
        }
    }

    pub fn wallpaper_changed(&self) {
        self.random.set(None);
        self.moved();
    }

    fn shown(&self) -> bool {
        self.settings.get().enable && !self.locked.get()
    }

    fn refresh(&self) {
        let now = self.now();
        let mut opacity = self.opacity.get();
        let target = if self.shown() { 1.0 } else { 0.0 };
        steer(&mut opacity, target, now, self.root.is_mapped());
        self.opacity.set(opacity);
        self.run();
        self.moved();
    }

    pub fn position(&self, width: f64, height: f64) -> (f64, f64) {
        let settings = self.settings.get();
        let (screen_width, screen_height) = self.screen;
        let target = if let Some(dragged) = self.dragged.get() {
            dragged
        } else if settings.free {
            (
                settings.x.min(screen_width - width).max(0.0),
                settings.y.min(screen_height - height).max(0.0),
            )
        } else {
            self.random.get().unwrap_or_else(|| {
                let pick = |screen: f64, size: f64| {
                    let most = screen - size - PLACEMENT_PADDING;
                    (PLACEMENT_PADDING
                        + glib::random_double() * (most - PLACEMENT_PADDING).max(0.0))
                    .max(0.0)
                };
                let spot = (pick(screen_width, width), pick(screen_height, height));
                self.random.set(Some(spot));
                spot
            })
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

    pub fn draggable(&self) -> bool {
        self.shown() && self.settings.get().free
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
        self.run();
    }

    fn follow(&self, dx: f64, dy: f64) {
        if self.dragged.get().is_none() {
            return;
        }
        let settings = self.settings.get();
        let (screen_width, screen_height) = self.screen;
        let origin = (
            settings
                .x
                .min(screen_width - self.root.width() as f64)
                .max(0.0),
            settings
                .y
                .min(screen_height - self.root.height() as f64)
                .max(0.0),
        );
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

    fn release(&self) {
        let Some((x, y)) = self.dragged.get() else {
            return;
        };
        config::store_value(&format!("{ENTRY}/x"), Value::from(x));
        config::store_value(&format!("{ENTRY}/y"), Value::from(y));
        self.settings.set(Settings {
            x,
            y,
            ..self.settings.get()
        });
        self.dragged.set(None);
        let mut scale = self.scale.get();
        scale.retarget(1.0, self.now());
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
            let running = [me.x.get(), me.y.get(), me.opacity.get(), me.scale.get()]
                .iter()
                .any(|tween| tween.running(now));
            if running {
                return glib::ControlFlow::Continue;
            }
            me.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn apply(&self, now: i64) {
        let fade = self.opacity.get();
        let opacity = fade.value(now);
        self.root.set_opacity(opacity);
        self.root.set_visible(opacity > 0.0 || fade.target() > 0.0);
        self.scaled.set_scale(self.scale.get().value(now));
        if self.dragged.get().is_none() {
            let cursor = if self.draggable() { "grab" } else { "default" };
            self.root.set_cursor_from_name(Some(cursor));
        }
    }
}

use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

const FADE_MILLIS: f64 = 200.0;

#[derive(Clone, Copy)]
pub enum Ease {
    Bezier(f64, f64, f64, f64),
    Emphasized,
    OutSine,
    OutCubic,
    InOutQuad,
}

pub const EMPHASIZED: Ease = Ease::Emphasized;

pub const EXPRESSIVE_FAST: Ease = Ease::Bezier(0.42, 1.67, 0.21, 0.90);
pub const EXPRESSIVE_DEFAULT: Ease = Ease::Bezier(0.38, 1.21, 0.22, 1.00);
pub const EMPHASIZED_DECEL: Ease = Ease::Bezier(0.05, 0.7, 0.1, 1.0);
pub const EXPRESSIVE_EFFECTS: Ease = Ease::Bezier(0.34, 0.80, 0.34, 1.00);
pub const STANDARD_DECEL: Ease = Ease::Bezier(0.0, 0.0, 0.0, 1.0);

impl Ease {
    pub fn at(self, part: f64) -> f64 {
        match self {
            Ease::OutSine => (part * std::f64::consts::FRAC_PI_2).sin(),
            Ease::OutCubic => 1.0 - (1.0 - part).powi(3),
            Ease::InOutQuad if part < 0.5 => 2.0 * part * part,
            Ease::InOutQuad => 1.0 - (-2.0 * part + 2.0).powi(2) / 2.0,
            Ease::Bezier(x1, y1, x2, y2) => bezier(part, x1, y1, x2, y2),
            Ease::Emphasized => {
                let (joint_x, joint_y) = (1.0 / 6.0, 0.4);
                if part < joint_x {
                    let local = part / joint_x;
                    joint_y
                        * bezier(
                            local,
                            0.05 / joint_x,
                            0.0,
                            (2.0 / 15.0) / joint_x,
                            0.06 / joint_y,
                        )
                } else {
                    let span_x = 1.0 - joint_x;
                    let span_y = 1.0 - joint_y;
                    let local = (part - joint_x) / span_x;
                    joint_y
                        + span_y
                            * bezier(
                                local,
                                (5.0 / 24.0 - joint_x) / span_x,
                                (0.82 - joint_y) / span_y,
                                (0.25 - joint_x) / span_x,
                                (1.0 - joint_y) / span_y,
                            )
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct Tween {
    from: f64,
    to: f64,
    start: i64,
    micros: f64,
    ease: Ease,
}

impl Tween {
    pub fn new(value: f64, millis: f64, ease: Ease) -> Self {
        Tween {
            from: value,
            to: value,
            start: 0,
            micros: millis * 1000.0,
            ease,
        }
    }

    pub fn value(&self, now: i64) -> f64 {
        let part = self.part(now);
        self.from + (self.to - self.from) * self.ease.at(part)
    }

    pub fn target(&self) -> f64 {
        self.to
    }

    pub fn is_running(&self, now: i64) -> bool {
        self.part(now) < 1.0
    }

    pub fn retarget(&mut self, target: f64, now: i64) {
        if (self.to - target).abs() < f64::EPSILON {
            return;
        }
        self.from = self.value(now);
        self.to = target;
        self.start = now;
    }

    pub fn jump(&mut self, value: f64) {
        self.from = value;
        self.to = value;
        self.start = 0;
    }

    pub fn set_timing(&mut self, millis: f64, ease: Ease) {
        self.micros = millis * 1000.0;
        self.ease = ease;
    }

    fn part(&self, now: i64) -> f64 {
        if self.micros <= 0.0 || (self.to - self.from).abs() < f64::EPSILON {
            return 1.0;
        }
        ((now - self.start) as f64 / self.micros).clamp(0.0, 1.0)
    }
}

pub struct Fade {
    from: Cell<Option<RGBA>>,
    start: Cell<i64>,
    target: Cell<Option<RGBA>>,
}

impl Fade {
    pub fn new() -> Self {
        Fade {
            from: Cell::new(None),
            start: Cell::new(0),
            target: Cell::new(None),
        }
    }

    pub fn retarget(&self, target: RGBA, now: i64, should_animate: bool) {
        if self.target.get() == Some(target) {
            return;
        }
        let current = self.value(now);
        self.target.set(Some(target));
        match current {
            Some(current) if should_animate => {
                self.from.set(Some(current));
                self.start.set(now);
            }
            _ => self.from.set(None),
        }
    }

    fn part(&self, now: i64) -> f64 {
        ((now - self.start.get()) as f64 / (FADE_MILLIS * 1000.0)).clamp(0.0, 1.0)
    }

    pub fn value(&self, now: i64) -> Option<RGBA> {
        let target = self.target.get()?;
        let Some(from) = self.from.get() else {
            return Some(target);
        };
        let eased_progress = EXPRESSIVE_EFFECTS.at(self.part(now)) as f32;
        Some(RGBA::new(
            from.red() + (target.red() - from.red()) * eased_progress,
            from.green() + (target.green() - from.green()) * eased_progress,
            from.blue() + (target.blue() - from.blue()) * eased_progress,
            from.alpha() + (target.alpha() - from.alpha()) * eased_progress,
        ))
    }

    pub fn is_running(&self, now: i64) -> bool {
        self.from.get().is_some() && self.part(now) < 1.0
    }
}

pub struct Fader {
    widget: glib::WeakRef<gtk4::Widget>,
    tween: Cell<Tween>,
    is_ticking: Cell<bool>,
}

impl Fader {
    pub fn new(
        widget: &impl IsA<gtk4::Widget>,
        is_shown: bool,
        millis: f64,
        ease: Ease,
    ) -> Rc<Self> {
        let weak_widget = glib::WeakRef::new();
        weak_widget.set(Some(widget.as_ref()));
        let opacity = if is_shown { 1.0 } else { 0.0 };
        widget.set_opacity(opacity);
        widget.set_visible(is_shown);
        Rc::new(Fader {
            widget: weak_widget,
            tween: Cell::new(Tween::new(opacity, millis, ease)),
            is_ticking: Cell::new(false),
        })
    }

    pub fn is_shown(&self) -> bool {
        self.tween.get().target() > 0.0
    }

    pub fn show(self: &Rc<Self>, should_show: bool) {
        let Some(widget) = self.widget.upgrade() else {
            return;
        };
        let target = if should_show { 1.0 } else { 0.0 };
        let mut tween = self.tween.get();
        if !widget.is_mapped() && !should_show {
            tween.jump(target);
            self.tween.set(tween);
            widget.set_opacity(target);
            widget.set_visible(false);
            return;
        }
        let now = widget
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        tween.retarget(target, now);
        self.tween.set(tween);
        widget.set_visible(true);
        if self.is_ticking.replace(true) {
            return;
        }
        let fader = self.clone();
        widget.add_tick_callback(move |widget, clock| {
            let tween = fader.tween.get();
            let now = clock.frame_time();
            let opacity = tween.value(now).clamp(0.0, 1.0);
            widget.set_opacity(opacity);
            if tween.is_running(now) {
                return glib::ControlFlow::Continue;
            }
            widget.set_visible(tween.target() > 0.0);
            fader.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

pub struct Motion {
    widget: glib::WeakRef<gtk4::Widget>,
    default_micros: f64,
    duration: Cell<f64>,
    ease: Ease,
    from: Cell<f64>,
    to: Cell<f64>,
    at: Cell<f64>,
    start_time: Cell<i64>,
    is_ticking: Cell<bool>,
}

impl Motion {
    pub fn new(widget: &impl IsA<gtk4::Widget>, start: f64, millis: f64, ease: Ease) -> Rc<Self> {
        let weak_widget = glib::WeakRef::new();
        weak_widget.set(Some(widget.as_ref()));
        Rc::new(Motion {
            widget: weak_widget,
            default_micros: millis * 1000.0,
            duration: Cell::new(millis * 1000.0),
            ease,
            from: Cell::new(start),
            to: Cell::new(start),
            at: Cell::new(start),
            start_time: Cell::new(0),
            is_ticking: Cell::new(false),
        })
    }

    pub fn get(&self) -> f64 {
        self.at.get()
    }

    pub fn is_running(&self) -> bool {
        self.is_ticking.get()
    }

    pub fn jump(&self, value: f64) {
        self.from.set(value);
        self.to.set(value);
        self.at.set(value);
    }

    pub fn to(self: &Rc<Self>, target: f64) {
        self.to_over(target, self.default_micros / 1000.0);
    }

    pub fn to_over(self: &Rc<Self>, target: f64, millis: f64) {
        self.duration.set(millis * 1000.0);
        if (self.to.get() - target).abs() < f64::EPSILON {
            return;
        }
        self.from.set(self.at.get());
        self.to.set(target);
        self.start_time.set(0);
        let Some(widget) = self.widget.upgrade() else {
            self.at.set(target);
            return;
        };
        if self.is_ticking.replace(true) {
            return;
        }

        let motion = self.clone();
        widget.add_tick_callback(move |widget, clock| {
            let now = clock.frame_time();
            if motion.start_time.get() == 0 {
                motion.start_time.set(now);
            }
            let part =
                ((now - motion.start_time.get()) as f64 / motion.duration.get()).clamp(0.0, 1.0);
            let eased_progress = motion.ease.at(part);
            let (from, to) = (motion.from.get(), motion.to.get());
            motion.at.set(from + (to - from) * eased_progress);
            widget.queue_draw();
            if part < 1.0 {
                return glib::ControlFlow::Continue;
            }
            motion.at.set(to);
            motion.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn bezier(x: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let curve = |a: f64, b: f64, t: f64| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * a + 3.0 * inverse * t * t * b + t * t * t
    };
    let mut low = 0.0;
    let mut high = 1.0;
    let mut t = x;
    for _ in 0..24 {
        let at = curve(x1, x2, t);
        if (at - x).abs() < 1e-5 {
            break;
        }
        if at < x {
            low = t;
        } else {
            high = t;
        }
        t = (low + high) / 2.0;
    }
    curve(y1, y2, t)
}

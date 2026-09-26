use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, STANDARD_DECEL, Tween};
use crate::ui::theme::{SharedTheme, rounding};

const MAX_VELOCITY: f64 = 3500.0;
const DECELERATION: f64 = 1500.0;
const WHEEL_DECELERATION: f64 = 15000.0;
const WHEEL_FLICK_DISTANCE: f64 = 72.0;
const MINIMUM_FLICK_VELOCITY: f64 = 75.0;
const RETAIN_GRAB_VELOCITY: f64 = 100.0;
const START_DRAG_DISTANCE: f64 = 10.0;
const FLICK_START_DISTANCE: f64 = 15.0;
const RELEASE_WINDOW: u32 = 100;
const SAMPLES: usize = 3;
const OVERSHOOT: f64 = 150.0;
const OVERSHOOT_FRICTION: f64 = 8.0;
const FIXUP_MILLIS: f64 = 400.0;
const SCROLL_MILLIS: f64 = 200.0;
const BAR_WIDTH: f64 = 4.0;
const BAR_PADDING: f64 = 2.0;
const BAR_INSET: f64 = rounding::NORMAL as f64;
const BAR_OPACITY: f64 = 0.5;
const BAR_FADE_MILLIS: f64 = 350.0;

const FASTER: &str = "/interactions/scrolling/fasterTouchpadScroll";
const THRESHOLD: &str = "/interactions/scrolling/mouseScrollDeltaThreshold";
const MOUSE_FACTOR: &str = "/interactions/scrolling/mouseScrollFactor";
const TOUCHPAD_FACTOR: &str = "/interactions/scrolling/touchpadScrollFactor";

#[derive(Clone, Copy)]
pub enum Motion {
    Still,
    Flick {
        start: i64,
        from: f64,
        velocity: f64,
        deceleration: f64,
    },
    Fixup {
        start: i64,
        from: f64,
        to: f64,
    },
    Ease(Tween),
}

impl Motion {
    fn at(self, now: i64) -> Option<(f64, bool)> {
        match self {
            Motion::Still => None,
            Motion::Flick {
                start,
                from,
                velocity,
                deceleration,
            } => {
                let end = velocity.abs() / deceleration;
                let time = ((now - start) as f64 / 1_000_000.0).clamp(0.0, end);
                let y =
                    from + velocity * time - velocity.signum() * deceleration * time * time / 2.0;
                Some((y, time >= end))
            }
            Motion::Fixup { start, from, to } => {
                let millis = (now - start) as f64 / 1000.0;
                let quarter = FIXUP_MILLIS / 4.0;
                let middle = to - (to - from) / 2.0;
                if millis >= FIXUP_MILLIS {
                    return Some((to, true));
                }
                if millis < quarter {
                    let part = (millis / quarter).max(0.0);
                    return Some((from + (middle - from) * part * part, false));
                }
                let part = (millis - quarter) / (FIXUP_MILLIS - quarter);
                let eased = 1.001 * (1.0 - 2f64.powf(-10.0 * part));
                Some((middle + (to - middle) * eased, false))
            }
            Motion::Ease(tween) => Some((tween.value(now), !tween.running(now))),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Drag {
    press: f64,
    start: Option<f64>,
    last: Option<(u32, f64)>,
    peak: f64,
    moving: bool,
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    pub struct Flickable {
        pub theme: RefCell<Option<SharedTheme>>,
        pub y: Cell<f64>,
        pub view: Cell<f64>,
        pub content: Cell<f64>,
        pub motion: Cell<Motion>,
        pub ticking: Cell<bool>,
        pub samples: RefCell<Vec<f64>>,
        pub velocity: Cell<f64>,
        pub wheel_time: Cell<Option<u32>>,
        pub drag: Cell<Option<Drag>>,
        pub touchpad: Cell<Option<Drag>>,
        pub hovered: Cell<bool>,
        pub bar: Cell<Option<f64>>,
        pub opacity: Cell<Tween>,
    }

    impl Default for Flickable {
        fn default() -> Self {
            Flickable {
                theme: RefCell::new(None),
                y: Cell::new(0.0),
                view: Cell::new(0.0),
                content: Cell::new(0.0),
                motion: Cell::new(Motion::Still),
                ticking: Cell::new(false),
                samples: RefCell::new(Vec::new()),
                velocity: Cell::new(0.0),
                wheel_time: Cell::new(None),
                drag: Cell::new(None),
                touchpad: Cell::new(None),
                hovered: Cell::new(false),
                bar: Cell::new(None),
                opacity: Cell::new(Tween::new(0.0, BAR_FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Flickable {
        const NAME: &'static str = "ProscenioFlickable";
        type Type = super::Flickable;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Flickable {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_overflow(gtk4::Overflow::Hidden);

            let wheel =
                gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
            wheel.connect_scroll(|controller, _, dy| {
                let Some(flickable) = controller.widget().and_downcast::<super::Flickable>() else {
                    return glib::Propagation::Proceed;
                };
                flickable.wheel(controller, dy)
            });
            wheel.connect_scroll_begin(|controller| {
                if let Some(flickable) = controller.widget().and_downcast::<super::Flickable>() {
                    flickable.touchpad_begin();
                }
            });
            wheel.connect_scroll_end(|controller| {
                if let Some(flickable) = controller.widget().and_downcast::<super::Flickable>() {
                    flickable.touchpad_end();
                }
            });
            obj.add_controller(wheel);

            let drag = gtk4::GestureDrag::new();
            drag.connect_drag_begin(|gesture, x, y| {
                if let Some(flickable) = gesture.widget().and_downcast::<super::Flickable>() {
                    flickable.press(gesture, x, y);
                }
            });
            drag.connect_drag_update(|gesture, _, dy| {
                if let Some(flickable) = gesture.widget().and_downcast::<super::Flickable>() {
                    flickable.follow(gesture, dy);
                }
            });
            drag.connect_drag_end(|gesture, _, dy| {
                if let Some(flickable) = gesture.widget().and_downcast::<super::Flickable>() {
                    flickable.release(gesture, dy);
                }
            });
            obj.add_controller(drag);

            let motion = gtk4::EventControllerMotion::new();
            motion.connect_motion(|controller, x, _| {
                if let Some(flickable) = controller.widget().and_downcast::<super::Flickable>() {
                    flickable.hover(x >= flickable.width() as f64 - BAR_WIDTH - BAR_PADDING * 2.0);
                }
            });
            motion.connect_leave(|controller| {
                if let Some(flickable) = controller.widget().and_downcast::<super::Flickable>() {
                    flickable.hover(false);
                }
            });
            obj.add_controller(motion);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Flickable {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let (minimum, natural, _, _) = child.measure(orientation, for_size);
            match orientation {
                gtk4::Orientation::Horizontal => (minimum, natural, -1, -1),
                _ => (0, natural, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let Some(child) = obj.first_child() else {
                return;
            };
            let content = child.measure(gtk4::Orientation::Vertical, width).1;
            self.view.set(height as f64);
            self.content.set(content as f64);
            let idle = matches!(self.motion.get(), Motion::Still)
                && self.drag.get().is_none()
                && self.touchpad.get().is_none();
            if idle {
                self.y.set(self.y.get().clamp(0.0, obj.max_y()));
            }
            let place = gsk::Transform::new()
                .translate(&graphene::Point::new(0.0, -self.y.get().round() as f32));
            child.allocate(width, content, -1, Some(place));
            obj.refresh_bar();
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            if let Some(child) = obj.first_child() {
                obj.snapshot_child(&child, snapshot);
            }
            let opacity = self.opacity.get().value(obj.now());
            let Some((size, position)) = obj.bar_area() else {
                return;
            };
            let Some(theme) = self.theme.borrow().clone() else {
                return;
            };
            if opacity <= 0.0 {
                return;
            }
            let available = self.view.get() - BAR_INSET * 2.0;
            let top = BAR_INSET + position.clamp(0.0, 1.0) * available;
            let bottom = BAR_INSET + (position + size).clamp(0.0, 1.0) * available;
            if bottom <= top {
                return;
            }
            let bounds = graphene::Rect::new(
                (obj.width() as f64 - BAR_PADDING - BAR_WIDTH) as f32,
                top as f32,
                BAR_WIDTH as f32,
                (bottom - top) as f32,
            );
            let colour = theme.borrow().colors.col_on_surface_variant;
            let colour = RGBA::new(
                colour.red(),
                colour.green(),
                colour.blue(),
                colour.alpha() * opacity as f32,
            );
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(
                bounds,
                (BAR_WIDTH / 2.0) as f32,
            ));
            snapshot.append_color(&colour, &bounds);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct Flickable(ObjectSubclass<imp::Flickable>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Flickable {
    pub fn new(theme: &SharedTheme) -> Self {
        let flickable: Self = glib::Object::new();
        flickable.imp().theme.replace(Some(theme.clone()));
        flickable
    }

    pub fn set_child(&self, child: &impl IsA<gtk4::Widget>) {
        while let Some(old) = self.first_child() {
            old.unparent();
        }
        child.set_parent(self);
    }

    fn now(&self) -> i64 {
        self.frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn max_y(&self) -> f64 {
        let imp = self.imp();
        (imp.content.get() - imp.view.get()).max(0.0)
    }

    fn run(&self) {
        let imp = self.imp();
        if imp.ticking.replace(true) {
            return;
        }
        self.add_tick_callback(|flickable, clock| {
            let now = clock.frame_time();
            flickable.advance(now);
            flickable.queue_allocate();
            flickable.queue_draw();
            let imp = flickable.imp();
            if !matches!(imp.motion.get(), Motion::Still) || imp.opacity.get().running(now) {
                return glib::ControlFlow::Continue;
            }
            imp.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn advance(&self, now: i64) {
        let imp = self.imp();
        let motion = imp.motion.get();
        let Some((y, done)) = motion.at(now) else {
            return;
        };
        imp.y.set(y);
        if !done {
            return;
        }
        match motion {
            Motion::Flick { .. } => self.fixup(now),
            _ => imp.motion.set(Motion::Still),
        }
    }

    fn start(&self, motion: Motion) {
        self.imp().motion.set(motion);
        self.run();
    }

    fn place(&self, y: f64) {
        let imp = self.imp();
        imp.motion.set(Motion::Still);
        imp.y.set(y);
        self.queue_allocate();
        self.queue_draw();
    }

    fn fixup(&self, now: i64) {
        let imp = self.imp();
        let y = imp.y.get();
        let max = self.max_y();
        let to = if y <= 0.0 || max <= 0.0 {
            0.0
        } else if y >= max {
            max
        } else {
            imp.motion.set(Motion::Still);
            return;
        };
        if y == to {
            imp.motion.set(Motion::Still);
            return;
        }
        self.start(Motion::Fixup {
            start: now,
            from: y,
            to,
        });
    }

    fn flick(&self, now: i64, velocity: f64, deceleration: f64) {
        let y = self.imp().y.get();
        let room = if velocity < 0.0 { y } else { self.max_y() - y };
        if room <= 0.0 || velocity == 0.0 {
            return self.fixup(now);
        }
        let velocity = velocity.clamp(-MAX_VELOCITY, MAX_VELOCITY);
        let squared = velocity * velocity;
        let target = (y + velocity.signum() * squared / (deceleration * 2.0)).round();
        let distance = (target - y).abs();
        if distance == 0.0 {
            return self.fixup(now);
        }
        self.start(Motion::Flick {
            start: now,
            from: y,
            velocity,
            deceleration: (squared / (distance * 2.0)).max(squared / (room * 2.0)),
        });
    }

    fn sample(&self, velocity: f64) {
        let imp = self.imp();
        let mut samples = imp.samples.borrow_mut();
        samples.push(velocity.clamp(-MAX_VELOCITY, MAX_VELOCITY));
        if samples.len() > SAMPLES {
            samples.remove(0);
        }
        imp.velocity
            .set(samples.iter().sum::<f64>() / samples.len() as f64);
    }

    fn reset_samples(&self) {
        let imp = self.imp();
        imp.samples.borrow_mut().clear();
        imp.velocity.set(0.0);
    }

    fn wheel(&self, controller: &gtk4::EventControllerScroll, dy: f64) -> glib::Propagation {
        let now = self.now();
        self.advance(now);
        let surface = controller.unit() == gtk4::gdk::ScrollUnit::Surface;
        let angle = wheel_angle(controller, dy);
        if config::value_bool(FASTER, false) {
            if angle != 0.0 {
                self.faster(angle, now);
            }
            return glib::Propagation::Stop;
        }
        if surface && self.imp().touchpad.get().is_some() {
            self.touchpad_follow(dy, controller.current_event_time());
            return glib::Propagation::Stop;
        }
        if angle == 0.0 {
            return glib::Propagation::Proceed;
        }
        self.wheel_flick(angle, controller.current_event_time(), now)
    }

    fn faster(&self, angle: f64, now: i64) {
        let imp = self.imp();
        let y = imp.y.get();
        let mut tween = match imp.motion.get() {
            Motion::Ease(tween) => tween,
            _ => Tween::new(y, SCROLL_MILLIS, STANDARD_DECEL),
        };
        let base = if tween.running(now) {
            tween.target()
        } else {
            y
        };
        tween.retarget(faster_target(base, angle, self.max_y()), now);
        self.start(Motion::Ease(tween));
    }

    fn wheel_flick(&self, angle: f64, time: u32, now: i64) -> glib::Propagation {
        let imp = self.imp();
        let elapsed = match imp.wheel_time.replace(Some(time)) {
            Some(last) => time.wrapping_sub(last) as f64 / 1000.0,
            None => 1.0,
        };
        if elapsed <= 0.0 {
            return glib::Propagation::Proceed;
        }
        let elapsed = if matches!(imp.motion.get(), Motion::Still) {
            120.0 / (WHEEL_DECELERATION * 2.0 * WHEEL_FLICK_DISTANCE).sqrt()
        } else {
            elapsed
        };
        let velocity = -angle / elapsed;
        if velocity * imp.velocity.get() < 0.0 {
            self.reset_samples();
        }
        self.sample(velocity);
        let y = imp.y.get();
        if (angle > 0.0 && y > 0.0) || (angle < 0.0 && y < self.max_y()) {
            self.flick(now, imp.velocity.get(), WHEEL_DECELERATION);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    fn begin(&self) -> Drag {
        let imp = self.imp();
        let now = self.now();
        self.advance(now);
        let motion = imp.motion.get();
        let moving = !matches!(motion, Motion::Still);
        if !matches!(motion, Motion::Fixup { .. }) {
            imp.motion.set(Motion::Still);
        }
        self.reset_samples();
        Drag {
            press: imp.y.get(),
            start: None,
            last: None,
            peak: 0.0,
            moving,
        }
    }

    fn touchpad_begin(&self) {
        if config::value_bool(FASTER, false) {
            return;
        }
        let drag = self.begin();
        self.imp().touchpad.set(Some(drag));
    }

    fn touchpad_follow(&self, dy: f64, time: u32) {
        let imp = self.imp();
        let Some(mut drag) = imp.touchpad.get() else {
            return;
        };
        let (offset, velocity) = match drag.last {
            Some((last, offset)) if time > last => {
                (offset + dy, dy / (time - last) as f64 * 1000.0)
            }
            Some((_, offset)) => (offset + dy, 0.0),
            None => (dy, 0.0),
        };
        drag.last = Some((time, offset));
        let start = *drag.start.get_or_insert(offset);
        let push = -velocity / OVERSHOOT_FRICTION;
        if (push > 0.0 && push > drag.peak) || (push < 0.0 && push < drag.peak) {
            drag.peak = push.clamp(-MAX_VELOCITY, MAX_VELOCITY);
        }
        imp.touchpad.set(Some(drag));

        let scale = self.scale_factor() as f64;
        let bound = -self.max_y();
        let mut moved = -(drag.press + offset - start);
        if moved > 0.0 {
            let overshoot = moved * drag.peak / MAX_VELOCITY / OVERSHOOT_FRICTION;
            moved = OVERSHOOT * scale * (overshoot / OVERSHOOT / scale).atan();
        } else if moved < bound {
            let overshoot = (moved - bound) * drag.peak / MAX_VELOCITY / OVERSHOOT_FRICTION;
            moved = bound - OVERSHOOT * scale * (overshoot / OVERSHOOT / scale).atan();
        }
        self.place(-moved);
    }

    fn touchpad_end(&self) {
        if self.imp().touchpad.take().is_none() {
            return;
        }
        self.fixup(self.now());
    }

    fn press(&self, gesture: &gtk4::GestureDrag, x: f64, y: f64) {
        let imp = self.imp();
        let strip = self.width() as f64 - BAR_WIDTH - BAR_PADDING * 2.0;
        if x >= strip {
            if let Some((size, position)) = self.bar_area() {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                let offset = self.bar_position(y) - position;
                let offset = if offset < 0.0 || offset > size {
                    size / 2.0
                } else {
                    offset
                };
                imp.bar.set(Some(offset));
                self.refresh_bar();
                return;
            }
        }
        let speed = match imp.motion.get() {
            Motion::Flick {
                start,
                velocity,
                deceleration,
                ..
            } => velocity.abs() - deceleration * (self.now() - start) as f64 / 1_000_000.0,
            _ => 0.0,
        };
        let mut drag = self.begin();
        drag.moving = speed > RETAIN_GRAB_VELOCITY;
        drag.last = Some((gesture.current_event_time(), 0.0));
        imp.drag.set(Some(drag));
    }

    fn follow(&self, gesture: &gtk4::GestureDrag, dy: f64) {
        let imp = self.imp();
        if let Some(offset) = imp.bar.get() {
            let (_, y) = gesture.start_point().unwrap_or_default();
            self.scroll_to_bar(y + dy, offset);
            return;
        }
        let Some(mut drag) = imp.drag.get() else {
            return;
        };
        let time = gesture.current_event_time();
        if let Some((last, offset)) = drag.last {
            if time > last {
                self.sample(-(dy - offset) / (time - last) as f64 * 1000.0);
            }
        }
        drag.last = Some((time, dy));
        if drag.start.is_none() {
            if !drag.moving && dy.abs() <= START_DRAG_DISTANCE {
                imp.drag.set(Some(drag));
                return;
            }
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            drag.start = Some(dy);
        }
        imp.drag.set(Some(drag));
        let max = self.max_y();
        let mut y = drag.press - (dy - drag.start.unwrap_or(dy));
        if y < 0.0 {
            y /= 2.0;
        } else if y > max {
            y = max + (y - max) / 2.0;
        }
        self.place(y);
    }

    fn release(&self, gesture: &gtk4::GestureDrag, dy: f64) {
        let imp = self.imp();
        let now = self.now();
        if let Some(offset) = imp.bar.take() {
            let (_, y) = gesture.start_point().unwrap_or_default();
            self.scroll_to_bar(y + dy, offset);
            self.refresh_bar();
            return;
        }
        let Some(drag) = imp.drag.take() else {
            return;
        };
        if drag.start.is_none() {
            return self.fixup(now);
        }
        let time = gesture.current_event_time();
        let recent = drag
            .last
            .is_some_and(|(last, _)| time.wrapping_sub(last) < RELEASE_WINDOW);
        let mut velocity = if recent { imp.velocity.get() } else { 0.0 };
        let y = imp.y.get();
        if (y <= 0.0 && velocity < 0.0) || (y >= self.max_y() && velocity > 0.0) {
            velocity /= 2.0;
        }
        if velocity.abs() > MINIMUM_FLICK_VELOCITY && dy.abs() > FLICK_START_DISTANCE {
            self.flick(now, velocity, DECELERATION);
        } else {
            self.fixup(now);
        }
    }

    fn bar_area(&self) -> Option<(f64, f64)> {
        let imp = self.imp();
        let content = imp.content.get();
        if content <= 0.0 {
            return None;
        }
        let size = imp.view.get() / content;
        (size < 1.0).then(|| (size, imp.y.get() / content))
    }

    fn bar_position(&self, y: f64) -> f64 {
        (y - BAR_INSET) / (self.imp().view.get() - BAR_INSET * 2.0).max(1.0)
    }

    fn scroll_to_bar(&self, y: f64, offset: f64) {
        let Some((size, _)) = self.bar_area() else {
            return;
        };
        let position = (self.bar_position(y) - offset).clamp(0.0, 1.0 - size);
        self.place(position * self.imp().content.get());
    }

    fn hover(&self, hovered: bool) {
        if self.imp().hovered.replace(hovered) != hovered {
            self.refresh_bar();
        }
    }

    fn refresh_bar(&self) {
        let imp = self.imp();
        let active = imp.hovered.get() || imp.bar.get().is_some();
        let target = if active && self.bar_area().is_some() {
            BAR_OPACITY
        } else {
            0.0
        };
        let mut opacity = imp.opacity.get();
        if opacity.target() == target {
            return;
        }
        if self.is_mapped() {
            opacity.retarget(target, self.now());
        } else {
            opacity.jump(target);
        }
        imp.opacity.set(opacity);
        self.run();
    }
}

fn wheel_angle(controller: &gtk4::EventControllerScroll, dy: f64) -> f64 {
    if controller.unit() == gtk4::gdk::ScrollUnit::Surface {
        (-dy * 12.0).round()
    } else {
        (-dy * 120.0).round()
    }
}

fn faster_target(base: f64, angle: f64, max: f64) -> f64 {
    let threshold = config::value_f64(THRESHOLD, 120.0);
    let factor = if angle.abs() >= threshold {
        config::value_f64(MOUSE_FACTOR, 120.0)
    } else {
        config::value_f64(TOUCHPAD_FACTOR, 450.0)
    };
    (base - angle / threshold * factor).clamp(0.0, max)
}

pub fn follow_scroll_settings(scroll: &gtk4::ScrolledWindow) {
    let tween = Rc::new(Cell::new(Tween::new(0.0, SCROLL_MILLIS, STANDARD_DECEL)));
    let ticking = Rc::new(Cell::new(false));
    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll(move |controller, _, dy| {
        if !config::value_bool(FASTER, false) {
            return glib::Propagation::Proceed;
        }
        let Some(scroll) = controller.widget().and_downcast::<gtk4::ScrolledWindow>() else {
            return glib::Propagation::Proceed;
        };
        let angle = wheel_angle(controller, dy);
        if angle == 0.0 {
            return glib::Propagation::Stop;
        }
        let adjustment = scroll.vadjustment();
        let now = scroll
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let mut next = tween.get();
        let base = if ticking.get() && next.running(now) {
            next.target()
        } else {
            next.jump(adjustment.value());
            adjustment.value()
        };
        let max = (adjustment.upper() - adjustment.page_size()).max(0.0);
        next.retarget(faster_target(base, angle, max), now);
        tween.set(next);
        if !ticking.replace(true) {
            let tween = tween.clone();
            let ticking = ticking.clone();
            scroll.add_tick_callback(move |scroll, clock| {
                let now = clock.frame_time();
                let current = tween.get();
                scroll.vadjustment().set_value(current.value(now));
                if current.running(now) {
                    return glib::ControlFlow::Continue;
                }
                ticking.set(false);
                glib::ControlFlow::Break
            });
        }
        glib::Propagation::Stop
    });
    scroll.add_controller(wheel);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_notch_flicks_three_lines_and_a_fixup_lands_on_its_bound() {
        let elapsed = 120.0 / (WHEEL_DECELERATION * 2.0 * WHEEL_FLICK_DISTANCE).sqrt();
        let flick = Motion::Flick {
            start: 0,
            from: 0.0,
            velocity: 120.0 / elapsed,
            deceleration: WHEEL_DECELERATION,
        };
        let (y, done) = flick.at(1_000_000).unwrap();
        assert!(done);
        assert!((y - WHEEL_FLICK_DISTANCE).abs() < 1e-9);

        let fixup = Motion::Fixup {
            start: 0,
            from: -40.0,
            to: 0.0,
        };
        assert_eq!(fixup.at(100_000), Some((-20.0, false)));
        assert_eq!(fixup.at(400_000), Some((0.0, true)));
    }
}

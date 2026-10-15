mod renderer;
pub(super) mod state;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};

use crate::ui::theme::SharedTheme;
use state::{Fades, Look, Moment, progress_of, steer, steer_clockwise};

pub const SIZE: f64 = 230.0;
const TURN_MICROS: f64 = 30_000_000.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Cookie {
        pub theme: RefCell<Option<SharedTheme>>,
        pub look: RefCell<Option<Look>>,
        pub moment: RefCell<Moment>,
        pub fades: Cell<Fades>,
        pub turn: Cell<f64>,
        pub turn_origin: Cell<Option<(i64, f64)>>,
        pub is_ticking: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Cookie {
        const NAME: &'static str = "ProscenioCookieClock";
        type Type = super::Cookie;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Cookie {}

    impl WidgetImpl for Cookie {
        fn measure(&self, _orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (SIZE as i32, SIZE as i32, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            self.obj().draw(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct Cookie(ObjectSubclass<imp::Cookie>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Cookie {
    pub fn new(theme: &SharedTheme) -> Self {
        let cookie: Self = glib::Object::new();
        cookie.imp().theme.replace(Some(theme.clone()));
        cookie
    }

    fn now(&self) -> i64 {
        self.frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    pub fn set_look(&self, look: Look) {
        let imp = self.imp();
        let should_animate = imp.look.borrow().is_some() && self.is_mapped();
        let now = self.now();
        let mut fades = imp.fades.get();
        steer(
            &mut fades.dots,
            progress_of(look.dial == "dots"),
            now,
            should_animate,
        );
        steer(
            &mut fades.numbers,
            progress_of(look.dial == "numbers"),
            now,
            should_animate,
        );
        steer(
            &mut fades.lines,
            progress_of(look.dial == "full"),
            now,
            should_animate,
        );
        steer(
            &mut fades.hour_marks,
            progress_of(look.shows_hour_marks()),
            now,
            should_animate,
        );
        steer(
            &mut fades.column,
            progress_of(look.shows_column()),
            now,
            should_animate,
        );
        steer(
            &mut fades.column_scale_progress,
            progress_of(look.shows_column()),
            now,
            should_animate,
        );
        steer(
            &mut fades.column_small,
            progress_of(look.shows_hour_marks()),
            now,
            should_animate,
        );
        steer(
            &mut fades.minute,
            progress_of(look.minute_hand != "hide"),
            now,
            should_animate,
        );
        steer(
            &mut fades.hour,
            progress_of(look.hour_hand != "hide"),
            now,
            should_animate,
        );
        let shows_second = look.seconds && look.second_hand != "hide";
        steer(
            &mut fades.second,
            progress_of(shows_second),
            now,
            should_animate,
        );
        let is_classic = look.second_hand == "classic";
        steer(
            &mut fades.classic_dot,
            progress_of(is_classic),
            now,
            should_animate,
        );
        steer(
            &mut fades.classic_dot_width,
            if is_classic { 14.0 } else { 0.0 },
            now,
            should_animate,
        );
        steer(
            &mut fades.center,
            progress_of(look.minute_hand != "bold"),
            now,
            should_animate,
        );
        let has_border = look.date == "border";
        steer(
            &mut fades.border,
            progress_of(has_border),
            now,
            should_animate,
        );
        steer(
            &mut fades.border_radius,
            if has_border { 90.0 } else { 0.0 },
            now,
            should_animate,
        );
        steer(
            &mut fades.rect,
            progress_of(look.date == "rect"),
            now,
            should_animate,
        );
        steer(
            &mut fades.bubble,
            progress_of(look.date == "bubble"),
            now,
            should_animate,
        );
        steer(
            &mut fades.hour_fill,
            progress_of(look.hour_hand != "hollow"),
            now,
            should_animate,
        );
        steer(
            &mut fades.hour_classic,
            progress_of(look.hour_hand == "classic"),
            now,
            should_animate,
        );
        let minute_width = match look.minute_hand.as_str() {
            "bold" => 20.0,
            "medium" => 12.0,
            _ => 5.0,
        };
        steer(&mut fades.minute_width, minute_width, now, should_animate);
        steer(
            &mut fades.minute_classic,
            progress_of(look.minute_hand == "classic"),
            now,
            should_animate,
        );
        let is_dot = look.second_hand == "dot";
        steer(
            &mut fades.second_width,
            if is_dot { 20.0 } else { 95.0 },
            now,
            should_animate,
        );
        steer(
            &mut fades.second_height,
            if is_dot { 20.0 } else { 2.0 },
            now,
            should_animate,
        );
        imp.fades.set(fades);
        if !look.rotate {
            imp.turn.set(self.turn(now));
            imp.turn_origin.set(None);
        } else if imp.turn_origin.get().is_none() {
            imp.turn_origin.set(Some((now, imp.turn.get())));
        }
        imp.look.replace(Some(look));
        self.run();
    }

    pub fn set_moment(&self, moment: Moment) {
        let imp = self.imp();
        if *imp.moment.borrow() == moment {
            return;
        }
        let should_animate = !imp.moment.borrow().numbers.is_empty()
            && moment.can_sweep_from(&imp.moment.borrow())
            && self.is_mapped();
        let should_rotate = imp.look.borrow().as_ref().is_some_and(|look| look.rotate);
        let now = self.now();
        let mut fades = imp.fades.get();
        let hour = -90.0 + 30.0 * (moment.hour as f64 + moment.minute as f64 / 60.0);
        steer_clockwise(&mut fades.hour_angle, hour, now, should_animate);
        steer_clockwise(
            &mut fades.minute_angle,
            -90.0 + 6.0 * moment.minute as f64,
            now,
            should_animate,
        );
        let second = 6.0 * moment.second as f64 + 90.0;
        steer_clockwise(
            &mut fades.second_angle,
            second,
            now,
            should_animate && should_rotate,
        );
        imp.fades.set(fades);
        imp.moment.replace(moment);
        self.run();
    }

    fn turn(&self, now: i64) -> f64 {
        let imp = self.imp();
        match imp.turn_origin.get() {
            Some((start, from)) => {
                let spent = (now - start) as f64 / TURN_MICROS;
                (from - 360.0 * spent).rem_euclid(360.0)
            }
            None => imp.turn.get(),
        }
    }

    fn is_running(&self, now: i64) -> bool {
        let imp = self.imp();
        imp.turn_origin.get().is_some()
            || imp
                .fades
                .get()
                .all()
                .iter()
                .any(|tween| tween.is_running(now))
    }

    fn run(&self) {
        self.queue_draw();
        let imp = self.imp();
        if imp.is_ticking.replace(true) {
            return;
        }
        self.add_tick_callback(|cookie, clock| {
            cookie.queue_draw();
            if cookie.is_running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            cookie.imp().is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

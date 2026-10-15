use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use crate::ui::anim::{EMPHASIZED_DECEL, Tween};
use crate::ui::widgets::input::slider;

const WIDTH: i32 = 120;
const HEIGHT: i32 = 4;
const GAP: f64 = 4.0;
const WAVE_SCALE: f32 = 6.0;
const VALUE_MILLIS: f64 = 400.0;

#[derive(Clone, Copy)]
pub struct Colors {
    pub highlight: RGBA,
    pub track: RGBA,
}

mod imp {
    use super::*;
    use std::cell::Cell;

    pub struct ProgressBar {
        pub value: Cell<Tween>,
        pub is_wavy: Cell<bool>,
        pub is_wave_moving: Cell<bool>,
        pub colors: Cell<Colors>,
        pub is_ticking: Cell<bool>,
        pub natural_width: Cell<i32>,
    }

    impl Default for ProgressBar {
        fn default() -> Self {
            let clear = RGBA::new(0.0, 0.0, 0.0, 0.0);
            ProgressBar {
                value: Cell::new(Tween::new(0.0, VALUE_MILLIS, EMPHASIZED_DECEL)),
                is_wavy: Cell::new(false),
                is_wave_moving: Cell::new(true),
                colors: Cell::new(Colors {
                    highlight: clear,
                    track: clear,
                }),
                is_ticking: Cell::new(false),
                natural_width: Cell::new(WIDTH),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProgressBar {
        const NAME: &'static str = "ProscenioProgressBar";
        type Type = super::ProgressBar;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for ProgressBar {}

    impl WidgetImpl for ProgressBar {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk4::Orientation::Horizontal => (0, self.natural_width.get(), -1, -1),
                _ => (HEIGHT, HEIGHT, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            let (width, height) = (obj.width() as f32, obj.height() as f32);
            let reach = height * WAVE_SCALE;
            let top = (height - reach) / 2.0;
            let bounds = graphene::Rect::new(0.0, top, width, reach);
            let cr = snapshot.append_cairo(&bounds);
            let now = self.now();
            let part = self.value.get().value(now).clamp(0.0, 1.0);
            let colors = self.colors.get();
            let (width, height) = (width as f64, height as f64);

            if self.is_wavy.get() {
                cr.translate(0.0, top as f64);
                slider::draw_wave(
                    &cr,
                    colors.highlight,
                    0.0,
                    width * part,
                    reach as f64,
                    width,
                    height,
                );
                cr.translate(0.0, -top as f64);
            } else {
                fill_pill(&cr, colors.highlight, 0.0, 0.0, width * part, height);
            }
            let rest = (1.0 - part) * width - GAP;
            fill_pill(&cr, colors.track, width - rest, 0.0, rest, height);
            fill_pill(&cr, colors.highlight, width - GAP, 0.0, GAP, GAP);
        }
    }

    impl ProgressBar {
        pub fn now(&self) -> i64 {
            self.obj()
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time)
        }
    }

    fn fill_pill(cr: &gtk4::cairo::Context, color: RGBA, x: f64, y: f64, width: f64, height: f64) {
        if width <= 0.0 {
            return;
        }
        let radius = (height / 2.0).min(width / 2.0);
        cr.set_source_rgba(
            color.red() as f64,
            color.green() as f64,
            color.blue() as f64,
            color.alpha() as f64,
        );
        cr.new_sub_path();
        cr.arc(
            x + width - radius,
            y + radius,
            radius,
            -std::f64::consts::FRAC_PI_2,
            0.0,
        );
        cr.arc(
            x + width - radius,
            y + height - radius,
            radius,
            0.0,
            std::f64::consts::FRAC_PI_2,
        );
        cr.arc(
            x + radius,
            y + height - radius,
            radius,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
        );
        cr.arc(
            x + radius,
            y + radius,
            radius,
            std::f64::consts::PI,
            1.5 * std::f64::consts::PI,
        );
        cr.close_path();
        let _ = cr.fill();
    }
}

glib::wrapper! {
    pub struct ProgressBar(ObjectSubclass<imp::ProgressBar>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl ProgressBar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_colors(&self, colors: Colors) {
        self.imp().colors.set(colors);
        self.queue_draw();
    }

    pub fn set_value(&self, value: f64) {
        let imp = self.imp();
        let mut tween = imp.value.get();
        if self.is_mapped() {
            tween.retarget(value, imp.now());
        } else {
            tween.jump(value);
        }
        imp.value.set(tween);
        self.run();
    }

    pub fn set_natural_width(&self, width: i32) {
        self.imp().natural_width.set(width);
        self.queue_resize();
    }

    pub fn set_wavy(&self, is_wavy: bool) {
        if self.imp().is_wavy.replace(is_wavy) != is_wavy {
            self.run();
        }
    }

    pub fn set_wave_moving(&self, is_moving: bool) {
        if self.imp().is_wave_moving.replace(is_moving) != is_moving {
            self.run();
        }
    }

    fn run(&self) {
        self.queue_draw();
        let imp = self.imp();
        if imp.is_ticking.replace(true) {
            return;
        }
        self.add_tick_callback(|bar, clock| {
            bar.queue_draw();
            let imp = bar.imp();
            let is_waving = imp.is_wavy.get() && imp.is_wave_moving.get();
            if is_waving || imp.value.get().is_running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            imp.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

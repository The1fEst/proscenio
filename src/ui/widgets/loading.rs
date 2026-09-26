use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::ui::anim::{EXPRESSIVE_EFFECTS, Ease};
use crate::ui::morph::Morph;
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::SharedTheme;

const SHAPES: [Shape; 7] = [
    Shape::SoftBurst,
    Shape::Cookie9Sided,
    Shape::Pentagon,
    Shape::Pill,
    Shape::Sunny,
    Shape::Cookie4Sided,
    Shape::Oval,
];
const BASE_SHAPE: f64 = 0.7;
const LEAP_ZOOM: f64 = 1.2;
const TURN_MICROS: f64 = 12_000_000.0;
const LEAP_EVERY_MICROS: i64 = 800_000;
const LEAP_TURN: f64 = 90.0;
const LEAP_TURN_MICROS: f64 = 350_000.0;
const LEAP_ZOOM_MICROS: f64 = 750_000.0;
const MORPH_MICROS: f64 = 200_000.0;
const STANDARD: Ease = Ease::Bezier(0.2, 0.0, 0.0, 1.0);

struct Motion {
    started: i64,
    leaps: i64,
    shape: usize,
    morph: Morph,
    leap_from: f64,
    leap_started: i64,
}

pub struct LoadingIndicator {
    pub area: gtk4::DrawingArea,
    motion: RefCell<Option<Motion>>,
    ticking: Cell<bool>,
}

impl LoadingIndicator {
    pub fn new(theme: &SharedTheme, size: i32) -> Rc<Self> {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(size);
        area.set_content_height(size);
        area.set_valign(gtk4::Align::Center);
        let indicator = Rc::new(LoadingIndicator {
            area,
            motion: RefCell::new(None),
            ticking: Cell::new(false),
        });
        indicator.area.set_draw_func({
            let theme = theme.clone();
            let indicator = Rc::downgrade(&indicator);
            move |area, cr, width, height| {
                let Some(indicator) = indicator.upgrade() else {
                    return;
                };
                let now = area
                    .frame_clock()
                    .map(|clock| clock.frame_time())
                    .unwrap_or_else(glib::monotonic_time);
                indicator.draw(&theme, cr, width as f64, height as f64, now);
            }
        });
        indicator
    }

    pub fn set_loading(self: &Rc<Self>, loading: bool) {
        if !loading {
            self.motion.replace(None);
            return;
        }
        if self.motion.borrow().is_some() {
            return;
        }
        let now = self
            .area
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let first = shapes::polygon(SHAPES[0]);
        self.motion.replace(Some(Motion {
            started: now,
            leaps: 0,
            shape: 0,
            morph: Morph::new(&first, &first),
            leap_from: 0.0,
            leap_started: i64::MIN,
        }));
        if self.ticking.replace(true) {
            return;
        }
        let indicator = Rc::downgrade(self);
        self.area.add_tick_callback(move |area, clock| {
            let Some(indicator) = indicator.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !indicator.advance(clock.frame_time()) {
                indicator.ticking.set(false);
                return glib::ControlFlow::Break;
            }
            area.queue_draw();
            glib::ControlFlow::Continue
        });
    }

    fn advance(&self, now: i64) -> bool {
        let mut motion = self.motion.borrow_mut();
        let Some(motion) = motion.as_mut() else {
            return false;
        };
        let due = (now - motion.started) / LEAP_EVERY_MICROS;
        if due > motion.leaps {
            motion.leaps = due;
            let previous = shapes::polygon(SHAPES[motion.shape]);
            motion.shape = (motion.shape + 1) % SHAPES.len();
            let next = shapes::polygon(SHAPES[motion.shape]);
            motion.morph = Morph::new(&previous, &next);
            motion.leap_from = leap_rotation(motion, now);
            motion.leap_started = now;
        }
        true
    }

    fn draw(
        &self,
        theme: &SharedTheme,
        cr: &gtk4::cairo::Context,
        width: f64,
        height: f64,
        now: i64,
    ) {
        let theme = theme.borrow();
        let size = width.min(height);
        let background = theme.colors.col_primary_container;
        cr.set_source_rgba(
            background.red() as f64,
            background.green() as f64,
            background.blue() as f64,
            background.alpha() as f64,
        );
        cr.arc(width / 2.0, height / 2.0, size / 2.0, 0.0, 2.0 * PI);
        let _ = cr.fill();

        let motion = self.motion.borrow();
        let Some(motion) = motion.as_ref() else {
            return;
        };
        let since_leap = (now - motion.leap_started) as f64;
        let zoom_progress = if motion.leap_started == i64::MIN {
            0.0
        } else {
            STANDARD.at((since_leap / LEAP_ZOOM_MICROS).clamp(0.0, 1.0))
        };
        let base = size * BASE_SHAPE;
        let zoom = base * LEAP_ZOOM - base;
        let first_half = zoom_progress.min(0.5) * 2.0;
        let second_half = (zoom_progress - 0.5).max(0.0) * 2.0;
        let shape_size = base + zoom * first_half - zoom * second_half;
        let morph_progress = if motion.leap_started == i64::MIN {
            1.0
        } else {
            EXPRESSIVE_EFFECTS.at((since_leap / MORPH_MICROS).clamp(0.0, 1.0))
        };
        let continuous = (now - motion.started) as f64 / TURN_MICROS * 360.0 % 360.0;
        let rotation = continuous + leap_rotation(motion, now);

        let colour = theme.colors.col_on_primary_container;
        cr.save().ok();
        cr.translate(width / 2.0, height / 2.0);
        cr.rotate(rotation.to_radians());
        cr.set_source_rgba(
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
            colour.alpha() as f64,
        );
        shapes::trace(
            &motion.morph.cubics(morph_progress),
            cr,
            -shape_size / 2.0,
            -shape_size / 2.0,
            shape_size,
        );
        let _ = cr.fill();
        cr.restore().ok();
    }
}

fn leap_rotation(motion: &Motion, now: i64) -> f64 {
    if motion.leap_started == i64::MIN {
        return motion.leap_from;
    }
    let part = ((now - motion.leap_started) as f64 / LEAP_TURN_MICROS).clamp(0.0, 1.0);
    (motion.leap_from + LEAP_TURN * Ease::InOutQuad.at(part)) % 360.0
}

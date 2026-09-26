use gtk4::cairo;
use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;

use crate::ui::anim::{EMPHASIZED, EXPRESSIVE_EFFECTS, Ease, Tween};
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::{SharedTheme, mix, rounding};
use crate::ui::widgets::text::{self, Family};

pub const SIZE: f64 = 230.0;
const FADE_MILLIS: f64 = 200.0;
const RESIZE_MILLIS: f64 = 300.0;
const HAND_MILLIS: f64 = 300.0;
const SECOND_MILLIS: f64 = 1000.0;
const TURN_MICROS: f64 = 30_000_000.0;
const SHADOW_RADIUS: f64 = 8.0;
const SINE_POINTS: usize = 360;
const DATE_SQUARE: f64 = 64.0;

#[derive(Clone, PartialEq)]
pub struct Look {
    pub sides: i64,
    pub dial: String,
    pub hour_hand: String,
    pub minute_hand: String,
    pub second_hand: String,
    pub date: String,
    pub time_indicators: bool,
    pub hour_marks: bool,
    pub rotate: bool,
    pub sine: bool,
    pub seconds: bool,
}

impl Look {
    pub fn read(root: &Value) -> Self {
        let cookie = |key: &str| root.pointer(&format!("/background/widgets/clock/cookie/{key}"));
        let text = |key: &str, default: &str| {
            cookie(key)
                .and_then(Value::as_str)
                .unwrap_or(default)
                .to_owned()
        };
        let flag =
            |key: &str, default: bool| cookie(key).and_then(Value::as_bool).unwrap_or(default);
        Look {
            sides: cookie("sides").and_then(Value::as_i64).unwrap_or(14),
            dial: text("dialNumberStyle", "full"),
            hour_hand: text("hourHandStyle", "fill"),
            minute_hand: text("minuteHandStyle", "medium"),
            second_hand: text("secondHandStyle", "dot"),
            date: text("dateStyle", "bubble"),
            time_indicators: flag("timeIndicators", true),
            hour_marks: flag("hourMarks", false),
            rotate: flag("constantlyRotate", false),
            sine: flag("useSineCookie", false),
            seconds: root
                .pointer("/time/secondPrecision")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    fn hour_marks_shown(&self) -> bool {
        self.hour_marks && (self.dial == "dots" || self.dial == "full")
    }

    fn column_shown(&self) -> bool {
        self.time_indicators && self.dial != "numbers"
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct Moment {
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub numbers: Vec<String>,
    pub day: String,
    pub day_padded: String,
    pub month: String,
    pub weekday_day: String,
}

#[derive(Clone, Copy)]
pub struct Fades {
    dots: Tween,
    numbers: Tween,
    lines: Tween,
    hour_marks: Tween,
    column: Tween,
    column_shown: Tween,
    column_small: Tween,
    minute: Tween,
    hour: Tween,
    second: Tween,
    classic_dot: Tween,
    classic_dot_width: Tween,
    centre: Tween,
    border: Tween,
    border_radius: Tween,
    rect: Tween,
    bubble: Tween,
    hour_fill: Tween,
    hour_classic: Tween,
    minute_width: Tween,
    minute_classic: Tween,
    second_width: Tween,
    second_height: Tween,
    hour_angle: Tween,
    minute_angle: Tween,
    second_angle: Tween,
}

impl Default for Fades {
    fn default() -> Self {
        let fade = Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS);
        let resize = Tween::new(0.0, RESIZE_MILLIS, EMPHASIZED);
        let hand = Tween::new(0.0, HAND_MILLIS, EMPHASIZED);
        Fades {
            dots: fade,
            numbers: fade,
            lines: fade,
            hour_marks: fade,
            column: fade,
            column_shown: resize,
            column_small: resize,
            minute: fade,
            hour: fade,
            second: fade,
            classic_dot: fade,
            classic_dot_width: resize,
            centre: fade,
            border: fade,
            border_radius: resize,
            rect: fade,
            bubble: fade,
            hour_fill: resize,
            hour_classic: resize,
            minute_width: resize,
            minute_classic: resize,
            second_width: resize,
            second_height: resize,
            hour_angle: hand,
            minute_angle: hand,
            second_angle: Tween::new(0.0, SECOND_MILLIS, Ease::InOutQuad),
        }
    }
}

impl Fades {
    fn all(&self) -> [&Tween; 26] {
        [
            &self.dots,
            &self.numbers,
            &self.lines,
            &self.hour_marks,
            &self.column,
            &self.column_shown,
            &self.column_small,
            &self.minute,
            &self.hour,
            &self.second,
            &self.classic_dot,
            &self.classic_dot_width,
            &self.centre,
            &self.border,
            &self.border_radius,
            &self.rect,
            &self.bubble,
            &self.hour_fill,
            &self.hour_classic,
            &self.minute_width,
            &self.minute_classic,
            &self.second_width,
            &self.second_height,
            &self.hour_angle,
            &self.minute_angle,
            &self.second_angle,
        ]
    }
}

fn steer(tween: &mut Tween, target: f64, now: i64, animate: bool) {
    if animate {
        tween.retarget(target, now);
    } else {
        tween.jump(target);
    }
}

fn clockwise(tween: &mut Tween, target: f64, now: i64, animate: bool) {
    let mut target = target;
    let from = tween.target();
    while target < from {
        target += 360.0;
    }
    while target - from >= 360.0 {
        target -= 360.0;
    }
    steer(tween, target, now, animate);
}

fn shown(flag: bool) -> f64 {
    if flag { 1.0 } else { 0.0 }
}

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
        pub ticking: Cell<bool>,
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
        let animate = imp.look.borrow().is_some() && self.is_mapped();
        let now = self.now();
        let mut fades = imp.fades.get();
        steer(&mut fades.dots, shown(look.dial == "dots"), now, animate);
        steer(
            &mut fades.numbers,
            shown(look.dial == "numbers"),
            now,
            animate,
        );
        steer(&mut fades.lines, shown(look.dial == "full"), now, animate);
        steer(
            &mut fades.hour_marks,
            shown(look.hour_marks_shown()),
            now,
            animate,
        );
        steer(&mut fades.column, shown(look.column_shown()), now, animate);
        steer(
            &mut fades.column_shown,
            shown(look.column_shown()),
            now,
            animate,
        );
        steer(
            &mut fades.column_small,
            shown(look.hour_marks_shown()),
            now,
            animate,
        );
        steer(
            &mut fades.minute,
            shown(look.minute_hand != "hide"),
            now,
            animate,
        );
        steer(
            &mut fades.hour,
            shown(look.hour_hand != "hide"),
            now,
            animate,
        );
        let second = look.seconds && look.second_hand != "hide";
        steer(&mut fades.second, shown(second), now, animate);
        let classic = look.second_hand == "classic";
        steer(&mut fades.classic_dot, shown(classic), now, animate);
        steer(
            &mut fades.classic_dot_width,
            if classic { 14.0 } else { 0.0 },
            now,
            animate,
        );
        steer(
            &mut fades.centre,
            shown(look.minute_hand != "bold"),
            now,
            animate,
        );
        let border = look.date == "border";
        steer(&mut fades.border, shown(border), now, animate);
        steer(
            &mut fades.border_radius,
            if border { 90.0 } else { 0.0 },
            now,
            animate,
        );
        steer(&mut fades.rect, shown(look.date == "rect"), now, animate);
        steer(
            &mut fades.bubble,
            shown(look.date == "bubble"),
            now,
            animate,
        );
        steer(
            &mut fades.hour_fill,
            shown(look.hour_hand != "hollow"),
            now,
            animate,
        );
        steer(
            &mut fades.hour_classic,
            shown(look.hour_hand == "classic"),
            now,
            animate,
        );
        let minute_width = match look.minute_hand.as_str() {
            "bold" => 20.0,
            "medium" => 12.0,
            _ => 5.0,
        };
        steer(&mut fades.minute_width, minute_width, now, animate);
        steer(
            &mut fades.minute_classic,
            shown(look.minute_hand == "classic"),
            now,
            animate,
        );
        let dot = look.second_hand == "dot";
        steer(
            &mut fades.second_width,
            if dot { 20.0 } else { 95.0 },
            now,
            animate,
        );
        steer(
            &mut fades.second_height,
            if dot { 20.0 } else { 2.0 },
            now,
            animate,
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
        let animate = !imp.moment.borrow().numbers.is_empty() && self.is_mapped();
        let rotate = imp.look.borrow().as_ref().is_some_and(|look| look.rotate);
        let now = self.now();
        let mut fades = imp.fades.get();
        let hour = -90.0 + 30.0 * (moment.hour as f64 + moment.minute as f64 / 60.0);
        clockwise(&mut fades.hour_angle, hour, now, animate);
        clockwise(
            &mut fades.minute_angle,
            -90.0 + 6.0 * moment.minute as f64,
            now,
            animate,
        );
        let second = 6.0 * moment.second as f64 + 90.0;
        clockwise(&mut fades.second_angle, second, now, animate && rotate);
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

    fn running(&self, now: i64) -> bool {
        let imp = self.imp();
        imp.turn_origin.get().is_some()
            || imp.fades.get().all().iter().any(|tween| tween.running(now))
    }

    fn run(&self) {
        self.queue_draw();
        let imp = self.imp();
        if imp.ticking.replace(true) {
            return;
        }
        self.add_tick_callback(|cookie, clock| {
            cookie.queue_draw();
            if cookie.running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            cookie.imp().ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn draw(&self, snapshot: &gtk4::Snapshot) {
        let imp = self.imp();
        let Some(theme) = imp.theme.borrow().clone() else {
            return;
        };
        let look = imp.look.borrow();
        let Some(look) = look.as_ref() else {
            return;
        };
        let theme = theme.borrow();
        let colors = &theme.colors;
        let now = self.now();
        let fades = imp.fades.get();
        let moment = imp.moment.borrow();
        let size = self.width() as f64;
        let bounds = graphene::Rect::new(0.0, 0.0, size as f32, size as f32);

        let background = colors.col_primary_container;
        let on_background = mix(colors.col_secondary, colors.col_primary_container, 0.15);
        let background_info = mix(colors.col_primary, colors.col_primary_container, 0.55);
        let hour_colour = colors.col_primary;
        let minute_colour = colors.col_tertiary;
        let second_colour = colors.col_primary;

        let deviation = (SHADOW_RADIUS + 1.0) / 3.3333;
        let shadow = gsk::Shadow::new(colors.col_shadow, 0.0, 0.0, (deviation * 2.0) as f32);
        snapshot.push_shadow(&[shadow]);
        {
            let cr = snapshot.append_cairo(&bounds);
            let turn = self.turn(now).to_radians();
            cr.translate(size / 2.0, size / 2.0);
            cr.rotate(turn);
            cr.translate(-size / 2.0, -size / 2.0);
            if look.sine {
                sine_cookie(&cr, size, look.sides as f64);
            } else {
                shapes::cookie(look.sides).trace(&cr, 0.0, 0.0, size);
            }
            paint(&cr, background, 1.0);
            let _ = cr.fill();
        }
        snapshot.pop();

        let cr = snapshot.append_cairo(&bounds);
        let centre = size / 2.0;

        let dots = fades.dots.value(now);
        if dots > 0.0 {
            let margin = 10.0 + 46.0 - dots * 34.0;
            for index in 0..12 {
                around(&cr, centre, 30.0 * index as f64, |cr| {
                    rounded(cr, margin, centre - 6.0, 12.0, 12.0, 6.0);
                });
                paint(&cr, on_background, dots);
                let _ = cr.fill();
            }
        }

        let numbers = fades.numbers.value(now);
        if numbers > 0.0 {
            let margin = 20.0 - 10.0 * numbers;
            let mut font = text::font(Family::Reading, 80.0, "");
            font.set_weight(pango::Weight::Heavy);
            for index in 0..4 {
                let angle = (90.0 * (index + 1) as f64).to_radians();
                let (x, y) = (centre, margin + 40.0);
                let (dx, dy) = (x - centre, y - centre);
                let at_x = centre + dx * angle.cos() - dy * angle.sin();
                let at_y = centre + dx * angle.sin() + dy * angle.cos();
                let label = (12 / 4 * (index + 1)).to_string();
                centred_text(&cr, &label, &font, on_background, numbers, at_x, at_y);
            }
        }

        let lines = fades.lines.value(now);
        if lines > 0.0 {
            let margin = 10.0 + 46.0 - lines * 34.0;
            for index in 0..12 {
                around(&cr, centre, 30.0 * index as f64, |cr| {
                    rounded(cr, margin, centre - 2.0, 18.0, 4.0, 9.0);
                });
                paint(&cr, on_background, lines);
                let _ = cr.fill();
            }
            for index in 0..60 {
                around(&cr, centre, 6.0 * index as f64, |cr| {
                    rounded(cr, margin, centre - 1.0, 7.0, 2.0, 3.5);
                });
                paint(&cr, on_background, lines);
                let _ = cr.fill();
            }
        }

        let marks = fades.hour_marks.value(now);
        if marks > 0.0 {
            let diameter = 135.0 * (1.75 - 0.75 * marks);
            let left = centre - diameter / 2.0;
            cr.arc(centre, centre, diameter / 2.0, 0.0, 2.0 * PI);
            paint(&cr, on_background, marks);
            let _ = cr.fill();
            let mark = mix(background_info, on_background, 0.5);
            for index in 0..12 {
                around(&cr, centre, 30.0 * index as f64, |cr| {
                    rounded(cr, left + 8.0, centre - 2.0, 12.0, 4.0, 6.0);
                });
                paint(&cr, mark, marks);
                let _ = cr.fill();
            }
        }

        let column = fades.column.value(now);
        if column > 0.0 && !moment.numbers.is_empty() {
            let small = fades.column_small.value(now);
            let scale = 1.4 - 0.4 * fades.column_shown.value(now);
            let lines: Vec<Line> = moment
                .numbers
                .iter()
                .map(|number| {
                    let am_pm = !(number.len() == 2 && number.chars().all(|c| c.is_ascii_digit()));
                    let (large, reduced) = if am_pm { (26.0, 20.0) } else { (68.0, 40.0) };
                    let mut font =
                        text::font(Family::Expressive, large + (reduced - large) * small, "");
                    font.set_weight(pango::Weight::Bold);
                    line(&cr, number, &font)
                })
                .collect();
            let width = lines.iter().map(|line| line.width).fold(0.0, f64::max);
            let total = lines.iter().map(|line| line.height).sum::<f64>()
                - 16.0 * (lines.len() as f64 - 1.0);
            let left = centre - width / 2.0;
            let top = centre - total / 2.0;
            cr.save().ok();
            cr.translate(centre, centre);
            cr.scale(scale, scale);
            cr.translate(-centre, -centre);
            paint(&cr, background_info, column);
            let mut y = top;
            for line in &lines {
                show(&cr, line, left + (width - line.width) / 2.0, y);
                y += line.height - 16.0;
            }
            cr.restore().ok();
        }

        let hour = fades.hour.value(now);
        let hollow = look.hour_hand == "hollow";
        if hollow && hour > 0.0 {
            draw_hour_hand(&cr, size, &fades, now, look, hour_colour, hour);
        }

        draw_date(
            &cr,
            size,
            &fades,
            now,
            look,
            &moment,
            colors,
            background_info,
        );

        let minute = fades.minute.value(now);
        if minute > 0.0 {
            let width = fades.minute_width.value(now);
            let radius = if look.minute_hand == "classic" {
                2.0
            } else {
                match look.minute_hand.as_str() {
                    "bold" => 10.0,
                    "medium" => 6.0,
                    _ => 2.5,
                }
            };
            let x = size / 2.0 - width / 2.0 - 15.0 * fades.minute_classic.value(now);
            around(&cr, centre, fades.minute_angle.value(now), |cr| {
                rounded(cr, x, centre - width / 2.0, 95.0, width, radius);
            });
            paint(&cr, minute_colour, minute);
            let _ = cr.fill();
        }

        if !hollow && hour > 0.0 {
            draw_hour_hand(&cr, size, &fades, now, look, hour_colour, hour);
        }

        let second = fades.second.value(now);
        if second > 0.0 {
            draw_second_hand(&cr, size, &fades, now, look, second_colour, second);
        }

        let dot = fades.centre.value(now);
        if dot > 0.0 {
            let colour = if look.minute_hand == "medium" {
                background
            } else {
                minute_colour
            };
            cr.arc(centre, centre, 3.0, 0.0, 2.0 * PI);
            paint(&cr, colour, dot);
            let _ = cr.fill();
        }
    }
}

fn draw_hour_hand(
    cr: &cairo::Context,
    size: f64,
    fades: &Fades,
    now: i64,
    look: &Look,
    colour: RGBA,
    opacity: f64,
) {
    let centre = size / 2.0;
    let classic = look.hour_hand == "classic";
    let height = if classic { 8.0 } else { 20.0 };
    let radius = if classic { 2.0 } else { 10.0 };
    let x = (size - 20.0) / 2.0 - 15.0 * fades.hour_classic.value(now);
    let y = centre - height / 2.0;
    let angle = fades.hour_angle.value(now);
    around(cr, centre, angle, |cr| {
        rounded(cr, x, y, 72.0, height, radius);
    });
    let fill = fades.hour_fill.value(now);
    let solid = RGBA::new(colour.red(), colour.green(), colour.blue(), fill as f32);
    paint(cr, solid, opacity);
    let _ = cr.fill();
    around(cr, centre, angle, |cr| {
        rounded(cr, x, y, 72.0, height, radius);
        rounded(
            cr,
            x + 4.0,
            y + 4.0,
            64.0,
            height - 8.0,
            (radius - 4.0).max(0.0),
        );
    });
    cr.set_fill_rule(cairo::FillRule::EvenOdd);
    paint(cr, colour, opacity);
    let _ = cr.fill();
    cr.set_fill_rule(cairo::FillRule::Winding);
}

fn draw_second_hand(
    cr: &cairo::Context,
    size: f64,
    fades: &Fades,
    now: i64,
    look: &Look,
    colour: RGBA,
    opacity: f64,
) {
    let centre = size / 2.0;
    let angle = fades.second_angle.value(now);
    let width = fades.second_width.value(now);
    let height = fades.second_height.value(now);
    let left = 10.0 + if look.second_hand == "dot" { 20.0 } else { 0.0 };
    around(cr, centre, angle, |cr| {
        rounded(
            cr,
            left,
            centre - height / 2.0,
            width,
            height,
            width.min(height) / 2.0,
        );
    });
    paint(cr, colour, opacity);
    let _ = cr.fill();
    let classic = fades.classic_dot.value(now);
    let dot = fades.classic_dot_width.value(now);
    if classic > 0.0 && dot > 0.0 {
        let radius = (rounding::SMALL as f64).min(dot / 2.0);
        around(cr, centre, angle, |cr| {
            rounded(cr, 40.0, centre - dot / 2.0, dot, dot, radius);
        });
        paint(cr, colour, opacity * classic);
        let _ = cr.fill();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_date(
    cr: &cairo::Context,
    size: f64,
    fades: &Fades,
    now: i64,
    look: &Look,
    moment: &Moment,
    colors: &crate::ui::theme::Colors,
    info: RGBA,
) {
    let centre = size / 2.0;

    let border = fades.border.value(now);
    if border > 0.0 && !moment.weekday_day.is_empty() {
        let radius = fades.border_radius.value(now);
        let step = 12.0_f64;
        let characters: Vec<char> = moment.weekday_day.chars().collect();
        let rotation = if look.seconds {
            6.0 * moment.second as f64 + 180.0 - step * characters.len() as f64 / 2.0
        } else {
            0.0
        };
        let font = text::font(Family::Title, 30.0, "wght=550");
        cr.save().ok();
        cr.translate(centre, centre);
        cr.rotate(rotation.to_radians());
        for (index, character) in characters.iter().enumerate() {
            let angle = (index as f64 * step).to_radians() - PI / 2.0;
            cr.save().ok();
            cr.translate(radius * angle.cos(), radius * angle.sin());
            cr.rotate(angle + PI / 2.0);
            centred_text(cr, &character.to_string(), &font, info, border, 0.0, 0.0);
            cr.restore().ok();
        }
        cr.restore().ok();
    }

    let rect = fades.rect.value(now);
    if rect > 0.0 {
        let (width, height) = (45.0 * rect, 30.0 * rect);
        let x = size - (40.0 - rect * 30.0) - width;
        let y = centre - height / 2.0;
        rounded(cr, x, y, width, height, rounding::SMALL as f64);
        paint(
            cr,
            mix(info, colors.col_secondary_container_hover, 0.5),
            rect,
        );
        let _ = cr.fill();
        let mut font = text::font(Family::Expressive, 20.0, "");
        font.set_weight(pango::Weight::Ultraheavy);
        centred_text(
            cr,
            &moment.day_padded,
            &font,
            colors.col_secondary_hover,
            rect,
            x + width / 2.0,
            y + height / 2.0,
        );
    }

    let bubble = fades.bubble.value(now);
    if bubble > 0.0 {
        let square = DATE_SQUARE * bubble;
        let mut font = text::font(Family::Expressive, 30.0, "");
        font.set_weight(pango::Weight::Heavy);
        for (x, y, shape, fill, ink, label) in [
            (
                0.0,
                0.0,
                Shape::Pentagon,
                colors.col_tertiary_container,
                colors.col_on_tertiary_container,
                &moment.day,
            ),
            (
                size - square,
                size - square,
                Shape::Pill,
                colors.col_secondary_container,
                colors.col_on_secondary_container,
                &moment.month,
            ),
        ] {
            shapes::polygon(shape).trace(cr, x, y, square);
            paint(cr, fill, bubble);
            let _ = cr.fill();
            centred_text(
                cr,
                label,
                &font,
                ink,
                bubble,
                x + square / 2.0,
                y + square / 2.0,
            );
        }
    }
}

fn around(cr: &cairo::Context, centre: f64, degrees: f64, path: impl FnOnce(&cairo::Context)) {
    cr.save().ok();
    cr.translate(centre, centre);
    cr.rotate(degrees.to_radians());
    cr.translate(-centre, -centre);
    cr.new_path();
    path(cr);
    cr.restore().ok();
}

fn rounded(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        PI / 2.0,
    );
    cr.arc(x + radius, y + height - radius, radius, PI / 2.0, PI);
    cr.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    cr.close_path();
}

fn paint(cr: &cairo::Context, colour: RGBA, opacity: f64) {
    cr.set_source_rgba(
        colour.red() as f64,
        colour.green() as f64,
        colour.blue() as f64,
        colour.alpha() as f64 * opacity,
    );
}

struct Line {
    layout: pango::Layout,
    width: f64,
    height: f64,
    baseline: f64,
}

fn line(cr: &cairo::Context, label: &str, font: &pango::FontDescription) -> Line {
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(font));
    layout.set_text(label);
    let metrics = layout.context().metrics(Some(font), None);
    let sixty_fourths = |units: i32| (units as f64 / pango::SCALE as f64 * 64.0).floor() / 64.0;
    let ascent = sixty_fourths(metrics.ascent());
    let descent = sixty_fourths(metrics.descent());
    Line {
        width: layout.size().0 as f64 / pango::SCALE as f64,
        height: (ascent + descent).ceil(),
        baseline: ascent,
        layout,
    }
}

fn show(cr: &cairo::Context, line: &Line, x: f64, top: f64) {
    let baseline = line.layout.baseline() as f64 / pango::SCALE as f64;
    cr.move_to(x, (top + line.baseline).round() - baseline);
    pangocairo::functions::show_layout(cr, &line.layout);
}

fn centred_text(
    cr: &cairo::Context,
    label: &str,
    font: &pango::FontDescription,
    colour: RGBA,
    opacity: f64,
    x: f64,
    y: f64,
) {
    let line = line(cr, label, font);
    paint(cr, colour, opacity);
    show(cr, &line, x - line.width / 2.0, y - line.height / 2.0);
}

fn sine_cookie(cr: &cairo::Context, size: f64, sides: f64) {
    let amplitude = size / 50.0;
    let radius = size / 2.0 - amplitude;
    let centre = size / 2.0;
    cr.new_path();
    for index in 0..=SINE_POINTS {
        let angle = index as f64 / SINE_POINTS as f64 * 2.0 * PI;
        let wave = (angle * sides + PI / 2.0).sin() * amplitude;
        let x = angle.cos() * (radius + wave) + centre;
        let y = angle.sin() * (radius + wave) + centre;
        if index == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    cr.close_path();
}

use gtk4::cairo;
use gtk4::gdk::RGBA;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::f64::consts::PI;

use super::Cookie;
use super::state::{Fades, Look, Moment};
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::{mix, rounding};
use crate::ui::widgets::display::text::{self, Family};

const SHADOW_RADIUS: f64 = 8.0;
const SINE_POINTS: usize = 360;
const DATE_SQUARE: f64 = 64.0;

impl Cookie {
    pub(super) fn draw(&self, snapshot: &gtk4::Snapshot) {
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
        let hour_color = colors.col_primary;
        let minute_color = colors.col_tertiary;
        let second_color = colors.col_primary;

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
        let center = size / 2.0;

        let dots = fades.dots.value(now);
        if dots > 0.0 {
            let margin = 10.0 + 46.0 - dots * 34.0;
            for index in 0..12 {
                rotate_around(&cr, center, 30.0 * index as f64, |cr| {
                    trace_rounded_rect(cr, margin, center - 6.0, 12.0, 12.0, 6.0);
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
                let (x, y) = (center, margin + 40.0);
                let (dx, dy) = (x - center, y - center);
                let at_x = center + dx * angle.cos() - dy * angle.sin();
                let at_y = center + dx * angle.sin() + dy * angle.cos();
                let label = (12 / 4 * (index + 1)).to_string();
                draw_centered_text(&cr, &label, &font, on_background, numbers, at_x, at_y);
            }
        }

        let lines = fades.lines.value(now);
        if lines > 0.0 {
            let margin = 10.0 + 46.0 - lines * 34.0;
            for index in 0..12 {
                rotate_around(&cr, center, 30.0 * index as f64, |cr| {
                    trace_rounded_rect(cr, margin, center - 2.0, 18.0, 4.0, 9.0);
                });
                paint(&cr, on_background, lines);
                let _ = cr.fill();
            }
            for index in 0..60 {
                rotate_around(&cr, center, 6.0 * index as f64, |cr| {
                    trace_rounded_rect(cr, margin, center - 1.0, 7.0, 2.0, 3.5);
                });
                paint(&cr, on_background, lines);
                let _ = cr.fill();
            }
        }

        let marks = fades.hour_marks.value(now);
        if marks > 0.0 {
            let diameter = 135.0 * (1.75 - 0.75 * marks);
            let left = center - diameter / 2.0;
            cr.arc(center, center, diameter / 2.0, 0.0, 2.0 * PI);
            paint(&cr, on_background, marks);
            let _ = cr.fill();
            let mark = mix(background_info, on_background, 0.5);
            for index in 0..12 {
                rotate_around(&cr, center, 30.0 * index as f64, |cr| {
                    trace_rounded_rect(cr, left + 8.0, center - 2.0, 12.0, 4.0, 6.0);
                });
                paint(&cr, mark, marks);
                let _ = cr.fill();
            }
        }

        let column = fades.column.value(now);
        if column > 0.0 && !moment.numbers.is_empty() {
            let small = fades.column_small.value(now);
            let scale = 1.4 - 0.4 * fades.column_scale_progress.value(now);
            let lines: Vec<Line> = moment
                .numbers
                .iter()
                .map(|number| {
                    let is_am_pm =
                        !(number.len() == 2 && number.chars().all(|c| c.is_ascii_digit()));
                    let (large, reduced) = if is_am_pm { (26.0, 20.0) } else { (68.0, 40.0) };
                    let mut font =
                        text::font(Family::Expressive, large + (reduced - large) * small, "");
                    font.set_weight(pango::Weight::Bold);
                    line(&cr, number, &font)
                })
                .collect();
            let width = lines.iter().map(|line| line.width).fold(0.0, f64::max);
            let total = lines.iter().map(|line| line.height).sum::<f64>()
                - 16.0 * (lines.len() as f64 - 1.0);
            let left = center - width / 2.0;
            let top = center - total / 2.0;
            cr.save().ok();
            cr.translate(center, center);
            cr.scale(scale, scale);
            cr.translate(-center, -center);
            paint(&cr, background_info, column);
            let mut y = top;
            for line in &lines {
                show(&cr, line, left + (width - line.width) / 2.0, y);
                y += line.height - 16.0;
            }
            cr.restore().ok();
        }

        let hour = fades.hour.value(now);
        let is_hollow = look.hour_hand == "hollow";
        if is_hollow && hour > 0.0 {
            draw_hour_hand(&cr, size, &fades, now, look, hour_color, hour);
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
            rotate_around(&cr, center, fades.minute_angle.value(now), |cr| {
                trace_rounded_rect(cr, x, center - width / 2.0, 95.0, width, radius);
            });
            paint(&cr, minute_color, minute);
            let _ = cr.fill();
        }

        if !is_hollow && hour > 0.0 {
            draw_hour_hand(&cr, size, &fades, now, look, hour_color, hour);
        }

        let second = fades.second.value(now);
        if second > 0.0 {
            draw_second_hand(&cr, size, &fades, now, look, second_color, second);
        }

        let dot = fades.center.value(now);
        if dot > 0.0 {
            let color = if look.minute_hand == "medium" {
                background
            } else {
                minute_color
            };
            cr.arc(center, center, 3.0, 0.0, 2.0 * PI);
            paint(&cr, color, dot);
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
    color: RGBA,
    opacity: f64,
) {
    let center = size / 2.0;
    let is_classic = look.hour_hand == "classic";
    let height = if is_classic { 8.0 } else { 20.0 };
    let radius = if is_classic { 2.0 } else { 10.0 };
    let x = (size - 20.0) / 2.0 - 15.0 * fades.hour_classic.value(now);
    let y = center - height / 2.0;
    let angle = fades.hour_angle.value(now);
    rotate_around(cr, center, angle, |cr| {
        trace_rounded_rect(cr, x, y, 72.0, height, radius);
    });
    let fill = fades.hour_fill.value(now);
    let solid = RGBA::new(color.red(), color.green(), color.blue(), fill as f32);
    paint(cr, solid, opacity);
    let _ = cr.fill();
    rotate_around(cr, center, angle, |cr| {
        trace_rounded_rect(cr, x, y, 72.0, height, radius);
        trace_rounded_rect(
            cr,
            x + 4.0,
            y + 4.0,
            64.0,
            height - 8.0,
            (radius - 4.0).max(0.0),
        );
    });
    cr.set_fill_rule(cairo::FillRule::EvenOdd);
    paint(cr, color, opacity);
    let _ = cr.fill();
    cr.set_fill_rule(cairo::FillRule::Winding);
}

fn draw_second_hand(
    cr: &cairo::Context,
    size: f64,
    fades: &Fades,
    now: i64,
    look: &Look,
    color: RGBA,
    opacity: f64,
) {
    let center = size / 2.0;
    let angle = fades.second_angle.value(now);
    let width = fades.second_width.value(now);
    let height = fades.second_height.value(now);
    let left = 10.0 + if look.second_hand == "dot" { 20.0 } else { 0.0 };
    rotate_around(cr, center, angle, |cr| {
        trace_rounded_rect(
            cr,
            left,
            center - height / 2.0,
            width,
            height,
            width.min(height) / 2.0,
        );
    });
    paint(cr, color, opacity);
    let _ = cr.fill();
    let classic = fades.classic_dot.value(now);
    let dot = fades.classic_dot_width.value(now);
    if classic > 0.0 && dot > 0.0 {
        let radius = (rounding::SMALL as f64).min(dot / 2.0);
        rotate_around(cr, center, angle, |cr| {
            trace_rounded_rect(cr, 40.0, center - dot / 2.0, dot, dot, radius);
        });
        paint(cr, color, opacity * classic);
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
    let center = size / 2.0;

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
        cr.translate(center, center);
        cr.rotate(rotation.to_radians());
        for (index, character) in characters.iter().enumerate() {
            let angle = (index as f64 * step).to_radians() - PI / 2.0;
            cr.save().ok();
            cr.translate(radius * angle.cos(), radius * angle.sin());
            cr.rotate(angle + PI / 2.0);
            draw_centered_text(cr, &character.to_string(), &font, info, border, 0.0, 0.0);
            cr.restore().ok();
        }
        cr.restore().ok();
    }

    let rect = fades.rect.value(now);
    if rect > 0.0 {
        let (width, height) = (45.0 * rect, 30.0 * rect);
        let x = size - (40.0 - rect * 30.0) - width;
        let y = center - height / 2.0;
        trace_rounded_rect(cr, x, y, width, height, rounding::SMALL as f64);
        paint(
            cr,
            mix(info, colors.col_secondary_container_hover, 0.5),
            rect,
        );
        let _ = cr.fill();
        let mut font = text::font(Family::Expressive, 20.0, "");
        font.set_weight(pango::Weight::Ultraheavy);
        draw_centered_text(
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
            draw_centered_text(
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

fn rotate_around(
    cr: &cairo::Context,
    center: f64,
    degrees: f64,
    path: impl FnOnce(&cairo::Context),
) {
    cr.save().ok();
    cr.translate(center, center);
    cr.rotate(degrees.to_radians());
    cr.translate(-center, -center);
    cr.new_path();
    path(cr);
    cr.restore().ok();
}

fn trace_rounded_rect(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
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

fn paint(cr: &cairo::Context, color: RGBA, opacity: f64) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64 * opacity,
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

fn draw_centered_text(
    cr: &cairo::Context,
    label: &str,
    font: &pango::FontDescription,
    color: RGBA,
    opacity: f64,
    x: f64,
    y: f64,
) {
    let line = line(cr, label, font);
    paint(cr, color, opacity);
    show(cr, &line, x - line.width / 2.0, y - line.height / 2.0);
}

fn sine_cookie(cr: &cairo::Context, size: f64, sides: f64) {
    let amplitude = size / 50.0;
    let radius = size / 2.0 - amplitude;
    let center = size / 2.0;
    cr.new_path();
    for index in 0..=SINE_POINTS {
        let angle = index as f64 / SINE_POINTS as f64 * 2.0 * PI;
        let wave = (angle * sides + PI / 2.0).sin() * amplitude;
        let x = angle.cos() * (radius + wave) + center;
        let y = angle.sin() * (radius + wave) + center;
        if index == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    cr.close_path();
}

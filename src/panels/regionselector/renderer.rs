use gtk4::gdk::{self, RGBA};
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;

use super::targets::Target;
use super::{
    AIM_OPACITY, BREATH_HIGH, BREATH_LOW, BREATH_MILLIS, DASH, GAP, LABEL_MARGIN, OVERLAY_ALPHA,
    Phase, REGION_BORDER, REGION_BORDER_TARGETED, REGION_TEXT_PADDING, Selection, TAG_PADDING,
    TAG_RADIUS, TAG_SPACING,
};
use crate::ui::theme::{mix, pixel_size, transparentize};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::layout::centered;

impl Selection {
    fn colors(&self) -> Colors {
        let theme = self.theme.borrow();
        let theme_colors = &theme.colors;
        let is_dark_mode = theme.m3.is_dark_mode;
        let bright_text = if is_dark_mode {
            theme_colors.col_on_layer0
        } else {
            theme_colors.col_layer0
        };
        let bright_secondary = if is_dark_mode {
            theme_colors.col_secondary
        } else {
            theme_colors.col_on_secondary
        };
        Colors {
            overlay: RGBA::new(0.0, 0.0, 0.0, OVERLAY_ALPHA),
            selection: mix(bright_text, bright_secondary, 0.5),
            window_border: bright_secondary,
            window_fill: transparentize(bright_secondary, 0.85),
            outline_variant: theme.m3.outline_variant,
        }
    }

    pub(super) fn draw(&self, snapshot: &gtk4::Snapshot) {
        let now = self.now();
        let colors = self.colors();
        let (width, height) = self.size;
        let is_selecting = self.phase.get() == Phase::Select;
        if is_selecting && let Some(texture) = self.screenshot_texture.borrow().as_ref() {
            snapshot.append_scaled_texture(
                texture,
                gsk::ScalingFilter::Linear,
                &rect(0.0, 0.0, width, height),
            );
        }
        if self.mode.get().is_circle() {
            self.draw_circle(snapshot, &colors);
        } else {
            self.draw_rect(snapshot, &colors);
        }
        if !is_selecting {
            return;
        }
        let opacity = self.fade.get().value(now);
        if opacity <= 0.0 {
            return;
        }
        let mode = self.mode.get();
        let fills = self.fills.borrow();
        let mut index = 0;
        for (is_enabled, targets, has_icon) in [
            (
                self.config.region.target_windows && mode.picks_region(),
                &self.windows,
                true,
            ),
            (
                self.config.region.target_layers && mode.picks_region(),
                &self.layers,
                false,
            ),
        ] {
            for (position, target) in targets.iter().enumerate() {
                let fill = fills
                    .get(index)
                    .map(|tween| tween.value(now))
                    .unwrap_or(0.0);
                index += 1;
                if !is_enabled {
                    continue;
                }
                for part in uncovered_parts(&target.bounds, &targets[..position]) {
                    snapshot.push_clip(&part);
                    self.draw_target(snapshot, &colors, target, fill, opacity, has_icon);
                    snapshot.pop();
                }
            }
        }
    }

    fn draw_rect(&self, snapshot: &gtk4::Snapshot, colors: &Colors) {
        let (width, height) = self.size;
        let [x, y, region_width, region_height] = self.region.get();
        let is_breathing = self.phase.get() == Phase::Post;
        if !is_breathing {
            for bounds in [
                rect(0.0, 0.0, width, y.max(0.0)),
                rect(0.0, y + region_height, width, height - y - region_height),
                rect(0.0, y, x.max(0.0), region_height),
                rect(x + region_width, y, width - x - region_width, region_height),
            ] {
                if bounds.width() > 0.0 && bounds.height() > 0.0 {
                    snapshot.append_color(&colors.overlay, &bounds);
                }
            }
            let text = format!("{} x {}", region_width.round(), region_height.round());
            let layout = self.paint.create_pango_layout(Some(&text));
            layout.set_font_description(Some(&text::font(
                Family::Main,
                pixel_size::SMALL as f64,
                "wght=450",
            )));
            let (_, logical_extents) = layout.pixel_extents();
            let border_right = x.round() + region_width.round() + 1.0;
            let border_bottom = y.round() + region_height.round() + 1.0;
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                (border_right - LABEL_MARGIN - logical_extents.width() as f64) as f32,
                (border_bottom + LABEL_MARGIN) as f32,
            ));
            snapshot.append_layout(&layout, &colors.selection);
            snapshot.restore();
            if self.config.region.aim_lines {
                let (pointer_x, pointer_y) = self.pointer.get();
                let aim_color = transparentize(colors.selection, 1.0 - AIM_OPACITY);
                snapshot.append_color(&aim_color, &rect(pointer_x, 0.0, 1.0, height));
                snapshot.append_color(&aim_color, &rect(0.0, pointer_y, width, 1.0));
            }
        }
        let opacity = if is_breathing {
            breath((self.now() - self.breath_start.get()) as f64 / 1000.0)
        } else {
            BREATH_HIGH
        };
        let left = x.round() - 1.0;
        let top = y.round() - 1.0;
        let border_width = region_width.round() + 2.0;
        let border_height = region_height.round() + 2.0;
        let bounds = rect(left, top, border_width.max(1.0), border_height.max(1.0));
        let cr = snapshot.append_cairo(&bounds);
        cr.set_source_rgba(
            colors.selection.red() as f64,
            colors.selection.green() as f64,
            colors.selection.blue() as f64,
            colors.selection.alpha() as f64 * opacity,
        );
        cr.set_line_width(1.0);
        cr.set_dash(&[DASH, GAP], 0.0);
        cr.rectangle(
            left + 0.5,
            top + 0.5,
            border_width - 1.0,
            border_height - 1.0,
        );
        let _ = cr.stroke();
    }

    fn draw_circle(&self, snapshot: &gtk4::Snapshot, colors: &Colors) {
        let (width, height) = self.size;
        let bounds = rect(0.0, 0.0, width, height);
        snapshot.append_color(&colors.overlay, &bounds);
        let drag = self.drag.borrow();
        if drag.points.is_empty() {
            return;
        }
        let cr = snapshot.append_cairo(&bounds);
        cr.set_source_rgba(
            colors.selection.red() as f64,
            colors.selection.green() as f64,
            colors.selection.blue() as f64,
            colors.selection.alpha() as f64,
        );
        cr.set_line_width(self.config.region.circle_stroke);
        cr.set_line_cap(gtk4::cairo::LineCap::Round);
        cr.set_line_join(gtk4::cairo::LineJoin::Round);
        let (first_x, first_y) = drag.points[0];
        cr.move_to(first_x, first_y);
        for (x, y) in drag.points.iter().skip(1) {
            cr.line_to(*x, *y);
        }
        let _ = cr.stroke();
    }

    fn draw_target(
        &self,
        snapshot: &gtk4::Snapshot,
        colors: &Colors,
        target: &Target,
        fill: f64,
        opacity: f64,
        has_icon: bool,
    ) {
        let [x, y, width, height] = target.bounds;
        let bounds = rect(x, y, width, height);
        let outline = gsk::RoundedRect::from_rect(bounds, target.radius as f32);
        let is_targeted = self.target_bounds.get() == Some(target.bounds);
        snapshot.push_opacity(opacity);
        if fill > 0.0 {
            let color = mix(
                colors.window_fill,
                RGBA::new(0.0, 0.0, 0.0, 0.0),
                fill as f32,
            );
            snapshot.push_rounded_clip(&outline);
            snapshot.append_color(&color, &bounds);
            snapshot.pop();
        }
        let stroke_width = if is_targeted {
            REGION_BORDER_TARGETED
        } else {
            REGION_BORDER
        };
        snapshot.append_border(&outline, &[stroke_width; 4], &[colors.window_border; 4]);
        if self.config.region.show_label {
            self.draw_tag(snapshot, colors, target, has_icon);
        }
        snapshot.pop();
    }

    fn draw_tag(
        &self,
        snapshot: &gtk4::Snapshot,
        colors: &Colors,
        target: &Target,
        has_icon: bool,
    ) {
        let layout = self.paint.create_pango_layout(Some(&target.label));
        layout.set_font_description(Some(&text::font(
            Family::Main,
            pixel_size::SMALL as f64,
            "wght=450",
        )));
        let (_, logical_extents) = layout.pixel_extents();
        let text_height = centered::layout_qt_metrics(&layout)
            .map(|(height, _)| height as f64)
            .unwrap_or(logical_extents.height() as f64);
        let icon_size = pixel_size::LARGER as f64;
        let icon_width = if has_icon {
            icon_size + TAG_SPACING
        } else {
            0.0
        };
        let row_height = text_height.max(if has_icon { icon_size } else { 0.0 });
        let width = icon_width + logical_extents.width() as f64 + TAG_PADDING.0 * 2.0;
        let height = row_height + TAG_PADDING.1 * 2.0;
        let left = target.bounds[0] + REGION_TEXT_PADDING;
        let top = target.bounds[1] + REGION_TEXT_PADDING;
        let bounds = rect(left, top, width, height);
        let outline = gsk::RoundedRect::from_rect(bounds, TAG_RADIUS as f32);
        snapshot.push_rounded_clip(&outline);
        snapshot.append_color(
            &RGBA::new(
                0x11 as f32 / 255.0,
                0x11 as f32 / 255.0,
                0x11 as f32 / 255.0,
                0.9,
            ),
            &bounds,
        );
        snapshot.pop();
        snapshot.append_border(&outline, &[1.0; 4], &[colors.outline_variant; 4]);
        let mut cursor = left + TAG_PADDING.0;
        if has_icon && let Some(display) = gdk::Display::default() {
            let theme = gtk4::IconTheme::for_display(&display);
            let name = crate::platform::system::appicon::guess(&theme, &target.label);
            let paintable = crate::platform::system::appicon::load_icon(
                &theme,
                &name,
                "image-missing",
                icon_size as i32,
                self.paint.scale_factor(),
            );
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                cursor as f32,
                (top + (height - icon_size) / 2.0) as f32,
            ));
            paintable.snapshot(snapshot, icon_size, icon_size);
            snapshot.restore();
            cursor += icon_width;
        }
        snapshot.save();
        snapshot.translate(&graphene::Point::new(
            cursor as f32,
            (top + (height - logical_extents.height() as f64) / 2.0) as f32,
        ));
        snapshot.append_layout(&layout, &RGBA::new(1.0, 1.0, 1.0, 0xdd as f32 / 255.0));
        snapshot.restore();
    }
}

struct Colors {
    overlay: RGBA,
    selection: RGBA,
    window_border: RGBA,
    window_fill: RGBA,
    outline_variant: RGBA,
}

fn breath(seconds: f64) -> f64 {
    let half_period = BREATH_MILLIS / 1000.0;
    let phase = seconds % (half_period * 2.0);
    let (from, to, part) = if phase < half_period {
        (BREATH_HIGH, BREATH_LOW, phase / half_period)
    } else {
        (BREATH_LOW, BREATH_HIGH, (phase - half_period) / half_period)
    };
    let eased_part = if part < 0.5 {
        2.0 * part * part
    } else {
        1.0 - (-2.0 * part + 2.0).powi(2) / 2.0
    };
    from + (to - from) * eased_part
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> graphene::Rect {
    graphene::Rect::new(x as f32, y as f32, width as f32, height as f32)
}

fn uncovered_parts(bounds: &[f64; 4], above: &[Target]) -> Vec<graphene::Rect> {
    let pixel_rect = |[x, y, width, height]: [f64; 4]| {
        let (left, top) = (x.floor() as i32, y.floor() as i32);
        let (right, bottom) = ((x + width).ceil() as i32, (y + height).ceil() as i32);
        gtk4::cairo::RectangleInt::new(left, top, right - left, bottom - top)
    };
    let region = gtk4::cairo::Region::create_rectangle(&pixel_rect(*bounds));
    for other in above {
        let _ = region.subtract_rectangle(&pixel_rect(other.bounds));
    }
    (0..region.num_rectangles())
        .map(|index| {
            let part = region.rectangle(index);
            rect(
                part.x() as f64,
                part.y() as f64,
                part.width() as f64,
                part.height() as f64,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_outline_keeps_only_what_the_windows_above_leave_visible() {
        let above = |bounds: [f64; 4]| Target {
            bounds,
            label: String::new(),
            radius: 0.0,
        };
        let area = |parts: Vec<graphene::Rect>| {
            parts
                .iter()
                .map(|part| part.width() * part.height())
                .sum::<f32>()
        };
        let window = [0.0, 0.0, 100.0, 100.0];
        assert_eq!(area(uncovered_parts(&window, &[])), 10000.0);
        assert_eq!(
            area(uncovered_parts(&window, &[above([50.0, 50.0, 80.0, 80.0])])),
            7500.0
        );
        assert!(uncovered_parts(&window, &[above([-10.0, -10.0, 200.0, 200.0])]).is_empty());
    }
}

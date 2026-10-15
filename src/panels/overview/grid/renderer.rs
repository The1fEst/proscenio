use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;

use super::model::{Monitor, State, Thumb, thumb_corners};
use super::{Grid, now};
use crate::ui::theme::{mix, transparentize};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::layout::centered;

const NUMBER_SIZE: f64 = 250.0;
const WORKSPACE_BORDER: f32 = 2.0;
const WINDOW_BORDER: f32 = 1.0;
const ICON_GAP_RATIO: f64 = 0.06;
const OTHER_MONITOR_OPACITY: f64 = 0.4;

impl Grid {
    pub(super) fn draw(&self, snapshot: &gtk4::Snapshot) {
        let now = now(&self.paint);
        let state = self.state.borrow();
        let (Some(monitor), Some(layout)) = (state.monitor.as_ref(), state.layout) else {
            return;
        };
        let theme = self.theme.borrow();
        let colors = &theme.colors;
        let workspace_count = state.workspace_count();
        let active_drag = state.drag.as_ref().filter(|drag| drag.has_moved);
        let number_color = transparentize(colors.col_on_layer1, 0.8);
        let mut number_font = text::font(
            Family::Expressive,
            NUMBER_SIZE * monitor.scale * state.scale,
            "wght=450",
        );
        number_font.set_weight(pango::Weight::Semibold);

        for row in 0..layout.rows {
            for column in 0..layout.columns {
                let workspace =
                    state.group * workspace_count + layout.workspace_in_cell(row, column);
                let (x, y) = layout.origin(row, column);
                let bounds = rect(x, y, layout.cell.0, layout.cell.1);
                let outline = rounded_rect(&bounds, layout.corners(row, column));
                let is_drop_target = active_drag.is_some_and(|drag| {
                    state.target_workspace == Some(workspace) && drag.source_workspace != workspace
                });
                let fill = if is_drop_target {
                    mix(
                        colors.col_surface_container_low,
                        colors.col_layer1_hover,
                        0.1,
                    )
                } else {
                    colors.col_surface_container_low
                };
                snapshot.push_rounded_clip(&outline);
                snapshot.append_color(&fill, &bounds);
                snapshot.pop();
                if is_drop_target {
                    snapshot.append_border(
                        &outline,
                        &[WORKSPACE_BORDER; 4],
                        &[colors.col_layer2_hover; 4],
                    );
                }
                let text = self.paint.create_pango_layout(Some(&workspace.to_string()));
                text.set_font_description(Some(&number_font));
                let (_, logical) = text.pixel_extents();
                let baseline = text.baseline() as f64 / gtk4::pango::SCALE as f64;
                let (height, ascent) = centered::layout_qt_metrics(&text)
                    .map(|(height, ascent)| (height as f64, ascent as f64))
                    .unwrap_or((logical.height() as f64, baseline));
                let left = x + ((layout.cell.0 - logical.width() as f64) / 2.0).round();
                let top = y + ((layout.cell.1 - height) / 2.0).round();
                snapshot.save();
                snapshot.translate(&graphene::Point::new(
                    left as f32,
                    (top + ascent - baseline) as f32,
                ));
                snapshot.append_layout(&text, &number_color);
                snapshot.restore();
            }
        }

        let (order, indicator_position) = state.order();
        for (position, index) in order.iter().enumerate() {
            if position == indicator_position {
                self.draw_indicator(snapshot, &state, now);
            }
            self.draw_thumb(snapshot, &state, &state.thumbs[*index], monitor, now);
        }
        if indicator_position == order.len() {
            self.draw_indicator(snapshot, &state, now);
        }
    }

    fn draw_indicator(&self, snapshot: &gtk4::Snapshot, state: &State, now: i64) {
        let (Some(indicator), Some(layout)) = (state.indicator.as_ref(), state.layout) else {
            return;
        };
        let bounds = rect(
            indicator.x.value(now),
            indicator.y.value(now),
            layout.cell.0,
            layout.cell.1,
        );
        let corners = [0, 1, 2, 3].map(|corner| indicator.corners[corner].value(now));
        snapshot.append_border(
            &rounded_rect(&bounds, corners),
            &[WORKSPACE_BORDER; 4],
            &[self.theme.borrow().colors.col_secondary; 4],
        );
    }

    fn draw_thumb(
        &self,
        snapshot: &gtk4::Snapshot,
        state: &State,
        thumb: &Thumb,
        monitor: &Monitor,
        now: i64,
    ) {
        let Some(layout) = state.layout else {
            return;
        };
        let theme = self.theme.borrow();
        let colors = &theme.colors;
        let [x, y, width, height] = state.thumb_rect(thumb, now);
        let (left, top) = (x.round(), y.round());
        let bounds = rect(
            left,
            top,
            (x + width).round() - left,
            (y + height).round() - top,
        );
        let outline = rounded_rect(&bounds, thumb_corners(thumb, &layout));
        let is_pressed = state
            .drag
            .as_ref()
            .is_some_and(|drag| drag.address == thumb.client.address);
        let is_hovered = state.hovered_address.as_deref() == Some(thumb.client.address.as_str());
        let is_dimmed = thumb.client.monitor_id != monitor.id;
        if is_dimmed {
            snapshot.push_opacity(OTHER_MONITOR_OPACITY);
        }
        snapshot.push_rounded_clip(&outline);
        if let Some(texture) = &thumb.texture {
            snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Trilinear, &bounds);
        }
        let overlay = if is_pressed {
            Some(transparentize(colors.col_layer2_active, 0.5))
        } else if is_hovered {
            Some(transparentize(colors.col_layer2_hover, 0.7))
        } else {
            None
        };
        if let Some(overlay) = overlay {
            snapshot.append_color(&overlay, &bounds);
        }
        snapshot.append_border(
            &outline,
            &[WINDOW_BORDER; 4],
            &[transparentize(theme.m3.outline, 0.88); 4],
        );
        if let Some((_, icon)) = &thumb.icon {
            let size = thumb.icon_size.value(now);
            let base = thumb.target[2].min(thumb.target[3]);
            let (left, top) = if self.config.overview_center_icons {
                (
                    left + ((bounds.width() as f64 - size) / 2.0).round(),
                    top + ((bounds.height() as f64 - size) / 2.0).round(),
                )
            } else {
                let gap = base * ICON_GAP_RATIO;
                (left + gap, top + gap)
            };
            snapshot.save();
            snapshot.translate(&graphene::Point::new(left as f32, top as f32));
            icon.snapshot(snapshot, size, size);
            snapshot.restore();
        }
        snapshot.pop();
        if is_dimmed {
            snapshot.pop();
        }
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> graphene::Rect {
    graphene::Rect::new(x as f32, y as f32, width as f32, height as f32)
}

fn rounded_rect(bounds: &graphene::Rect, corners: [f64; 4]) -> gsk::RoundedRect {
    let size = |radius: f64| graphene::Size::new(radius as f32, radius as f32);
    gsk::RoundedRect::new(
        *bounds,
        size(corners[0]),
        size(corners[1]),
        size(corners[2]),
        size(corners[3]),
    )
}

use gtk4::gdk::RGBA;
use gtk4::prelude::*;
use gtk4::{graphene, gsk};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::services::hardware::displays::Monitor;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;

const HEIGHT: i32 = 240;
const FILL_RATIO: f64 = 0.9;
const SNAP_DISTANCE: f64 = 10.0;
const DRAG_THRESHOLD: f64 = 10.0;

struct Drag {
    name: String,
    grab_origin: (f64, f64),
    is_active: bool,
    drop_position: (i64, i64),
}

struct Layout {
    span_left: f64,
    span_top: f64,
    zoom: f64,
    origin_x: f64,
    origin_y: f64,
}

pub struct Arrangement {
    pub paint: Paint,
    monitors: RefCell<Vec<Monitor>>,
    selected_name: RefCell<String>,
    drag: RefCell<Option<Drag>>,
    on_moved: RefCell<Option<Box<dyn Fn(&str, i64, i64)>>>,
    on_picked: RefCell<Option<Box<dyn Fn(&str)>>>,
    is_hovering: Cell<bool>,
}

impl Arrangement {
    pub fn new(theme: &SharedTheme) -> Rc<Self> {
        let paint = Paint::new(|_, _, _| {});
        paint.set_size_request(-1, HEIGHT);
        paint.set_hexpand(true);
        let arrangement = Rc::new(Arrangement {
            paint: paint.clone(),
            monitors: RefCell::new(Vec::new()),
            selected_name: RefCell::new(String::new()),
            drag: RefCell::new(None),
            on_moved: RefCell::new(None),
            on_picked: RefCell::new(None),
            is_hovering: Cell::new(false),
        });
        paint.set_draw({
            let theme = theme.clone();
            let arrangement = Rc::downgrade(&arrangement);
            move |snapshot, width, height| {
                if let Some(arrangement) = arrangement.upgrade() {
                    arrangement.draw(&theme, snapshot, width as f64, height as f64);
                }
            }
        });

        let drag = gtk4::GestureDrag::new();
        drag.connect_drag_begin({
            let arrangement = Rc::downgrade(&arrangement);
            move |_, x, y| {
                if let Some(arrangement) = arrangement.upgrade() {
                    arrangement.begin(x, y);
                }
            }
        });
        drag.connect_drag_update({
            let arrangement = Rc::downgrade(&arrangement);
            move |_, dx, dy| {
                if let Some(arrangement) = arrangement.upgrade() {
                    arrangement.update(dx, dy);
                }
            }
        });
        drag.connect_drag_end({
            let arrangement = Rc::downgrade(&arrangement);
            move |_, _, _| {
                if let Some(arrangement) = arrangement.upgrade() {
                    arrangement.end();
                }
            }
        });
        paint.add_controller(drag);

        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_motion({
            let arrangement = Rc::downgrade(&arrangement);
            move |_, x, y| {
                if let Some(arrangement) = arrangement.upgrade() {
                    let is_over_plate = arrangement.plate_at(x, y).is_some();
                    if arrangement.is_hovering.replace(is_over_plate) != is_over_plate {
                        arrangement.paint.set_cursor_from_name(if is_over_plate {
                            Some("grab")
                        } else {
                            None
                        });
                    }
                }
            }
        });
        paint.add_controller(motion_controller);
        arrangement
    }

    pub fn connect_moved(&self, action: impl Fn(&str, i64, i64) + 'static) {
        self.on_moved.replace(Some(Box::new(action)));
    }

    pub fn connect_picked(&self, action: impl Fn(&str) + 'static) {
        self.on_picked.replace(Some(Box::new(action)));
    }

    pub fn set(&self, monitors: Vec<Monitor>, selected_name: &str) {
        self.monitors.replace(monitors);
        self.selected_name.replace(selected_name.to_owned());
        self.paint.queue_draw();
    }

    fn layout(&self) -> Layout {
        let monitors = self.monitors.borrow();
        let width = self.paint.width() as f64;
        let height = self.paint.height() as f64;
        let span_left = monitors.iter().map(|m| m.x).min().unwrap_or(0).min(0) as f64;
        let span_top = monitors.iter().map(|m| m.y).min().unwrap_or(0).min(0) as f64;
        let span_width = monitors
            .iter()
            .map(|m| m.x + m.width)
            .max()
            .unwrap_or(1)
            .max(1) as f64
            - span_left;
        let span_height = monitors
            .iter()
            .map(|m| m.y + m.height)
            .max()
            .unwrap_or(1)
            .max(1) as f64
            - span_top;
        let zoom = (width / span_width.max(1.0)).min(height / span_height.max(1.0)) * FILL_RATIO;
        Layout {
            span_left,
            span_top,
            zoom,
            origin_x: (width - span_width * zoom) / 2.0,
            origin_y: (height - span_height * zoom) / 2.0,
        }
    }

    fn home(&self, layout: &Layout, monitor: &Monitor) -> (f64, f64) {
        (
            (monitor.x as f64 - layout.span_left) * layout.zoom + layout.origin_x,
            (monitor.y as f64 - layout.span_top) * layout.zoom + layout.origin_y,
        )
    }

    fn drawn_position(&self, layout: &Layout, monitor: &Monitor) -> (f64, f64) {
        if let Some(drag) = self.drag.borrow().as_ref()
            && drag.is_active
            && drag.name == monitor.name
        {
            return (
                (drag.drop_position.0 as f64 - layout.span_left) * layout.zoom + layout.origin_x,
                (drag.drop_position.1 as f64 - layout.span_top) * layout.zoom + layout.origin_y,
            );
        }
        self.home(layout, monitor)
    }

    fn plate_at(&self, x: f64, y: f64) -> Option<Monitor> {
        let layout = self.layout();
        self.monitors
            .borrow()
            .iter()
            .rev()
            .find(|monitor| {
                let (left, top) = self.drawn_position(&layout, monitor);
                x >= left
                    && y >= top
                    && x < left + monitor.width as f64 * layout.zoom
                    && y < top + monitor.height as f64 * layout.zoom
            })
            .cloned()
    }

    fn begin(&self, x: f64, y: f64) {
        let Some(monitor) = self.plate_at(x, y) else {
            self.drag.replace(None);
            return;
        };
        let layout = self.layout();
        self.drag.replace(Some(Drag {
            name: monitor.name.clone(),
            grab_origin: self.home(&layout, &monitor),
            is_active: false,
            drop_position: (monitor.x, monitor.y),
        }));
    }

    fn update(&self, dx: f64, dy: f64) {
        let layout = self.layout();
        let monitors = self.monitors.borrow().clone();
        let mut drag_slot = self.drag.borrow_mut();
        let Some(drag) = drag_slot.as_mut() else {
            return;
        };
        if !drag.is_active && dx.hypot(dy) < DRAG_THRESHOLD {
            return;
        }
        drag.is_active = true;
        let Some(plate) = monitors.iter().find(|m| m.name == drag.name) else {
            return;
        };
        let neighbors: Vec<&Monitor> = monitors.iter().filter(|m| m.name != plate.name).collect();
        let reach = SNAP_DISTANCE / layout.zoom;
        let mut lefts = Vec::new();
        let mut tops = Vec::new();
        for other in &neighbors {
            lefts.extend([
                other.x,
                other.x + other.width,
                other.x - plate.width,
                other.x + other.width - plate.width,
            ]);
            tops.extend([
                other.y,
                other.y + other.height,
                other.y - plate.height,
                other.y + other.height - plate.height,
            ]);
        }
        let px = drag.grab_origin.0 + dx;
        let py = drag.grab_origin.1 + dy;
        let x = snap(
            layout.span_left + (px - layout.origin_x) / layout.zoom,
            &lefts,
            reach,
        );
        let y = snap(
            layout.span_top + (py - layout.origin_y) / layout.zoom,
            &tops,
            reach,
        );
        drag.drop_position = place_beside(plate, &neighbors, x.round() as i64, y.round() as i64);
        drop(drag_slot);
        self.paint.queue_draw();
    }

    fn end(&self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        self.paint.queue_draw();
        if drag.is_active {
            if let Some(on_moved) = self.on_moved.borrow().as_ref() {
                on_moved(&drag.name, drag.drop_position.0, drag.drop_position.1);
            }
        } else if let Some(on_picked) = self.on_picked.borrow().as_ref() {
            on_picked(&drag.name);
        }
    }

    fn draw(&self, theme: &SharedTheme, snapshot: &gtk4::Snapshot, width: f64, height: f64) {
        let theme = theme.borrow();
        let colors = &theme.colors;
        let field = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(field, rounding::SMALL as f32));
        snapshot.append_color(&colors.col_layer2, &field);
        snapshot.pop();

        let layout = self.layout();
        let selected_name = self.selected_name.borrow().clone();
        let dragged_name = self
            .drag
            .borrow()
            .as_ref()
            .filter(|drag| drag.is_active)
            .map(|drag| drag.name.clone());
        for monitor in self.monitors.borrow().iter() {
            let (left, top) = self.drawn_position(&layout, monitor);
            let bounds = graphene::Rect::new(
                left as f32,
                top as f32,
                (monitor.width as f64 * layout.zoom) as f32,
                (monitor.height as f64 * layout.zoom) as f32,
            );
            let shape = gsk::RoundedRect::from_rect(bounds, rounding::VERYSMALL as f32);
            let is_selected = monitor.name == selected_name;
            let is_dragged = dragged_name.as_deref() == Some(monitor.name.as_str());
            let fill = if is_selected {
                colors.col_secondary_container
            } else {
                colors.col_layer3
            };
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&fill, &bounds);
            snapshot.pop();
            let (line_width, border) = if is_dragged {
                (2.0, colors.col_primary)
            } else {
                (1.0, colors.col_outline_variant)
            };
            snapshot.append_border(&shape, &[line_width; 4], &[border; 4]);
            let label = if monitor.model.is_empty() {
                &monitor.name
            } else {
                &monitor.model
            };
            let color = if is_selected {
                colors.col_on_secondary_container
            } else {
                colors.col_on_layer3
            };
            draw_centered_text(snapshot, &self.paint, label, color, &bounds);
        }
    }
}

fn draw_centered_text(
    snapshot: &gtk4::Snapshot,
    paint: &Paint,
    label: &str,
    color: RGBA,
    bounds: &graphene::Rect,
) {
    let layout = paint.create_pango_layout(Some(label));
    layout.set_font_description(Some(&text::font(
        text::Family::Main,
        pixel_size::SMALL as f64,
        "wght=450",
    )));
    let (_, logical) = layout.pixel_extents();
    let x = bounds.x() + (bounds.width() - logical.width() as f32) / 2.0;
    let y = bounds.y() + (bounds.height() - logical.height() as f32) / 2.0;
    snapshot.save();
    snapshot.translate(&graphene::Point::new(x.round(), y.round()));
    snapshot.append_layout(&layout, &color);
    snapshot.restore();
}

fn snap(value: f64, edges: &[i64], reach: f64) -> f64 {
    let mut best = value;
    let mut closest = reach;
    for &edge in edges {
        let distance = (edge as f64 - value).abs();
        if distance >= closest {
            continue;
        }
        closest = distance;
        best = edge as f64;
    }
    best
}

fn find_covering<'a>(
    plate: &Monitor,
    neighbors: &[&'a Monitor],
    x: i64,
    y: i64,
) -> Option<&'a Monitor> {
    neighbors
        .iter()
        .find(|other| {
            x < other.x + other.width
                && other.x < x + plate.width
                && y < other.y + other.height
                && other.y < y + plate.height
        })
        .copied()
}

fn place_beside(plate: &Monitor, neighbors: &[&Monitor], x: i64, y: i64) -> (i64, i64) {
    let (mut spot_x, mut spot_y) = (x, y);
    for _ in 0..=neighbors.len() {
        let Some(covering) = find_covering(plate, neighbors, spot_x, spot_y) else {
            break;
        };
        let mut sides = [
            (covering.x - plate.width, spot_y),
            (covering.x + covering.width, spot_y),
            (spot_x, covering.y - plate.height),
            (spot_x, covering.y + covering.height),
        ];
        sides.sort_by_key(|(side_x, side_y)| (side_x - spot_x).abs() + (side_y - spot_y).abs());
        (spot_x, spot_y) = sides[0];
    }
    (spot_x, spot_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(name: &str, x: i64, y: i64, width: i64, height: i64) -> Monitor {
        Monitor {
            id: 0,
            name: name.to_owned(),
            model: String::new(),
            width,
            height,
            x,
            y,
            scale: 1.0,
            transform: 0,
            refresh_rate: 60.0,
            mirror_of: "none".to_owned(),
            current_format: String::new(),
            color_preset: String::new(),
            available_modes: Vec::new(),
            is_disabled: false,
            active_workspace_id: 0,
        }
    }

    #[test]
    fn a_display_dropped_on_another_moves_to_the_nearest_free_side() {
        let left = screen("DP-1", 0, 0, 2560, 1440);
        let right = screen("DP-2", 2560, 0, 1920, 1080);
        assert_eq!(place_beside(&right, &[&left], 2000, 100), (2560, 100));
        assert_eq!(place_beside(&right, &[&left], 3000, 100), (3000, 100));
        assert_eq!(snap(2555.0, &[2560, 0], 10.0), 2560.0);
        assert_eq!(snap(2540.0, &[2560, 0], 10.0), 2540.0);
    }
}

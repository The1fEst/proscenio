use gtk4::gdk::RGBA;
use gtk4::prelude::*;
use gtk4::{graphene, gsk};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::services::displays::Monitor;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::text;

const HEIGHT: i32 = 240;
const FILL: f64 = 0.9;
const SNAP_DISTANCE: f64 = 10.0;
const DRAG_THRESHOLD: f64 = 10.0;

struct Drag {
    name: String,
    grab: (f64, f64),
    active: bool,
    placed: (i64, i64),
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
    selected: RefCell<String>,
    drag: RefCell<Option<Drag>>,
    moved: RefCell<Option<Box<dyn Fn(&str, i64, i64)>>>,
    picked: RefCell<Option<Box<dyn Fn(&str)>>>,
    hovering: Cell<bool>,
}

impl Arrangement {
    pub fn new(theme: &SharedTheme) -> Rc<Self> {
        let paint = Paint::new(|_, _, _| {});
        paint.set_size_request(-1, HEIGHT);
        paint.set_hexpand(true);
        let arrangement = Rc::new(Arrangement {
            paint: paint.clone(),
            monitors: RefCell::new(Vec::new()),
            selected: RefCell::new(String::new()),
            drag: RefCell::new(None),
            moved: RefCell::new(None),
            picked: RefCell::new(None),
            hovering: Cell::new(false),
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

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion({
            let arrangement = Rc::downgrade(&arrangement);
            move |_, x, y| {
                if let Some(arrangement) = arrangement.upgrade() {
                    let over = arrangement.plate_at(x, y).is_some();
                    if arrangement.hovering.replace(over) != over {
                        arrangement.paint.set_cursor_from_name(if over {
                            Some("grab")
                        } else {
                            None
                        });
                    }
                }
            }
        });
        paint.add_controller(motion);
        arrangement
    }

    pub fn connect_moved(&self, action: impl Fn(&str, i64, i64) + 'static) {
        self.moved.replace(Some(Box::new(action)));
    }

    pub fn connect_picked(&self, action: impl Fn(&str) + 'static) {
        self.picked.replace(Some(Box::new(action)));
    }

    pub fn set(&self, monitors: Vec<Monitor>, selected: &str) {
        self.monitors.replace(monitors);
        self.selected.replace(selected.to_owned());
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
        let zoom = (width / span_width.max(1.0)).min(height / span_height.max(1.0)) * FILL;
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

    fn shown_at(&self, layout: &Layout, monitor: &Monitor) -> (f64, f64) {
        if let Some(drag) = self.drag.borrow().as_ref()
            && drag.active
            && drag.name == monitor.name
        {
            return (
                (drag.placed.0 as f64 - layout.span_left) * layout.zoom + layout.origin_x,
                (drag.placed.1 as f64 - layout.span_top) * layout.zoom + layout.origin_y,
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
                let (left, top) = self.shown_at(&layout, monitor);
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
            grab: self.home(&layout, &monitor),
            active: false,
            placed: (monitor.x, monitor.y),
        }));
    }

    fn update(&self, dx: f64, dy: f64) {
        let layout = self.layout();
        let monitors = self.monitors.borrow().clone();
        let mut held = self.drag.borrow_mut();
        let Some(drag) = held.as_mut() else {
            return;
        };
        if !drag.active && dx.hypot(dy) < DRAG_THRESHOLD {
            return;
        }
        drag.active = true;
        let Some(plate) = monitors.iter().find(|m| m.name == drag.name) else {
            return;
        };
        let neighbours: Vec<&Monitor> = monitors.iter().filter(|m| m.name != plate.name).collect();
        let reach = SNAP_DISTANCE / layout.zoom;
        let mut lefts = Vec::new();
        let mut tops = Vec::new();
        for other in &neighbours {
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
        let px = drag.grab.0 + dx;
        let py = drag.grab.1 + dy;
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
        drag.placed = beside(plate, &neighbours, x.round() as i64, y.round() as i64);
        drop(held);
        self.paint.queue_draw();
    }

    fn end(&self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        self.paint.queue_draw();
        if drag.active {
            if let Some(moved) = self.moved.borrow().as_ref() {
                moved(&drag.name, drag.placed.0, drag.placed.1);
            }
        } else if let Some(picked) = self.picked.borrow().as_ref() {
            picked(&drag.name);
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
        let selected = self.selected.borrow().clone();
        let dragging = self
            .drag
            .borrow()
            .as_ref()
            .filter(|drag| drag.active)
            .map(|drag| drag.name.clone());
        for monitor in self.monitors.borrow().iter() {
            let (left, top) = self.shown_at(&layout, monitor);
            let bounds = graphene::Rect::new(
                left as f32,
                top as f32,
                (monitor.width as f64 * layout.zoom) as f32,
                (monitor.height as f64 * layout.zoom) as f32,
            );
            let shape = gsk::RoundedRect::from_rect(bounds, rounding::VERYSMALL as f32);
            let current = monitor.name == selected;
            let active = dragging.as_deref() == Some(monitor.name.as_str());
            let fill = if current {
                colors.col_secondary_container
            } else {
                colors.col_layer3
            };
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&fill, &bounds);
            snapshot.pop();
            let (line, border) = if active {
                (2.0, colors.col_primary)
            } else {
                (1.0, colors.col_outline_variant)
            };
            snapshot.append_border(&shape, &[line; 4], &[border; 4]);
            let label = if monitor.model.is_empty() {
                &monitor.name
            } else {
                &monitor.model
            };
            let colour = if current {
                colors.col_on_secondary_container
            } else {
                colors.col_on_layer3
            };
            centred_text(snapshot, &self.paint, label, colour, &bounds);
        }
    }
}

fn centred_text(
    snapshot: &gtk4::Snapshot,
    paint: &Paint,
    label: &str,
    colour: RGBA,
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
    snapshot.append_layout(&layout, &colour);
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

fn covered_by<'a>(
    plate: &Monitor,
    neighbours: &[&'a Monitor],
    x: i64,
    y: i64,
) -> Option<&'a Monitor> {
    neighbours
        .iter()
        .find(|other| {
            x < other.x + other.width
                && other.x < x + plate.width
                && y < other.y + other.height
                && other.y < y + plate.height
        })
        .copied()
}

fn beside(plate: &Monitor, neighbours: &[&Monitor], x: i64, y: i64) -> (i64, i64) {
    let (mut spot_x, mut spot_y) = (x, y);
    for _ in 0..=neighbours.len() {
        let Some(covered) = covered_by(plate, neighbours, spot_x, spot_y) else {
            break;
        };
        let mut sides = [
            (covered.x - plate.width, spot_y),
            (covered.x + covered.width, spot_y),
            (spot_x, covered.y - plate.height),
            (spot_x, covered.y + covered.height),
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
            disabled: false,
        }
    }

    #[test]
    fn a_display_dropped_on_another_moves_to_the_nearest_free_side() {
        let left = screen("DP-1", 0, 0, 2560, 1440);
        let right = screen("DP-2", 2560, 0, 1920, 1080);
        assert_eq!(beside(&right, &[&left], 2000, 100), (2560, 100));
        assert_eq!(beside(&right, &[&left], 3000, 100), (3000, 100));
        assert_eq!(snap(2555.0, &[2560, 0], 10.0), 2560.0);
        assert_eq!(snap(2540.0, &[2560, 0], 10.0), 2540.0);
    }
}

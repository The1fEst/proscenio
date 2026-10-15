use gtk4::gdk;
use serde_json::Value;

use crate::ui::anim::Tween;
use crate::ui::theme::{pixel_size, rounding};

const SPACING: f64 = 5.0;
const ICON_RATIO: f64 = 0.15;
const ICON_RATIO_CENTERED: f64 = 0.35;
const ICON_RATIO_COMPACT: f64 = 0.6;

#[derive(Clone)]
pub(super) struct Monitor {
    pub(super) id: i64,
    pub(super) name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    pub(super) scale: f64,
    transform: i64,
    reserved_insets: [f64; 4],
    pub(super) active_workspace: i32,
}

impl Monitor {
    pub(super) fn read(value: &Value) -> Option<Self> {
        let read_number = |key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let mut reserved_insets = [0.0; 4];
        if let Some(list) = value.get("reserved").and_then(Value::as_array) {
            for (slot, entry) in reserved_insets.iter_mut().zip(list) {
                *slot = entry.as_f64().unwrap_or(0.0);
            }
        }
        Some(Monitor {
            id: value.get("id")?.as_i64()?,
            name: value.get("name")?.as_str()?.to_owned(),
            x: read_number("x"),
            y: read_number("y"),
            width: read_number("width"),
            height: read_number("height"),
            scale: value.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
            transform: value.get("transform").and_then(Value::as_i64).unwrap_or(0),
            reserved_insets,
            active_workspace: value
                .pointer("/activeWorkspace/id")
                .and_then(Value::as_i64)
                .unwrap_or(1) as i32,
        })
    }

    fn span(&self) -> (f64, f64) {
        if self.transform & 1 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    pub(super) fn usable_size(&self) -> (f64, f64) {
        let (width, height) = self.span();
        (
            width / self.scale - self.reserved_insets[0] - self.reserved_insets[2],
            height / self.scale - self.reserved_insets[1] - self.reserved_insets[3],
        )
    }
}

#[derive(Clone)]
pub(super) struct Client {
    pub(super) address: String,
    position: (f64, f64),
    pub(super) size: (f64, f64),
    pub(super) workspace: i32,
    pub(super) monitor_id: i64,
    is_floating: bool,
    fullscreen_mode: i64,
    pub(super) is_xwayland: bool,
    pub(super) title: String,
    pub(super) class: String,
}

impl Client {
    pub(super) fn read(value: &Value) -> Option<Self> {
        if value.get("mapped").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let read_pair = |key: &str| {
            let list = value.get(key).and_then(Value::as_array);
            let read_component = |index: usize| {
                list.and_then(|list| list.get(index))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            };
            (read_component(0), read_component(1))
        };
        let read_string = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Some(Client {
            address: value.get("address")?.as_str()?.to_owned(),
            position: read_pair("at"),
            size: read_pair("size"),
            workspace: value.pointer("/workspace/id")?.as_i64()? as i32,
            monitor_id: value.get("monitor").and_then(Value::as_i64).unwrap_or(-1),
            is_floating: value.get("floating").and_then(Value::as_bool) == Some(true),
            fullscreen_mode: value.get("fullscreen").and_then(Value::as_i64).unwrap_or(0),
            is_xwayland: value.get("xwayland").and_then(Value::as_bool) == Some(true),
            title: read_string("title"),
            class: read_string("class"),
        })
    }

    fn depth(&self) -> i64 {
        1 + self.is_floating as i64 + self.fullscreen_mode * 2
    }

    pub(super) fn is_movable(&self) -> bool {
        self.is_floating && self.fullscreen_mode == 0
    }
}

#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub(super) cell: (f64, f64),
    pub(super) rows: i32,
    pub(super) columns: i32,
    pub(super) is_right_to_left: bool,
    pub(super) is_bottom_up: bool,
}

impl Layout {
    pub(super) fn row(&self, workspace: i32) -> i32 {
        let row = ((workspace - 1).div_euclid(self.columns)).rem_euclid(self.rows);
        if self.is_bottom_up {
            self.rows - row - 1
        } else {
            row
        }
    }

    pub(super) fn column(&self, workspace: i32) -> i32 {
        let column = (workspace - 1).rem_euclid(self.columns);
        if self.is_right_to_left {
            self.columns - column - 1
        } else {
            column
        }
    }

    pub(super) fn workspace_in_cell(&self, row: i32, column: i32) -> i32 {
        let row = if self.is_bottom_up {
            self.rows - row - 1
        } else {
            row
        };
        let column = if self.is_right_to_left {
            self.columns - column - 1
        } else {
            column
        };
        row * self.columns + column + 1
    }

    pub(super) fn origin(&self, row: i32, column: i32) -> (f64, f64) {
        (
            column as f64 * (self.cell.0 + SPACING),
            row as f64 * (self.cell.1 + SPACING),
        )
    }

    pub(super) fn size(&self) -> (f64, f64) {
        (
            self.columns as f64 * self.cell.0 + (self.columns - 1) as f64 * SPACING,
            self.rows as f64 * self.cell.1 + (self.rows - 1) as f64 * SPACING,
        )
    }

    pub(super) fn corners(&self, row: i32, column: i32) -> [f64; 4] {
        let (is_left, is_right) = (column == 0, column == self.columns - 1);
        let (is_top, is_bottom) = (row == 0, row == self.rows - 1);
        let pick_radius = |is_large: bool| {
            if is_large {
                rounding::LARGE as f64
            } else {
                rounding::VERYSMALL as f64
            }
        };
        [
            pick_radius(is_left && is_top),
            pick_radius(is_right && is_top),
            pick_radius(is_right && is_bottom),
            pick_radius(is_left && is_bottom),
        ]
    }
}

pub(super) struct Thumb {
    pub(super) client: Client,
    pub(super) target: [f64; 4],
    pub(super) offset: (f64, f64),
    pub(super) x: Tween,
    pub(super) y: Tween,
    pub(super) width: Tween,
    pub(super) height: Tween,
    pub(super) icon_size: Tween,
    pub(super) icon: Option<(i32, gdk::Paintable)>,
    pub(super) texture: Option<gdk::Texture>,
    pub(super) failures: u32,
}

impl Thumb {
    fn rect(&self, now: i64) -> [f64; 4] {
        [
            self.x.value(now),
            self.y.value(now),
            self.width.value(now),
            self.height.value(now),
        ]
    }

    pub(super) fn is_running(&self, now: i64) -> bool {
        [&self.x, &self.y, &self.width, &self.height, &self.icon_size]
            .iter()
            .any(|tween| tween.is_running(now))
    }
}

pub(super) struct Indicator {
    pub(super) x: Tween,
    pub(super) y: Tween,
    pub(super) corners: [Tween; 4],
}

pub(super) struct Drag {
    pub(super) address: String,
    pub(super) button: u32,
    pub(super) source_workspace: i32,
    pub(super) start: (f64, f64),
    pub(super) grab_offset: (f64, f64),
    pub(super) pointer: (f64, f64),
    pub(super) has_moved: bool,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) monitor: Option<Monitor>,
    pub(super) monitors: Vec<Monitor>,
    pub(super) layout: Option<Layout>,
    pub(super) scale: f64,
    pub(super) group: i32,
    pub(super) thumbs: Vec<Thumb>,
    pub(super) indicator: Option<Indicator>,
    pub(super) hovered_address: Option<String>,
    pub(super) drag: Option<Drag>,
    pub(super) target_workspace: Option<i32>,
}

impl State {
    pub(super) fn workspace_count(&self) -> i32 {
        self.layout
            .map(|layout| layout.rows * layout.columns)
            .unwrap_or(0)
    }

    pub(super) fn thumb_rect(&self, thumb: &Thumb, now: i64) -> [f64; 4] {
        let rect = thumb.rect(now);
        match &self.drag {
            Some(drag) if drag.has_moved && drag.address == thumb.client.address => [
                drag.pointer.0 - drag.grab_offset.0,
                drag.pointer.1 - drag.grab_offset.1,
                rect[2],
                rect[3],
            ],
            _ => rect,
        }
    }

    pub(super) fn order(&self) -> (Vec<usize>, usize) {
        let mut order: Vec<usize> = (0..self.thumbs.len()).collect();
        order.sort_by_key(|&index| self.thumbs[index].client.depth());
        if let Some(drag) = &self.drag
            && let Some(at) = order
                .iter()
                .position(|&index| self.thumbs[index].client.address == drag.address)
        {
            let dragged_index = order.remove(at);
            order.push(dragged_index);
        }
        let dragged_address = self.drag.as_ref().map(|drag| drag.address.as_str());
        let indicator_position = order
            .iter()
            .take_while(|&&index| {
                self.thumbs[index].client.depth() <= 1
                    && Some(self.thumbs[index].client.address.as_str()) != dragged_address
            })
            .count();
        (order, indicator_position)
    }

    pub(super) fn thumb_at(&self, x: f64, y: f64, now: i64) -> Option<usize> {
        let (order, _) = self.order();
        order.into_iter().rev().find(|&index| {
            let [left, top, width, height] = self.thumb_rect(&self.thumbs[index], now);
            x >= left && x < left + width && y >= top && y < top + height
        })
    }

    pub(super) fn workspace_at(&self, x: f64, y: f64) -> Option<i32> {
        let layout = self.layout?;
        for row in 0..layout.rows {
            for column in 0..layout.columns {
                let (left, top) = layout.origin(row, column);
                if x >= left && x < left + layout.cell.0 && y >= top && y < top + layout.cell.1 {
                    return Some(
                        self.group * self.workspace_count() + layout.workspace_in_cell(row, column),
                    );
                }
            }
        }
        None
    }
}

pub(super) fn place(
    client: &Client,
    window_monitor: &Monitor,
    widget_monitor: &Monitor,
    layout: &Layout,
    scale: f64,
) -> ([f64; 4], (f64, f64)) {
    let (width_ratio, height_ratio) = ratios(window_monitor, widget_monitor);
    let offset = layout.origin(
        layout.row(client.workspace),
        layout.column(client.workspace),
    );
    let x = ((client.position.0 - window_monitor.x - window_monitor.reserved_insets[0])
        * width_ratio
        * scale)
        .max(0.0);
    let y = ((client.position.1 - window_monitor.y - window_monitor.reserved_insets[1])
        * height_ratio
        * scale)
        .max(0.0);
    (
        [
            x + offset.0,
            y + offset.1,
            client.size.0 * scale * width_ratio,
            client.size.1 * scale * height_ratio,
        ],
        offset,
    )
}

pub(super) fn map_to_window_position(
    (left, top): (f64, f64),
    offset: (f64, f64),
    window_monitor: &Monitor,
    widget_monitor: &Monitor,
    scale: f64,
) -> (f64, f64) {
    let (width_ratio, height_ratio) = ratios(window_monitor, widget_monitor);
    (
        (left - offset.0) / (width_ratio * scale)
            + window_monitor.x
            + window_monitor.reserved_insets[0],
        (top - offset.1) / (height_ratio * scale)
            + window_monitor.y
            + window_monitor.reserved_insets[1],
    )
}

pub(super) fn clamp_to_usable_area(
    (x, y): (f64, f64),
    (width, height): (f64, f64),
    border_size: f64,
    monitor: &Monitor,
) -> (f64, f64) {
    let (usable_width, usable_height) = monitor.usable_size();
    let (left, top) = (
        monitor.x + monitor.reserved_insets[0] + border_size,
        monitor.y + monitor.reserved_insets[1] + border_size,
    );
    (
        x.min(left + usable_width - width - 2.0 * border_size)
            .max(left),
        y.min(top + usable_height - height - 2.0 * border_size)
            .max(top),
    )
}

fn ratios(window_monitor: &Monitor, widget_monitor: &Monitor) -> (f64, f64) {
    let (widget_width, widget_height) = widget_monitor.usable_size();
    let (monitor_width, monitor_height) = window_monitor.usable_size();
    (widget_width / monitor_width, widget_height / monitor_height)
}

pub(super) fn icon_size(target: &[f64; 4], is_centered: bool) -> f64 {
    let base = target[2].min(target[3]);
    let compact_limit = pixel_size::SMALLER as f64 * 4.0;
    let ratio = if compact_limit > target[2] || compact_limit > target[3] {
        ICON_RATIO_COMPACT
    } else if is_centered {
        ICON_RATIO_CENTERED
    } else {
        ICON_RATIO
    };
    base * ratio
}

pub(super) fn thumb_corners(thumb: &Thumb, layout: &Layout) -> [f64; 4] {
    let workspace = thumb.client.workspace;
    let base = layout.corners(layout.row(workspace), layout.column(workspace));
    let left = thumb.target[0] - thumb.offset.0;
    let top = thumb.target[1] - thumb.offset.1;
    let right = layout.cell.0 - (left + thumb.target[2]);
    let bottom = layout.cell.1 - (top + thumb.target[3]);
    let reach = [
        left.max(top),
        right.max(top),
        right.max(bottom),
        left.max(bottom),
    ];
    [0, 1, 2, 3].map(|corner| (base[corner] - reach[corner]).max(rounding::SMALL as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: i64, x: f64, size: (f64, f64), scale: f64) -> Monitor {
        Monitor::read(&serde_json::json!({
            "id": id,
            "name": format!("DP-{id}"),
            "x": x,
            "y": 0.0,
            "width": size.0,
            "height": size.1,
            "scale": scale,
            "reserved": [0.0, 45.0, 0.0, 0.0],
        }))
        .unwrap()
    }

    #[test]
    fn a_window_filling_a_scaled_monitor_fills_its_workspace_cell_on_every_monitor() {
        let main = monitor(0, 0.0, (2560.0, 1440.0), 1.0);
        let side = monitor(1, -1920.0, (2880.0, 1620.0), 1.5);
        let scale = 0.18;
        let client = Client::read(&serde_json::json!({
            "address": "0x1",
            "at": [-1920.0, 45.0],
            "size": [1920.0, 1035.0],
            "workspace": { "id": 1 },
            "monitor": 1,
        }))
        .unwrap();
        for widget_monitor in [&side, &main] {
            let (usable_width, usable_height) = widget_monitor.usable_size();
            let layout = Layout {
                cell: (usable_width * scale, usable_height * scale),
                rows: 2,
                columns: 5,
                is_right_to_left: false,
                is_bottom_up: false,
            };
            let (target, _) = place(&client, &side, widget_monitor, &layout, scale);
            assert!(
                (target[2] - layout.cell.0).abs() < 1e-6
                    && (target[3] - layout.cell.1).abs() < 1e-6,
                "{target:?} in a {:?} cell",
                layout.cell
            );
        }
    }

    #[test]
    fn a_dropped_window_is_kept_inside_the_usable_area_of_its_monitor() {
        let side = monitor(1, -1920.0, (2880.0, 1620.0), 1.5);
        let size = (800.0, 600.0);
        for border_size in [0.0, 2.0] {
            assert_eq!(
                clamp_to_usable_area((-1500.0, 400.0), size, border_size, &side),
                (-1500.0, 400.0)
            );
            assert_eq!(
                clamp_to_usable_area((-2000.0, 10.0), size, border_size, &side),
                (-1920.0 + border_size, 45.0 + border_size)
            );
            assert_eq!(
                clamp_to_usable_area((-500.0, 900.0), size, border_size, &side),
                (-800.0 - border_size, 480.0 - border_size)
            );
            assert_eq!(
                clamp_to_usable_area((-500.0, 900.0), (2000.0, 1200.0), border_size, &side),
                (-1920.0 + border_size, 45.0 + border_size)
            );
        }
    }

    #[test]
    fn only_floating_windows_that_are_not_fullscreen_are_moved_by_a_drop() {
        let is_movable = |is_floating: bool, fullscreen: i64| {
            Client::read(&serde_json::json!({
                "address": "0x1",
                "workspace": { "id": 1 },
                "floating": is_floating,
                "fullscreen": fullscreen,
            }))
            .unwrap()
            .is_movable()
        };
        assert!(is_movable(true, 0));
        assert!(!is_movable(false, 0));
        assert!(!is_movable(true, 1));
        assert!(!is_movable(true, 2));
    }

    #[test]
    fn a_dropped_floating_window_lands_where_its_thumbnail_was() {
        let main = monitor(0, 0.0, (2560.0, 1440.0), 1.0);
        let side = monitor(1, -1920.0, (2880.0, 1620.0), 1.5);
        let scale = 0.18;
        let (usable_width, usable_height) = main.usable_size();
        let layout = Layout {
            cell: (usable_width * scale, usable_height * scale),
            rows: 2,
            columns: 5,
            is_right_to_left: false,
            is_bottom_up: false,
        };
        for (window_monitor, position) in [(&main, (300.0, 240.0)), (&side, (-1500.0, 400.0))] {
            let client = Client::read(&serde_json::json!({
                "address": "0x1",
                "at": [position.0, position.1],
                "size": [800.0, 600.0],
                "workspace": { "id": 7 },
                "monitor": window_monitor.id,
                "floating": true,
            }))
            .unwrap();
            let (target, offset) = place(&client, window_monitor, &main, &layout, scale);
            let (x, y) = map_to_window_position(
                (target[0], target[1]),
                offset,
                window_monitor,
                &main,
                scale,
            );
            assert!(
                (x - position.0).abs() < 1e-6 && (y - position.1).abs() < 1e-6,
                "{position:?} came back as {:?}",
                (x, y)
            );
        }
    }
}

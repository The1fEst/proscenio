use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::platform::appicon;
use crate::platform::capture::Capture;
use crate::platform::hypr::{self, Events};
use crate::ui::anim::{EMPHASIZED_DECEL, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, mix, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::text::{self, Family};
use crate::ui::widgets::tooltip::{self, Tooltip};

const PADDING: i32 = 10;
const SPACING: f64 = 5.0;
const NUMBER_SIZE: f64 = 250.0;
const WORKSPACE_BORDER: f32 = 2.0;
const WINDOW_BORDER: f32 = 1.0;
const MOVE_MILLIS: f64 = 400.0;
const FAST_MILLIS: f64 = 200.0;
const ICON_GAP_RATIO: f64 = 0.06;
const ICON_RATIO: f64 = 0.15;
const ICON_RATIO_CENTRED: f64 = 0.35;
const ICON_RATIO_COMPACT: f64 = 0.6;
const OTHER_MONITOR_OPACITY: f64 = 0.4;
const LAST_WORKSPACE: i32 = 100;
const GIVE_UP: u32 = 3;
const QUIET_EVENTS: [&str; 3] = ["openlayer", "closelayer", "screencast"];

#[derive(Clone)]
struct Monitor {
    id: i64,
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
    transform: i64,
    reserved: [f64; 4],
    active: i32,
}

impl Monitor {
    fn read(value: &Value) -> Option<Self> {
        let number = |key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let mut reserved = [0.0; 4];
        if let Some(list) = value.get("reserved").and_then(Value::as_array) {
            for (slot, entry) in reserved.iter_mut().zip(list) {
                *slot = entry.as_f64().unwrap_or(0.0);
            }
        }
        Some(Monitor {
            id: value.get("id")?.as_i64()?,
            name: value.get("name")?.as_str()?.to_owned(),
            x: number("x"),
            y: number("y"),
            width: number("width"),
            height: number("height"),
            scale: value.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
            transform: value.get("transform").and_then(Value::as_i64).unwrap_or(0),
            reserved,
            active: value
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
}

#[derive(Clone)]
struct Client {
    address: String,
    at: (f64, f64),
    size: (f64, f64),
    workspace: i32,
    monitor: i64,
    floating: bool,
    fullscreen: i64,
    xwayland: bool,
    title: String,
    class: String,
}

impl Client {
    fn read(value: &Value) -> Option<Self> {
        if value.get("mapped").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let pair = |key: &str| {
            let list = value.get(key).and_then(Value::as_array);
            let at = |index: usize| {
                list.and_then(|list| list.get(index))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            };
            (at(0), at(1))
        };
        let string = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Some(Client {
            address: value.get("address")?.as_str()?.to_owned(),
            at: pair("at"),
            size: pair("size"),
            workspace: value.pointer("/workspace/id")?.as_i64()? as i32,
            monitor: value.get("monitor").and_then(Value::as_i64).unwrap_or(-1),
            floating: value.get("floating").and_then(Value::as_bool) == Some(true),
            fullscreen: value.get("fullscreen").and_then(Value::as_i64).unwrap_or(0),
            xwayland: value.get("xwayland").and_then(Value::as_bool) == Some(true),
            title: string("title"),
            class: string("class"),
        })
    }

    fn depth(&self) -> i64 {
        1 + self.floating as i64 + self.fullscreen * 2
    }
}

#[derive(Clone, Copy)]
struct Layout {
    cell: (f64, f64),
    rows: i32,
    columns: i32,
    right_left: bool,
    bottom_up: bool,
}

impl Layout {
    fn row(&self, workspace: i32) -> i32 {
        let normal = ((workspace - 1).div_euclid(self.columns)).rem_euclid(self.rows);
        if self.bottom_up {
            self.rows - normal - 1
        } else {
            normal
        }
    }

    fn column(&self, workspace: i32) -> i32 {
        let normal = (workspace - 1).rem_euclid(self.columns);
        if self.right_left {
            self.columns - normal - 1
        } else {
            normal
        }
    }

    fn workspace_in_cell(&self, row: i32, column: i32) -> i32 {
        let row = if self.bottom_up {
            self.rows - row - 1
        } else {
            row
        };
        let column = if self.right_left {
            self.columns - column - 1
        } else {
            column
        };
        row * self.columns + column + 1
    }

    fn origin(&self, row: i32, column: i32) -> (f64, f64) {
        (
            column as f64 * (self.cell.0 + SPACING),
            row as f64 * (self.cell.1 + SPACING),
        )
    }

    fn size(&self) -> (f64, f64) {
        (
            self.columns as f64 * self.cell.0 + (self.columns - 1) as f64 * SPACING,
            self.rows as f64 * self.cell.1 + (self.rows - 1) as f64 * SPACING,
        )
    }

    fn corners(&self, row: i32, column: i32) -> [f64; 4] {
        let (left, right) = (column == 0, column == self.columns - 1);
        let (top, bottom) = (row == 0, row == self.rows - 1);
        let pick = |large: bool| {
            if large {
                rounding::LARGE as f64
            } else {
                rounding::VERYSMALL as f64
            }
        };
        [
            pick(left && top),
            pick(right && top),
            pick(right && bottom),
            pick(left && bottom),
        ]
    }
}

struct Thumb {
    client: Client,
    target: [f64; 4],
    offset: (f64, f64),
    x: Tween,
    y: Tween,
    width: Tween,
    height: Tween,
    icon_size: Tween,
    icon: Option<(i32, gdk::Paintable)>,
    texture: Option<gdk::Texture>,
    failures: u32,
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

    fn running(&self, now: i64) -> bool {
        [&self.x, &self.y, &self.width, &self.height, &self.icon_size]
            .iter()
            .any(|tween| tween.running(now))
    }
}

struct Indicator {
    x: Tween,
    y: Tween,
    corners: [Tween; 4],
}

struct Drag {
    address: String,
    button: u32,
    from: i32,
    start: (f64, f64),
    grab: (f64, f64),
    pointer: (f64, f64),
    moved: bool,
}

#[derive(Default)]
struct State {
    monitor: Option<Monitor>,
    layout: Option<Layout>,
    scale: f64,
    group: i32,
    active: i32,
    thumbs: Vec<Thumb>,
    indicator: Option<Indicator>,
    hovered: Option<String>,
    drag: Option<Drag>,
    target: Option<i32>,
}

impl State {
    fn shown(&self) -> i32 {
        self.layout
            .map(|layout| layout.rows * layout.columns)
            .unwrap_or(0)
    }

    fn thumb_rect(&self, thumb: &Thumb, now: i64) -> [f64; 4] {
        let rect = thumb.rect(now);
        match &self.drag {
            Some(drag) if drag.moved && drag.address == thumb.client.address => [
                drag.pointer.0 - drag.grab.0,
                drag.pointer.1 - drag.grab.1,
                rect[2],
                rect[3],
            ],
            _ => rect,
        }
    }

    fn order(&self) -> (Vec<usize>, usize) {
        let mut order: Vec<usize> = (0..self.thumbs.len()).collect();
        order.sort_by_key(|&index| self.thumbs[index].client.depth());
        if let Some(drag) = &self.drag
            && let Some(at) = order
                .iter()
                .position(|&index| self.thumbs[index].client.address == drag.address)
        {
            let dragged = order.remove(at);
            order.push(dragged);
        }
        let dragged = self.drag.as_ref().map(|drag| drag.address.as_str());
        let indicator = order
            .iter()
            .take_while(|&&index| {
                self.thumbs[index].client.depth() <= 1
                    && Some(self.thumbs[index].client.address.as_str()) != dragged
            })
            .count();
        (order, indicator)
    }

    fn thumb_at(&self, x: f64, y: f64, now: i64) -> Option<usize> {
        let (order, _) = self.order();
        order.into_iter().rev().find(|&index| {
            let [left, top, width, height] = self.thumb_rect(&self.thumbs[index], now);
            x >= left && x < left + width && y >= top && y < top + height
        })
    }

    fn workspace_at(&self, x: f64, y: f64) -> Option<i32> {
        let layout = self.layout?;
        for row in 0..layout.rows {
            for column in 0..layout.columns {
                let (left, top) = layout.origin(row, column);
                if x >= left && x < left + layout.cell.0 && y >= top && y < top + layout.cell.1 {
                    return Some(self.group * self.shown() + layout.workspace_in_cell(row, column));
                }
            }
        }
        None
    }
}

pub struct Grid {
    pub widget: gtk4::Box,
    paint: Paint,
    config: Rc<Config>,
    theme: SharedTheme,
    connector: String,
    tooltip: Rc<Tooltip>,
    state: RefCell<State>,
    capture: RefCell<Option<Rc<Capture>>>,
    open: Cell<bool>,
    ticking: Cell<bool>,
    pending: Cell<bool>,
    on_close: RefCell<Option<Box<dyn Fn()>>>,
}

impl Grid {
    pub fn connect_close(&self, action: impl Fn() + 'static) {
        self.on_close.replace(Some(Box::new(action)));
    }

    pub fn open(self: &Rc<Self>) {
        if !self.config.overview_enable || self.open.replace(true) {
            return;
        }
        self.capture.replace(Capture::new(&self.paint));
        self.refresh();
    }

    pub fn close(&self) {
        if !self.open.replace(false) {
            return;
        }
        if let Some(capture) = self.capture.take() {
            capture.stop();
        }
        self.tooltip.show(false);
        self.state.replace(State::default());
    }

    fn request_close(&self) {
        if let Some(action) = self.on_close.borrow().as_ref() {
            action();
        }
    }

    fn schedule_refresh(self: &Rc<Self>) {
        if !self.open.get() || self.pending.replace(true) {
            return;
        }
        let grid = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(grid) = grid.upgrade() {
                grid.pending.set(false);
                grid.refresh();
            }
        });
    }

    fn refresh(self: &Rc<Self>) {
        if !self.open.get() {
            return;
        }
        let monitors: Vec<Monitor> = hypr::json("monitors")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(Monitor::read)
            .collect();
        let Some(monitor) = monitors
            .iter()
            .find(|monitor| monitor.name == self.connector)
            .cloned()
        else {
            return;
        };
        let clients: Vec<Client> = hypr::json("clients")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(Client::read)
            .collect();

        let scale = self.config.overview_scale;
        let (span_width, span_height) = monitor.span();
        let layout = Layout {
            cell: (
                (span_width - monitor.reserved[0] - monitor.reserved[2]) * scale / monitor.scale,
                (span_height - monitor.reserved[1] - monitor.reserved[3]) * scale / monitor.scale,
            ),
            rows: self.config.overview_rows.max(1),
            columns: self.config.overview_columns.max(1),
            right_left: self.config.overview_right_left,
            bottom_up: self.config.overview_bottom_up,
        };
        let (width, height) = layout.size();
        self.paint
            .set_size_request(width.ceil() as i32, height.ceil() as i32);

        let now = now(&self.paint);
        let shown = layout.rows * layout.columns;
        let active = monitor.active.clamp(1, LAST_WORKSPACE);
        let group = (active - 1) / shown;
        let mut started = Vec::new();
        {
            let mut state = self.state.borrow_mut();
            let mut kept = Vec::new();
            for client in clients {
                if client.workspace <= group * shown || client.workspace > (group + 1) * shown {
                    continue;
                }
                let Some(window_monitor) = monitors.iter().find(|entry| entry.id == client.monitor)
                else {
                    continue;
                };
                let (target, offset) = place(&client, window_monitor, &monitor, &layout, scale);
                let icon_target = icon_size(&target, self.config.overview_center_icons);
                let known = state
                    .thumbs
                    .iter()
                    .position(|thumb| thumb.client.address == client.address);
                let thumb = match known {
                    Some(at) => {
                        let mut thumb = state.thumbs.swap_remove(at);
                        for (tween, value) in [
                            &mut thumb.x,
                            &mut thumb.y,
                            &mut thumb.width,
                            &mut thumb.height,
                        ]
                        .into_iter()
                        .zip(target)
                        {
                            tween.retarget(value, now);
                        }
                        thumb.icon_size.retarget(icon_target, now);
                        thumb.client = client;
                        thumb.target = target;
                        thumb.offset = offset;
                        thumb
                    }
                    None => {
                        started.push(client.address.clone());
                        let tween = |value: f64| Tween::new(value, MOVE_MILLIS, EMPHASIZED_DECEL);
                        Thumb {
                            client,
                            target,
                            offset,
                            x: tween(target[0]),
                            y: tween(target[1]),
                            width: tween(target[2]),
                            height: tween(target[3]),
                            icon_size: tween(icon_target),
                            icon: None,
                            texture: None,
                            failures: 0,
                        }
                    }
                };
                kept.push(thumb);
            }
            for thumb in &mut kept {
                let size = thumb.icon_size.target().ceil().max(1.0) as i32;
                if thumb
                    .icon
                    .as_ref()
                    .is_some_and(|(loaded, _)| *loaded == size)
                {
                    continue;
                }
                thumb.icon = gdk::Display::default().map(|display| {
                    let theme = gtk4::IconTheme::for_display(&display);
                    let name = appicon::guess(&theme, &thumb.client.class);
                    (
                        size,
                        appicon::themed(
                            &theme,
                            &name,
                            "image-missing",
                            size,
                            self.paint.scale_factor(),
                        ),
                    )
                });
            }
            state.thumbs = kept;

            let (row, column) = (layout.row(active), layout.column(active));
            let (x, y) = layout.origin(row, column);
            let corners = layout.corners(row, column);
            match state.indicator.as_mut() {
                Some(indicator) => {
                    indicator.x.retarget(x, now);
                    indicator.y.retarget(y, now);
                    for (tween, value) in indicator.corners.iter_mut().zip(corners) {
                        tween.retarget(value, now);
                    }
                }
                None => {
                    state.indicator = Some(Indicator {
                        x: Tween::new(x, FAST_MILLIS, EXPRESSIVE_EFFECTS),
                        y: Tween::new(y, FAST_MILLIS, EXPRESSIVE_EFFECTS),
                        corners: corners
                            .map(|value| Tween::new(value, MOVE_MILLIS, EMPHASIZED_DECEL)),
                    });
                }
            }
            state.monitor = Some(monitor);
            state.layout = Some(layout);
            state.scale = scale;
            state.group = group;
            state.active = active;
        }
        for address in started {
            self.capture(address, true);
        }
        self.hover_changed();
        self.animate();
    }

    fn capture(self: &Rc<Self>, address: String, first: bool) {
        let Some(capture) = self.capture.borrow().clone() else {
            return;
        };
        let scale = self.paint.scale_factor().max(1) as f64;
        let Some(limit) = self
            .state
            .borrow()
            .thumbs
            .iter()
            .find(|thumb| thumb.client.address == address)
            .map(|thumb| {
                (
                    (thumb.target[2] * scale).ceil().max(1.0) as u32,
                    (thumb.target[3] * scale).ceil().max(1.0) as u32,
                )
            })
        else {
            return;
        };
        let grid = Rc::downgrade(self);
        let key = address.clone();
        let done = move |texture: Option<gdk::Texture>| {
            let Some(grid) = grid.upgrade() else {
                return;
            };
            if !grid.open.get() {
                return;
            }
            let again = {
                let mut state = grid.state.borrow_mut();
                let Some(thumb) = state
                    .thumbs
                    .iter_mut()
                    .find(|thumb| thumb.client.address == key)
                else {
                    return;
                };
                match texture {
                    Some(texture) => {
                        thumb.texture = Some(texture);
                        thumb.failures = 0;
                    }
                    None => thumb.failures += 1,
                }
                thumb.failures < GIVE_UP
            };
            grid.paint.queue_draw();
            if again {
                grid.capture(key, false);
            }
        };
        if first {
            capture.grab(&address, limit, done);
        } else {
            capture.next_frame(&address, limit, done);
        }
    }

    fn animate(self: &Rc<Self>) {
        self.paint.queue_draw();
        if self.ticking.replace(true) {
            return;
        }
        let grid = Rc::downgrade(self);
        self.paint.add_tick_callback(move |paint, clock| {
            let Some(grid) = grid.upgrade() else {
                return glib::ControlFlow::Break;
            };
            paint.queue_draw();
            if grid.running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            grid.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn running(&self, now: i64) -> bool {
        let state = self.state.borrow();
        state.thumbs.iter().any(|thumb| thumb.running(now))
            || state.indicator.as_ref().is_some_and(|indicator| {
                indicator.x.running(now)
                    || indicator.y.running(now)
                    || indicator.corners.iter().any(|tween| tween.running(now))
            })
    }

    fn hover(&self, point: Option<(f64, f64)>) {
        let now = now(&self.paint);
        let hovered = {
            let state = self.state.borrow();
            point
                .and_then(|(x, y)| state.thumb_at(x, y, now))
                .map(|index| state.thumbs[index].client.address.clone())
        };
        let changed = self.state.borrow().hovered != hovered;
        self.state.borrow_mut().hovered = hovered;
        if changed {
            self.hover_changed();
            self.paint.queue_draw();
        }
    }

    fn hover_changed(&self) {
        let now = now(&self.paint);
        let shown = {
            let state = self.state.borrow();
            let hovered = state.hovered.as_ref().filter(|_| state.drag.is_none());
            hovered
                .and_then(|address| {
                    state
                        .thumbs
                        .iter()
                        .find(|thumb| &thumb.client.address == address)
                })
                .map(|thumb| (state.thumb_rect(thumb, now), tooltip_text(&thumb.client)))
        };
        let Some(([x, y, width, height], text)) = shown else {
            self.tooltip.show(false);
            return;
        };
        self.tooltip.set_text(&text);
        self.tooltip.point_at(
            x.round() as i32,
            y.round() as i32,
            width.round() as i32,
            height.round() as i32,
        );
        self.tooltip.show(true);
    }

    fn press(self: &Rc<Self>, button: u32, x: f64, y: f64) {
        let now = now(&self.paint);
        let mut state = self.state.borrow_mut();
        if let Some(index) = state.thumb_at(x, y, now) {
            if button != gdk::BUTTON_PRIMARY && button != gdk::BUTTON_MIDDLE {
                return;
            }
            let [left, top, _, _] = state.thumb_rect(&state.thumbs[index], now);
            let client = &state.thumbs[index].client;
            state.drag = Some(Drag {
                address: client.address.clone(),
                button,
                from: client.workspace,
                start: (x, y),
                grab: (x - left, y - top),
                pointer: (x, y),
                moved: false,
            });
            state.target = state.workspace_at(x, y);
            drop(state);
            self.hover_changed();
            self.paint.queue_draw();
            return;
        }
        if button != gdk::BUTTON_PRIMARY {
            return;
        }
        let Some(workspace) = state.workspace_at(x, y) else {
            return;
        };
        drop(state);
        self.request_close();
        hypr::focus_workspace(workspace);
    }

    fn drag_to(&self, dx: f64, dy: f64) {
        let threshold = self.paint.settings().gtk_dnd_drag_threshold() as f64;
        let mut state = self.state.borrow_mut();
        let Some(drag) = state.drag.as_mut() else {
            return;
        };
        drag.pointer = (drag.start.0 + dx, drag.start.1 + dy);
        if !drag.moved {
            if dx.hypot(dy) < threshold {
                return;
            }
            drag.grab = (drag.grab.0 + dx, drag.grab.1 + dy);
        }
        drag.moved = true;
        let pointer = drag.pointer;
        state.target = state.workspace_at(pointer.0, pointer.1);
        drop(state);
        self.paint.queue_draw();
    }

    fn release(self: &Rc<Self>) {
        let now = now(&self.paint);
        let (drag, target, dropped) = {
            let mut state = self.state.borrow_mut();
            let Some(drag) = state.drag.take() else {
                return;
            };
            let target = state.target.take();
            let dropped = drag
                .moved
                .then(|| (drag.pointer.0 - drag.grab.0, drag.pointer.1 - drag.grab.1));
            (drag, target, dropped)
        };
        self.hover_changed();
        let Some((left, top)) = dropped else {
            self.paint.queue_draw();
            match drag.button {
                gdk::BUTTON_PRIMARY => {
                    self.request_close();
                    hypr::focus_window(&drag.address);
                }
                gdk::BUTTON_MIDDLE => hypr::close_window(&drag.address),
                _ => {}
            }
            return;
        };
        let moved = {
            let mut state = self.state.borrow_mut();
            let screen = state
                .monitor
                .as_ref()
                .map(|monitor| {
                    let (width, height) = monitor.span();
                    (width / monitor.scale, height / monitor.scale)
                })
                .unwrap_or_default();
            let cell = state.layout.map(|layout| layout.cell).unwrap_or((1.0, 1.0));
            let Some(thumb) = state
                .thumbs
                .iter_mut()
                .find(|thumb| thumb.client.address == drag.address)
            else {
                return;
            };
            thumb.x.jump(left);
            thumb.y.jump(top);
            thumb.x.retarget(thumb.target[0], now);
            thumb.y.retarget(thumb.target[1], now);
            match target {
                Some(workspace) if workspace != drag.from => Some((workspace, None)),
                _ if thumb.client.floating => Some((
                    drag.from,
                    Some((
                        (left - thumb.offset.0) / cell.0 * screen.0,
                        (top - thumb.offset.1) / cell.1 * screen.1,
                    )),
                )),
                _ => None,
            }
        };
        match moved {
            Some((workspace, None)) => hypr::move_to_workspace(&drag.address, workspace),
            Some((_, Some((x, y)))) => hypr::move_window(&drag.address, x, y),
            None => {}
        }
        self.refresh();
    }

    fn draw(&self, snapshot: &gtk4::Snapshot) {
        let now = now(&self.paint);
        let state = self.state.borrow();
        let (Some(monitor), Some(layout)) = (state.monitor.as_ref(), state.layout) else {
            return;
        };
        let theme = self.theme.borrow();
        let colours = &theme.colors;
        let shown = state.shown();
        let dragging = state.drag.as_ref().filter(|drag| drag.moved);
        let number_colour = transparentize(colours.col_on_layer1, 0.8);
        let mut number_font = text::font(
            Family::Expressive,
            NUMBER_SIZE * monitor.scale * state.scale,
            "wght=450",
        );
        number_font.set_weight(pango::Weight::Semibold);

        for row in 0..layout.rows {
            for column in 0..layout.columns {
                let workspace = state.group * shown + layout.workspace_in_cell(row, column);
                let (x, y) = layout.origin(row, column);
                let bounds = rect(x, y, layout.cell.0, layout.cell.1);
                let outline = rounded(&bounds, layout.corners(row, column));
                let hovered = dragging
                    .is_some_and(|drag| state.target == Some(workspace) && drag.from != workspace);
                let fill = if hovered {
                    mix(
                        colours.col_surface_container_low,
                        colours.col_layer1_hover,
                        0.1,
                    )
                } else {
                    colours.col_surface_container_low
                };
                snapshot.push_rounded_clip(&outline);
                snapshot.append_color(&fill, &bounds);
                snapshot.pop();
                if hovered {
                    snapshot.append_border(
                        &outline,
                        &[WORKSPACE_BORDER; 4],
                        &[colours.col_layer2_hover; 4],
                    );
                }
                let text = self.paint.create_pango_layout(Some(&workspace.to_string()));
                text.set_font_description(Some(&number_font));
                let (_, logical) = text.pixel_extents();
                let baseline = text.baseline() as f64 / gtk4::pango::SCALE as f64;
                let (height, ascent) = centred::layout_qt_metrics(&text)
                    .map(|(height, ascent)| (height as f64, ascent as f64))
                    .unwrap_or((logical.height() as f64, baseline));
                let left = x + ((layout.cell.0 - logical.width() as f64) / 2.0).round();
                let top = y + ((layout.cell.1 - height) / 2.0).round();
                snapshot.save();
                snapshot.translate(&graphene::Point::new(
                    left as f32,
                    (top + ascent - baseline) as f32,
                ));
                snapshot.append_layout(&text, &number_colour);
                snapshot.restore();
            }
        }

        let (order, indicator_at) = state.order();
        for (position, index) in order.iter().enumerate() {
            if position == indicator_at {
                self.draw_indicator(snapshot, &state, now);
            }
            self.draw_thumb(snapshot, &state, &state.thumbs[*index], monitor, now);
        }
        if indicator_at == order.len() {
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
            &rounded(&bounds, corners),
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
        let colours = &theme.colors;
        let [x, y, width, height] = state.thumb_rect(thumb, now);
        let (left, top) = (x.round(), y.round());
        let bounds = rect(
            left,
            top,
            (x + width).round() - left,
            (y + height).round() - top,
        );
        let outline = rounded(&bounds, thumb_corners(thumb, &layout));
        let pressed = state
            .drag
            .as_ref()
            .is_some_and(|drag| drag.address == thumb.client.address);
        let hovered = state.hovered.as_deref() == Some(thumb.client.address.as_str());
        let dimmed = thumb.client.monitor != monitor.id;
        if dimmed {
            snapshot.push_opacity(OTHER_MONITOR_OPACITY);
        }
        snapshot.push_rounded_clip(&outline);
        if let Some(texture) = &thumb.texture {
            snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Trilinear, &bounds);
        }
        let overlay = if pressed {
            Some(transparentize(colours.col_layer2_active, 0.5))
        } else if hovered {
            Some(transparentize(colours.col_layer2_hover, 0.7))
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
        if dimmed {
            snapshot.pop();
        }
    }
}

fn place(
    client: &Client,
    window_monitor: &Monitor,
    widget_monitor: &Monitor,
    layout: &Layout,
    scale: f64,
) -> ([f64; 4], (f64, f64)) {
    let (widget_width, widget_height) = widget_monitor.span();
    let (monitor_width, monitor_height) = window_monitor.span();
    let width_ratio =
        (widget_width * window_monitor.scale) / (monitor_width * widget_monitor.scale);
    let height_ratio =
        (widget_height * window_monitor.scale) / (monitor_height * widget_monitor.scale);
    let offset = layout.origin(
        layout.row(client.workspace),
        layout.column(client.workspace),
    );
    let x = ((client.at.0 - window_monitor.x - window_monitor.reserved[0]) * width_ratio * scale)
        .max(0.0);
    let y = ((client.at.1 - window_monitor.y - window_monitor.reserved[1]) * height_ratio * scale)
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

fn tooltip_text(client: &Client) -> String {
    let marker = if client.xwayland { "[XWayland] " } else { "" };
    format!("{}\n[{}] {marker}", client.title, client.class)
}

fn icon_size(target: &[f64; 4], centred: bool) -> f64 {
    let base = target[2].min(target[3]);
    let compact_limit = pixel_size::SMALLER as f64 * 4.0;
    let ratio = if compact_limit > target[2] || compact_limit > target[3] {
        ICON_RATIO_COMPACT
    } else if centred {
        ICON_RATIO_CENTRED
    } else {
        ICON_RATIO
    };
    base * ratio
}

fn thumb_corners(thumb: &Thumb, layout: &Layout) -> [f64; 4] {
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

fn rect(x: f64, y: f64, width: f64, height: f64) -> graphene::Rect {
    graphene::Rect::new(x as f32, y as f32, width as f32, height as f32)
}

fn rounded(bounds: &graphene::Rect, corners: [f64; 4]) -> gsk::RoundedRect {
    let size = |radius: f64| graphene::Size::new(radius as f32, radius as f32);
    gsk::RoundedRect::new(
        *bounds,
        size(corners[0]),
        size(corners[1]),
        size(corners[2]),
        size(corners[3]),
    )
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

pub fn build(
    config: &Rc<Config>,
    theme: &SharedTheme,
    events: &Events,
    connector: &str,
    scope: &Scope,
) -> Rc<Grid> {
    let paint = Paint::new(|_, _, _| {});
    paint.set_margin_top(PADDING);
    paint.set_margin_bottom(PADDING);
    paint.set_margin_start(PADDING);
    paint.set_margin_end(PADDING);
    paint.set_halign(gtk4::Align::Center);
    paint.set_valign(gtk4::Align::Center);

    let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    widget.add_css_class("overview-grid");
    widget.set_halign(gtk4::Align::Center);
    widget.append(&paint);
    widget.set_visible(config.overview_enable);

    let tooltip = Tooltip::new(&paint, theme, tooltip::Kind::Styled);
    tooltip.place_like_qt();

    let grid = Rc::new(Grid {
        widget,
        paint: paint.clone(),
        config: config.clone(),
        theme: theme.clone(),
        connector: connector.to_owned(),
        tooltip,
        state: RefCell::new(State::default()),
        capture: RefCell::new(None),
        open: Cell::new(false),
        ticking: Cell::new(false),
        pending: Cell::new(false),
        on_close: RefCell::new(None),
    });

    paint.set_draw({
        let grid = Rc::downgrade(&grid);
        move |snapshot, _, _| {
            if let Some(grid) = grid.upgrade() {
                grid.draw(snapshot);
            }
        }
    });

    let motion = gtk4::EventControllerMotion::new();
    motion.connect_motion({
        let grid = Rc::downgrade(&grid);
        move |_, x, y| {
            if let Some(grid) = grid.upgrade() {
                grid.hover(Some((x, y)));
            }
        }
    });
    motion.connect_leave({
        let grid = Rc::downgrade(&grid);
        move |_| {
            if let Some(grid) = grid.upgrade() {
                grid.hover(None);
            }
        }
    });
    paint.add_controller(motion);

    let drag = gtk4::GestureDrag::new();
    drag.set_button(0);
    drag.connect_drag_begin({
        let grid = Rc::downgrade(&grid);
        move |gesture, x, y| {
            if let Some(grid) = grid.upgrade() {
                grid.press(gesture.current_button(), x, y);
            }
        }
    });
    drag.connect_drag_update({
        let grid = Rc::downgrade(&grid);
        move |_, dx, dy| {
            if let Some(grid) = grid.upgrade() {
                grid.drag_to(dx, dy);
            }
        }
    });
    drag.connect_drag_end({
        let grid = Rc::downgrade(&grid);
        move |_, _, _| {
            if let Some(grid) = grid.upgrade() {
                grid.release();
            }
        }
    });
    paint.add_controller(drag);

    scope.keep(events.subscribe({
        let grid = Rc::downgrade(&grid);
        move |event, _| {
            if QUIET_EVENTS.contains(&event) {
                return;
            }
            if let Some(grid) = grid.upgrade() {
                grid.schedule_refresh();
            }
        }
    }));

    grid
}

mod model;
mod renderer;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::platform::hyprland::hypr::{self, Events};
use crate::platform::system::appicon;
use crate::platform::wayland::capture::Capture;
use crate::ui::anim::{EMPHASIZED_DECEL, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};
use model::{
    Client, Drag, Indicator, Layout, Monitor, State, Thumb, clamp_to_usable_area, icon_size,
    map_to_window_position, place,
};

const PADDING: i32 = 10;
const MOVE_MILLIS: f64 = 400.0;
const FAST_MILLIS: f64 = 200.0;
const LAST_WORKSPACE: i32 = 100;
const MAX_FAILURES: u32 = 3;
const QUIET_EVENTS: [&str; 3] = ["openlayer", "closelayer", "screencast"];

pub struct Grid {
    pub widget: gtk4::Box,
    paint: Paint,
    config: Rc<Config>,
    theme: SharedTheme,
    connector: String,
    tooltip: Rc<Tooltip>,
    state: RefCell<State>,
    capture: RefCell<Option<Rc<Capture>>>,
    is_open: Cell<bool>,
    is_ticking: Cell<bool>,
    is_refresh_pending: Cell<bool>,
    on_close: RefCell<Option<Box<dyn Fn()>>>,
}

impl Grid {
    pub fn connect_close(&self, action: impl Fn() + 'static) {
        self.on_close.replace(Some(Box::new(action)));
    }

    pub fn open(self: &Rc<Self>) {
        if !self.config.overview_enable || self.is_open.replace(true) {
            return;
        }
        self.capture.replace(Capture::new(&self.paint));
        self.refresh();
    }

    pub fn close(&self) {
        if !self.is_open.replace(false) {
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
        if !self.is_open.get() || self.is_refresh_pending.replace(true) {
            return;
        }
        let grid = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(grid) = grid.upgrade() {
                grid.is_refresh_pending.set(false);
                grid.refresh();
            }
        });
    }

    fn refresh(self: &Rc<Self>) {
        if !self.is_open.get() {
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
        let (usable_width, usable_height) = monitor.usable_size();
        let layout = Layout {
            cell: (usable_width * scale, usable_height * scale),
            rows: self.config.overview_rows.max(1),
            columns: self.config.overview_columns.max(1),
            is_right_to_left: self.config.overview_right_left,
            is_bottom_up: self.config.overview_bottom_up,
        };
        let (width, height) = layout.size();
        self.paint
            .set_size_request(width.ceil() as i32, height.ceil() as i32);

        let now = now(&self.paint);
        let workspace_count = layout.rows * layout.columns;
        let active_workspace = monitor.active_workspace.clamp(1, LAST_WORKSPACE);
        let group = (active_workspace - 1) / workspace_count;
        let mut new_addresses = Vec::new();
        {
            let mut state = self.state.borrow_mut();
            let mut thumbs = Vec::new();
            for client in clients {
                if client.workspace <= group * workspace_count
                    || client.workspace > (group + 1) * workspace_count
                {
                    continue;
                }
                let Some(window_monitor) =
                    monitors.iter().find(|entry| entry.id == client.monitor_id)
                else {
                    continue;
                };
                let (target, offset) = place(&client, window_monitor, &monitor, &layout, scale);
                let icon_target = icon_size(&target, self.config.overview_center_icons);
                let previous_index = state
                    .thumbs
                    .iter()
                    .position(|thumb| thumb.client.address == client.address);
                let thumb = match previous_index {
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
                        new_addresses.push(client.address.clone());
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
                thumbs.push(thumb);
            }
            for thumb in &mut thumbs {
                let size = thumb.icon_size.target().ceil().max(1.0) as i32;
                if thumb
                    .icon
                    .as_ref()
                    .is_some_and(|(current_size, _)| *current_size == size)
                {
                    continue;
                }
                thumb.icon = gdk::Display::default().map(|display| {
                    let theme = gtk4::IconTheme::for_display(&display);
                    let name = appicon::guess(&theme, &thumb.client.class);
                    (
                        size,
                        appicon::load_icon(
                            &theme,
                            &name,
                            "image-missing",
                            size,
                            self.paint.scale_factor(),
                        ),
                    )
                });
            }
            state.thumbs = thumbs;

            let (row, column) = (
                layout.row(active_workspace),
                layout.column(active_workspace),
            );
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
            state.monitors = monitors;
            state.layout = Some(layout);
            state.scale = scale;
            state.group = group;
        }
        for address in new_addresses {
            self.capture(address, true);
        }
        self.update_tooltip();
        self.animate();
    }

    fn capture(self: &Rc<Self>, address: String, is_first_frame: bool) {
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
        let on_frame = move |texture: Option<gdk::Texture>| {
            let Some(grid) = grid.upgrade() else {
                return;
            };
            if !grid.is_open.get() {
                return;
            }
            let should_retry = {
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
                thumb.failures < MAX_FAILURES
            };
            grid.paint.queue_draw();
            if should_retry {
                grid.capture(key, false);
            }
        };
        if is_first_frame {
            capture.grab(&address, limit, on_frame);
        } else {
            capture.next_frame(&address, limit, on_frame);
        }
    }

    fn animate(self: &Rc<Self>) {
        self.paint.queue_draw();
        if self.is_ticking.replace(true) {
            return;
        }
        let grid = Rc::downgrade(self);
        self.paint.add_tick_callback(move |paint, clock| {
            let Some(grid) = grid.upgrade() else {
                return glib::ControlFlow::Break;
            };
            paint.queue_draw();
            if grid.is_running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            grid.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn is_running(&self, now: i64) -> bool {
        let state = self.state.borrow();
        state.thumbs.iter().any(|thumb| thumb.is_running(now))
            || state.indicator.as_ref().is_some_and(|indicator| {
                indicator.x.is_running(now)
                    || indicator.y.is_running(now)
                    || indicator.corners.iter().any(|tween| tween.is_running(now))
            })
    }

    fn hover(&self, point: Option<(f64, f64)>) {
        let now = now(&self.paint);
        let hovered_address = {
            let state = self.state.borrow();
            point
                .and_then(|(x, y)| state.thumb_at(x, y, now))
                .map(|index| state.thumbs[index].client.address.clone())
        };
        let has_changed = self.state.borrow().hovered_address != hovered_address;
        self.state.borrow_mut().hovered_address = hovered_address;
        if has_changed {
            self.update_tooltip();
            self.paint.queue_draw();
        }
    }

    fn update_tooltip(&self) {
        let now = now(&self.paint);
        let tooltip_target = {
            let state = self.state.borrow();
            let hovered_address = state
                .hovered_address
                .as_ref()
                .filter(|_| state.drag.is_none());
            hovered_address
                .and_then(|address| {
                    state
                        .thumbs
                        .iter()
                        .find(|thumb| &thumb.client.address == address)
                })
                .map(|thumb| (state.thumb_rect(thumb, now), tooltip_text(&thumb.client)))
        };
        let Some(([x, y, width, height], text)) = tooltip_target else {
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
                source_workspace: client.workspace,
                start: (x, y),
                grab_offset: (x - left, y - top),
                pointer: (x, y),
                has_moved: false,
            });
            state.target_workspace = state.workspace_at(x, y);
            drop(state);
            self.update_tooltip();
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
        if !drag.has_moved {
            if dx.hypot(dy) < threshold {
                return;
            }
            drag.grab_offset = (drag.grab_offset.0 + dx, drag.grab_offset.1 + dy);
        }
        drag.has_moved = true;
        let pointer = drag.pointer;
        state.target_workspace = state.workspace_at(pointer.0, pointer.1);
        drop(state);
        self.paint.queue_draw();
    }

    fn release(self: &Rc<Self>) {
        let now = now(&self.paint);
        let (drag, target_workspace, drop_position) = {
            let mut state = self.state.borrow_mut();
            let Some(drag) = state.drag.take() else {
                return;
            };
            let target_workspace = state.target_workspace.take();
            let drop_position = drag.has_moved.then(|| {
                (
                    drag.pointer.0 - drag.grab_offset.0,
                    drag.pointer.1 - drag.grab_offset.1,
                )
            });
            (drag, target_workspace, drop_position)
        };
        self.update_tooltip();
        let Some((left, top)) = drop_position else {
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
        let move_target = {
            let mut state = self.state.borrow_mut();
            let state = &mut *state;
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
            let window_monitor = state
                .monitors
                .iter()
                .find(|monitor| monitor.id == thumb.client.monitor_id);
            match (target_workspace, window_monitor, state.monitor.as_ref()) {
                (Some(workspace), _, _) if workspace != drag.source_workspace => {
                    Some((workspace, None))
                }
                (_, Some(window_monitor), Some(widget_monitor)) if thumb.client.is_movable() => {
                    let border_size =
                        hypr::request(&format!("getprop address:{} border_size", drag.address))
                            .and_then(|reply| reply.trim().parse().ok())
                            .or_else(|| hypr::option_int("general:border_size"))
                            .unwrap_or(0);
                    Some((
                        drag.source_workspace,
                        Some(clamp_to_usable_area(
                            map_to_window_position(
                                (left, top),
                                thumb.offset,
                                window_monitor,
                                widget_monitor,
                                state.scale,
                            ),
                            thumb.client.size,
                            border_size as f64,
                            window_monitor,
                        )),
                    ))
                }
                _ => None,
            }
        };
        match move_target {
            Some((workspace, None)) => hypr::move_to_workspace(&drag.address, workspace),
            Some((_, Some((x, y)))) => hypr::move_window(&drag.address, x, y),
            None => {}
        }
        self.refresh();
    }
}

fn tooltip_text(client: &Client) -> String {
    let marker = if client.is_xwayland {
        "[XWayland] "
    } else {
        ""
    };
    format!("{}\n[{}] {marker}", client.title, client.class)
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
        is_open: Cell::new(false),
        is_ticking: Cell::new(false),
        is_refresh_pending: Cell::new(false),
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

    let motion_controller = gtk4::EventControllerMotion::new();
    motion_controller.connect_motion({
        let grid = Rc::downgrade(&grid);
        move |_, x, y| {
            if let Some(grid) = grid.upgrade() {
                grid.hover(Some((x, y)));
            }
        }
    });
    motion_controller.connect_leave({
        let grid = Rc::downgrade(&grid);
        move |_| {
            if let Some(grid) = grid.upgrade() {
                grid.hover(None);
            }
        }
    });
    paint.add_controller(motion_controller);

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

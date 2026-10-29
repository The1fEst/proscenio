use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::taskbar::{DotStyle, SEPARATOR, Window, choose_dot_styles, targets};
use super::{Dock, ELEVATION, GAPS_OUT, MOVE_MILLIS, Metrics, ROW_PADDING, refresh_docks};
use crate::platform::hyprland::hypr;
use crate::platform::system::desktop;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;

const SPRING_LOAD_DELAY: Duration = Duration::from_millis(250);
const BUTTON_PADDING: i32 = 8;
const DOT_WIDTH: i32 = 10;
const DOT_HEIGHT: i32 = 4;
const DOT_SPACING: i32 = 3;
const DOT_GAP: f64 = 2.0;
const MAX_DOTS: usize = 3;
const SEPARATOR_INSET: i32 = ELEVATION + ROW_PADDING + rounding::NORMAL;
const DRAG_THRESHOLD: f64 = 10.0;
const DESATURATION: f32 = 0.8;
const OVERLAY: f32 = 0.1;

pub(super) struct AppButton {
    pub(super) id: String,
    pub(super) button: RippleButton,
    background: i32,
    pub(super) windows: RefCell<Vec<Window>>,
    cycle_index: Cell<i64>,
    dots: gtk4::Box,
    pub(super) x: Cell<Tween>,
    pub(super) is_placed: Cell<bool>,
}

impl AppButton {
    pub(super) fn is_separator(&self) -> bool {
        self.id == SEPARATOR
    }

    pub(super) fn width(&self) -> f64 {
        if self.is_separator() {
            1.0
        } else {
            self.background as f64
        }
    }
}

impl Dock {
    fn open_preview(&self, entry: &AppButton) {
        let count = entry.windows.borrow().len() as i64;
        if count == 0 {
            return;
        }
        entry.cycle_index.set(count - 1);
        let center = entry
            .button
            .compute_point(
                &self.root,
                &gtk4::graphene::Point::new(entry.button.width() as f32 / 2.0, 0.0),
            )
            .map(|point| point.x() as i32)
            .unwrap_or(0);
        self.preview
            .enter(&entry.id, center, targets(&entry.windows.borrow()));
    }

    pub(super) fn make_button(self: &Rc<Self>, id: &str) -> Rc<AppButton> {
        let button = RippleButton::new(&self.theme);
        button.set_radius(rounding::NORMAL as f64);
        let background = self.metrics.background();
        button.set_background_size(background as f32, background as f32);
        let dots = gtk4::Box::new(gtk4::Orientation::Horizontal, DOT_SPACING);
        let is_separator = id == SEPARATOR;
        if is_separator {
            button.set_size_request(1, self.metrics.button_height());
            button.set_sensitive(false);
            let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            holder.append(&separator(SEPARATOR_INSET, SEPARATOR_INSET));
            button.set_content(&holder, 0, 0);
        } else {
            button.set_size_request(background, self.metrics.button_height());
            button.set_content(&self.icon(id, &dots), 0, 0);
        }
        let entry = Rc::new(AppButton {
            id: id.to_owned(),
            button: button.clone(),
            background,
            windows: RefCell::new(Vec::new()),
            cycle_index: Cell::new(-1),
            dots,
            x: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_EFFECTS)),
            is_placed: Cell::new(false),
        });
        self.apps.put(&button, 0.0, 0.0);
        if is_separator {
            return entry;
        }

        let hover = gtk4::EventControllerMotion::new();
        hover.connect_enter({
            let dock = Rc::downgrade(self);
            let entry = Rc::downgrade(&entry);
            move |_, _, _| {
                if let (Some(dock), Some(entry)) = (dock.upgrade(), entry.upgrade()) {
                    dock.open_preview(&entry);
                }
            }
        });
        hover.connect_leave({
            let dock = Rc::downgrade(self);
            let id = id.to_owned();
            move |_| {
                if let Some(dock) = dock.upgrade() {
                    dock.preview.leave(&id);
                }
            }
        });
        button.add_controller(hover);

        let drag_over = spring_load(&button, {
            let dock = Rc::downgrade(self);
            let entry = Rc::downgrade(&entry);
            move || {
                let (Some(dock), Some(entry)) = (dock.upgrade(), entry.upgrade()) else {
                    return;
                };
                let windows = entry.windows.borrow();
                match windows.as_slice() {
                    [] => {}
                    [window] => hypr::focus_window(&window.address),
                    _ => {
                        drop(windows);
                        dock.open_preview(&entry);
                    }
                }
            }
        });
        drag_over.connect_leave({
            let dock = Rc::downgrade(self);
            let id = id.to_owned();
            move |_| {
                if let Some(dock) = dock.upgrade() {
                    dock.preview.leave(&id);
                }
            }
        });

        let file_drop = gtk4::DropTarget::new(
            gdk::FileList::static_type(),
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );
        file_drop.connect_enter(|_, _, _| gdk::DragAction::COPY);
        file_drop.connect_motion(|_, _, _| gdk::DragAction::COPY);
        file_drop.connect_drop({
            let id = id.to_owned();
            move |_, value, _, _| {
                let Ok(list) = value.get::<gdk::FileList>() else {
                    return false;
                };
                let Some(entry) = desktop::find(&id) else {
                    return false;
                };
                desktop::launch_with(&entry, &list.files());
                true
            }
        });
        button.add_controller(file_drop);

        button.connect_clicked({
            let entry = Rc::downgrade(&entry);
            move |_| {
                let Some(entry) = entry.upgrade() else {
                    return;
                };
                let windows = entry.windows.borrow();
                if windows.is_empty() {
                    launch(&entry.id);
                    return;
                }
                let next = (entry.cycle_index.get() + 1).rem_euclid(windows.len() as i64);
                entry.cycle_index.set(next);
                hypr::focus_window(&windows[next as usize].address);
            }
        });
        button.connect_middle({
            let id = id.to_owned();
            move |_| launch(&id)
        });
        button.connect_alt({
            let dock = Rc::downgrade(self);
            let id = id.to_owned();
            move |_| {
                let Some(dock) = dock.upgrade() else {
                    return;
                };
                dock.config.toggle_dock_pin(&id);
                refresh_docks();
            }
        });

        let drag = gtk4::GestureDrag::new();
        drag.connect_drag_update({
            let dock = Rc::downgrade(self);
            let entry = Rc::downgrade(&entry);
            move |gesture, offset, _| {
                let (Some(dock), Some(entry)) = (dock.upgrade(), entry.upgrade()) else {
                    return;
                };
                if !dock.drag.get().is_dragging {
                    if offset.abs() < DRAG_THRESHOLD {
                        return;
                    }
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    dock.begin_drag(&entry);
                }
                let Some((x, y)) = gesture.point(gesture.current_sequence().as_ref()) else {
                    return;
                };
                let Some(row) = entry
                    .button
                    .compute_point(&dock.apps, &gtk4::graphene::Point::new(x as f32, y as f32))
                else {
                    return;
                };
                dock.update_drag(row.x() as f64);
            }
        });
        drag.connect_drag_end({
            let dock = Rc::downgrade(self);
            move |_, _, _| {
                if let Some(dock) = dock.upgrade()
                    && dock.drag.get().is_dragging
                {
                    dock.commit_drag();
                }
            }
        });
        button.add_controller(drag);
        entry
    }

    fn icon(&self, id: &str, dots: &gtk4::Box) -> gtk4::Widget {
        let class = id.to_owned();
        let theme = self.theme.clone();
        let monochrome = self.config.dock_monochrome;
        let icon_size = self.metrics.icon_size;
        let image = Paint::new(move |snapshot, width, height| {
            let Some(display) = gdk::Display::default() else {
                return;
            };
            let icons = gtk4::IconTheme::for_display(&display);
            let paintable = crate::platform::system::appicon::load_icon(
                &icons,
                &crate::platform::system::appicon::guess(&icons, &class),
                "image-missing",
                icon_size,
                1,
            );
            let (width, height) = (width as f64, height as f64);
            if monochrome {
                let (matrix, offset) = monochrome_matrix(theme.borrow().colors.col_primary);
                snapshot.push_color_matrix(&matrix, &offset);
                paintable.snapshot(snapshot, width, height);
                snapshot.pop();
            } else {
                paintable.snapshot(snapshot, width, height);
            }
        });
        image.set_size_request(self.metrics.background() - BUTTON_PADDING * 2, icon_size);

        dots.set_halign(gtk4::Align::Center);
        let content = gtk4::Overlay::new();
        content.set_child(Some(&image));
        content.add_overlay(dots);
        content.connect_get_child_position({
            let dots = dots.clone().upcast::<gtk4::Widget>();
            move |content, child| {
                if *child != dots {
                    return None;
                }
                let width = child.measure(gtk4::Orientation::Horizontal, -1).1;
                Some(gdk::Rectangle::new(
                    (content.width() - width) / 2,
                    (icon_size as f64 + DOT_GAP) as i32,
                    width,
                    DOT_HEIGHT,
                ))
            }
        });
        Centered::integral(&content).upcast()
    }

    pub(super) fn update_dots(&self, entry: &AppButton) {
        if entry.is_separator() {
            return;
        }
        while let Some(child) = entry.dots.first_child() {
            entry.dots.remove(&child);
        }
        let windows = entry.windows.borrow();
        for style in choose_dot_styles(&windows, MAX_DOTS) {
            let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            dot.add_css_class(match style {
                DotStyle::Urgent => "dock-dot-urgent",
                DotStyle::Active => "dock-dot-active",
                DotStyle::Idle => "dock-dot",
                DotStyle::Minimized => "dock-dot-minimized",
            });
            dot.set_size_request(
                if windows.len() <= MAX_DOTS {
                    DOT_WIDTH
                } else {
                    DOT_HEIGHT
                },
                DOT_HEIGHT,
            );
            entry.dots.append(&dot);
        }
    }
}

pub(super) fn overview_button(theme: &SharedTheme, metrics: Metrics) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_look(Look::default());
    button.set_radius(rounding::NORMAL as f64);
    let background = metrics.background();
    button.set_size_request(background, metrics.button_height());
    button.set_background_size(background as f32, background as f32);
    button.set_margin_top(GAPS_OUT);
    button.set_valign(gtk4::Align::Fill);
    let symbol = text::symbol("apps", (background / 2) as f64);
    text::set_color(&symbol, "colOnLayer0");
    button.set_content(&Centered::new(&symbol), 0, 0);
    button
}

pub(super) fn separator(top: i32, bottom: i32) -> gtk4::Widget {
    let line = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    line.add_css_class("dock-separator");
    line.set_size_request(1, -1);
    line.set_margin_top(top);
    line.set_margin_bottom(bottom);
    line.set_valign(gtk4::Align::Fill);
    line.upcast()
}

fn monochrome_matrix(tint: gdk::RGBA) -> (gtk4::graphene::Matrix, gtk4::graphene::Vec4) {
    let keep = 1.0 - DESATURATION;
    let base = 1.0 - OVERLAY;
    let gray = [0.299, 0.587, 0.114].map(|weight| weight * DESATURATION * base);
    let own = keep * base;
    let matrix = gtk4::graphene::Matrix::from_float([
        gray[0] + own,
        gray[0],
        gray[0],
        0.0,
        gray[1],
        gray[1] + own,
        gray[1],
        0.0,
        gray[2],
        gray[2],
        gray[2] + own,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]);
    let offset = gtk4::graphene::Vec4::new(
        tint.red() * OVERLAY,
        tint.green() * OVERLAY,
        tint.blue() * OVERLAY,
        0.0,
    );
    (matrix, offset)
}

fn launch(id: &str) {
    if let Some(entry) = desktop::find(id) {
        desktop::launch(&entry);
    }
}

pub(super) fn spring_load(
    widget: &impl IsA<gtk4::Widget>,
    action: impl Fn() + 'static,
) -> gtk4::DropControllerMotion {
    let controller = gtk4::DropControllerMotion::new();
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let action = Rc::new(action);
    controller.connect_enter({
        let timer = timer.clone();
        move |_, _, _| {
            if let Some(previous_source) = timer.take() {
                previous_source.remove();
            }
            let source = glib::timeout_add_local_once(SPRING_LOAD_DELAY, {
                let (timer, action) = (timer.clone(), action.clone());
                move || {
                    timer.take();
                    action();
                }
            });
            timer.replace(Some(source));
        }
    });
    controller.connect_leave(move |_| {
        if let Some(pending_source) = timer.take() {
            pending_source.remove();
        }
    });
    widget.add_controller(controller.clone());
    controller
}

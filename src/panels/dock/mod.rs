pub mod preview;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::actions;
use crate::core::config::{self, Config};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::panels::dock::preview::{Preview, Target};
use crate::platform::desktop;
use crate::platform::hypr;
use crate::services::Services;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Tween};
use crate::ui::reserve::Reserve;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::coalesce;
use crate::ui::widgets::group::GroupButton;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text::{self, Shift};

const NAMESPACE: &str = "proscenio:dock";
const TRIGGER_NAMESPACE: &str = "proscenio:dockTrigger";
const RESERVE_NAMESPACE: &str = "proscenio:dockReserve";
const ELEVATION: i32 = 10;
const GAPS_OUT: i32 = 5;
const ROW_SPACING: i32 = 3;
const ROW_PADDING: i32 = 5;
const APP_SPACING: f64 = 2.0;
const PIN_SIZE: f64 = 35.0;
const BUTTON_BACKGROUND: i32 = 50;
const BUTTON_INSET: i32 = GAPS_OUT + ROW_PADDING;
const BUTTON_HEIGHT: i32 = BUTTON_BACKGROUND + BUTTON_INSET * 2;
const BUTTON_PADDING: i32 = 8;
const ICON_SIZE: i32 = 35;
const DOT_WIDTH: i32 = 10;
const DOT_HEIGHT: i32 = 4;
const DOT_SPACING: i32 = 3;
const DOT_GAP: f64 = 2.0;
const MAX_DOTS: usize = 3;
const SEPARATOR: &str = "SEPARATOR";
const SEPARATOR_INSET: i32 = ELEVATION + ROW_PADDING + rounding::NORMAL;
const DRAG_THRESHOLD: f64 = 10.0;
const DRAGGED_OPACITY: f64 = 0.85;
const MOVE_MILLIS: f64 = 200.0;
const DESATURATION: f32 = 0.8;
const OVERLAY: f32 = 0.1;

thread_local! {
    static PINNED: RefCell<Option<Rc<Cell<bool>>>> = const { RefCell::new(None) };
    static DOCKS: RefCell<Vec<Rc<Dock>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct Window {
    address: String,
    class: String,
    title: String,
    active: bool,
}

struct Entry {
    id: String,
    windows: Vec<Window>,
}

struct AppButton {
    id: String,
    button: RippleButton,
    windows: RefCell<Vec<Window>>,
    last_focused: Cell<i64>,
    dots: gtk4::Box,
    x: Cell<Tween>,
    placed: Cell<bool>,
}

impl AppButton {
    fn separator(&self) -> bool {
        self.id == SEPARATOR
    }

    fn width(&self) -> f64 {
        if self.separator() {
            1.0
        } else {
            BUTTON_BACKGROUND as f64
        }
    }
}

#[derive(Clone, Copy)]
struct Drag {
    dragging: bool,
    index: i64,
    drop: i64,
    x: f64,
}

const IDLE_DRAG: Drag = Drag {
    dragging: false,
    index: -1,
    drop: -1,
    x: 0.0,
};

struct Dock {
    config: Rc<Config>,
    theme: SharedTheme,
    window: gtk4::ApplicationWindow,
    trigger: gtk4::ApplicationWindow,
    strip: gtk4::Box,
    height: i32,
    screen_width: i32,
    pinned: Rc<Cell<bool>>,
    hovered: Cell<bool>,
    idle: Cell<bool>,
    area: gtk4::Box,
    shift: Shift,
    top: Cell<Tween>,
    pin: GroupButton,
    pin_icon: gtk4::Label,
    apps: gtk4::Fixed,
    buttons: RefCell<Vec<Rc<AppButton>>>,
    drag: Cell<Drag>,
    width: Cell<Tween>,
    ticking: Cell<bool>,
    rebuild: RefCell<Option<Rc<dyn Fn()>>>,
    root: gtk4::Box,
    preview: Rc<Preview>,
    locked: Rc<Cell<bool>>,
    covered: Cell<bool>,
    reserve: Reserve,
}

pub fn open(
    app: &gtk4::Application,
    config: &Rc<Config>,
    services: &Rc<Services>,
    theme: &SharedTheme,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Option<gtk4::ApplicationWindow> {
    if !config.dock_enable {
        return None;
    }
    let height = config.dock_height + ELEVATION + GAPS_OUT;
    let connector: String = monitor.connector().map(Into::into).unwrap_or_default();
    let pinned = PINNED.with(|shared| {
        shared
            .borrow_mut()
            .get_or_insert_with(|| Rc::new(Cell::new(config.dock_pinned_on_startup)))
            .clone()
    });

    let (pin, pin_icon) = pin_button(theme, height);
    let apps = gtk4::Fixed::new();
    apps.set_margin_top(GAPS_OUT);
    apps.set_valign(gtk4::Align::Fill);
    let overview = overview_button(theme, height);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    row.set_margin_start(ROW_PADDING);
    row.set_margin_end(ROW_PADDING);
    row.append(&pin);
    row.append(&separator(
        ELEVATION + ROW_PADDING + rounding::NORMAL,
        GAPS_OUT + ROW_PADDING + rounding::NORMAL,
    ));
    row.append(&apps);
    row.append(&separator(
        ELEVATION + ROW_PADDING + rounding::NORMAL,
        GAPS_OUT + ROW_PADDING + rounding::NORMAL,
    ));
    row.append(&overview);

    let plate = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    plate.add_css_class("dock");
    plate.set_margin_top(ELEVATION);
    plate.set_margin_bottom(GAPS_OUT);

    let background = gtk4::Overlay::new();
    background.set_child(Some(&plate));
    background.add_overlay(&row);
    background.set_measure_overlay(&row, true);
    background.set_margin_start(ELEVATION);
    background.set_margin_end(ELEVATION);

    let area = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    area.set_size_request(-1, height);
    area.append(&background);
    let shift = Shift::new(&area);
    shift.set_halign(gtk4::Align::Center);
    shift.set_valign(gtk4::Align::Start);

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.set_size_request(-1, height);
    root.append(&shift);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&root)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    let strip = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    strip.add_css_class("dock-trigger");
    let trigger = gtk4::ApplicationWindow::builder()
        .application(app)
        .default_width(1)
        .default_height(1)
        .child(&strip)
        .build();
    trigger.init_layer_shell();
    trigger.set_namespace(Some(TRIGGER_NAMESPACE));
    trigger.set_monitor(Some(monitor));
    trigger.set_layer(Layer::Top);
    trigger.set_anchor(Edge::Bottom, true);

    let dock = Rc::new(Dock {
        config: config.clone(),
        theme: theme.clone(),
        window: window.clone(),
        trigger: trigger.clone(),
        strip: strip.clone(),
        height,
        screen_width: monitor.geometry().width(),
        pinned,
        hovered: Cell::new(false),
        idle: Cell::new(true),
        area: area.clone(),
        shift,
        top: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_EFFECTS)),
        pin: pin.clone(),
        pin_icon,
        apps,
        buttons: RefCell::new(Vec::new()),
        drag: Cell::new(IDLE_DRAG),
        width: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_EFFECTS)),
        ticking: Cell::new(false),
        rebuild: RefCell::new(None),
        root: root.clone(),
        preview: Preview::new(theme, &root),
        locked: services.states.screen_locked.clone(),
        covered: Cell::new(services.fullscreen.covers(&connector)),
        reserve: Reserve::new(app, monitor, Edge::Bottom, RESERVE_NAMESPACE),
    });
    scope.keep(services.states.subscribe({
        let dock = Rc::downgrade(&dock);
        move || {
            if let Some(dock) = dock.upgrade() {
                dock.swap_surfaces(now(&dock.area));
            }
        }
    }));
    scope.keep(services.fullscreen.subscribe({
        let dock = Rc::downgrade(&dock);
        let fullscreen = services.fullscreen.clone();
        move || {
            if let Some(dock) = dock.upgrade() {
                dock.covered.set(fullscreen.covers(&connector));
                dock.swap_surfaces(now(&dock.area));
            }
        }
    }));
    dock.preview.connect_changed({
        let dock = Rc::downgrade(&dock);
        move || {
            if let Some(dock) = dock.upgrade() {
                dock.place(true);
            }
        }
    });
    scope.hold(watch::config("/dock/pinnedApps", {
        let dock = Rc::downgrade(&dock);
        move || {
            let Some(dock) = dock.upgrade() else {
                return;
            };
            let pins = config::current().dock_pinned.borrow().clone();
            if *dock.config.dock_pinned.borrow() != pins {
                dock.config.dock_pinned.replace(pins);
                dock.refresh();
            }
        }
    }));
    scope.hold(watch::config("/dock/pinnedOnStartup", {
        let dock = Rc::downgrade(&dock);
        move || {
            if let Some(dock) = dock.upgrade() {
                dock.pinned
                    .set(config::value_bool("/dock/pinnedOnStartup", false));
                dock.place(true);
            }
        }
    }));
    DOCKS.with(|docks| docks.borrow_mut().push(dock.clone()));
    scope.defer({
        let dock = Rc::downgrade(&dock);
        let trigger = trigger.clone();
        move || {
            trigger.destroy();
            DOCKS.with(|docks| {
                docks
                    .borrow_mut()
                    .retain(|held| !dock.upgrade().is_some_and(|dock| Rc::ptr_eq(held, &dock)))
            });
        }
    });

    pin.connect_clicked(|| {
        PINNED.with(|shared| {
            if let Some(pinned) = shared.borrow().as_ref() {
                pinned.set(!pinned.get());
            }
        });
        for dock in DOCKS.with(|docks| docks.borrow().clone()) {
            dock.place(true);
        }
    });
    overview.connect_clicked(|_| actions::run("overviewWorkspacesToggle"));

    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let dock = Rc::downgrade(&dock);
        move |_, _, _| {
            if let Some(dock) = dock.upgrade() {
                dock.hovered.set(true);
                dock.place(true);
            }
        }
    });
    hover.connect_leave({
        let dock = Rc::downgrade(&dock);
        move |_| {
            if let Some(dock) = dock.upgrade() {
                dock.hovered.set(false);
                dock.place(true);
            }
        }
    });
    area.add_controller(hover);

    let approach = gtk4::EventControllerMotion::new();
    approach.connect_motion({
        let dock = Rc::downgrade(&dock);
        move |_, _, _| {
            if let Some(dock) = dock.upgrade()
                && !dock.hovered.replace(true)
            {
                dock.place(true);
            }
        }
    });
    trigger.add_controller(approach);

    let rebuild: Rc<dyn Fn()> = {
        let dock = Rc::downgrade(&dock);
        Rc::new(move || {
            if let Some(dock) = dock.upgrade() {
                dock.refresh();
            }
        })
    };
    dock.rebuild.replace(Some(rebuild.clone()));
    dock.refresh();
    dock.place(false);
    scope.keep(services.events.subscribe({
        let queue = coalesce(rebuild);
        move |_, _| queue()
    }));

    window.connect_map({
        let dock = Rc::downgrade(&dock);
        move |_| {
            if let Some(dock) = dock.upgrade() {
                dock.mask();
            }
        }
    });
    dock.swap_surfaces(now(&dock.area));
    Some(window)
}

impl Dock {
    fn reveal(&self) -> bool {
        self.pinned.get()
            || (self.config.dock_hover_to_reveal && self.hovered.get())
            || self.preview.shown()
            || self.idle.get()
    }

    fn place(self: &Rc<Self>, animate: bool) {
        let pinned = self.pinned.get();
        self.pin.set_toggled(pinned);
        text::set_color(
            &self.pin_icon,
            if pinned { "m3onPrimary" } else { "colOnLayer0" },
        );
        self.window.set_exclusive_zone(if pinned {
            self.height - GAPS_OUT - (ELEVATION - GAPS_OUT)
        } else {
            0
        });
        let target = if self.reveal() {
            0.0
        } else if self.config.dock_hover_to_reveal {
            (self.height - self.config.dock_hover_region) as f64
        } else {
            (self.height + 1) as f64
        };
        if self.reveal() {
            self.swap_surfaces(now(&self.area));
        }
        let mut top = self.top.get();
        if animate && self.window.is_mapped() {
            top.retarget(target, now(&self.area));
        } else {
            top.jump(target);
        }
        self.top.set(top);
        self.run();
    }

    fn refresh(self: &Rc<Self>) {
        let entries = taskbar(&self.config);
        self.idle.set(
            !entries
                .iter()
                .any(|entry| entry.windows.iter().any(|window| window.active)),
        );
        let mut old = self.buttons.take();
        let mut kept: Vec<Rc<AppButton>> = Vec::new();
        for entry in &entries {
            let button = match old.iter().position(|button| button.id == entry.id) {
                Some(index) => old.remove(index),
                None => self.make_button(&entry.id),
            };
            button.windows.replace(entry.windows.clone());
            self.dress(&button);
            kept.push(button);
        }
        for button in old {
            self.apps.remove(&button.button);
        }
        if let Some(app) = self.preview.app() {
            let windows = entries
                .iter()
                .find(|entry| entry.id == app)
                .map(|entry| targets(&entry.windows))
                .unwrap_or_default();
            self.preview.set_targets(windows);
        }
        self.buttons.replace(kept);
        if !self.drag.get().dragging {
            self.cancel_drag();
        }
        self.layout(self.window.is_mapped());
        self.place(true);
    }

    fn make_button(self: &Rc<Self>, id: &str) -> Rc<AppButton> {
        let button = RippleButton::new(&self.theme);
        button.set_radius(rounding::NORMAL as f64);
        button.set_background_size(BUTTON_BACKGROUND as f32, BUTTON_BACKGROUND as f32);
        let dots = gtk4::Box::new(gtk4::Orientation::Horizontal, DOT_SPACING);
        let is_separator = id == SEPARATOR;
        if is_separator {
            button.set_size_request(1, BUTTON_HEIGHT);
            button.set_sensitive(false);
            let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            holder.append(&separator(SEPARATOR_INSET, SEPARATOR_INSET));
            button.set_content(&holder, 0, 0);
        } else {
            button.set_size_request(BUTTON_BACKGROUND, BUTTON_HEIGHT);
            button.set_content(&self.icon(id, &dots), 0, 0);
        }
        let entry = Rc::new(AppButton {
            id: id.to_owned(),
            button: button.clone(),
            windows: RefCell::new(Vec::new()),
            last_focused: Cell::new(-1),
            dots,
            x: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_EFFECTS)),
            placed: Cell::new(false),
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
                let (Some(dock), Some(entry)) = (dock.upgrade(), entry.upgrade()) else {
                    return;
                };
                let count = entry.windows.borrow().len() as i64;
                if count == 0 {
                    return;
                }
                entry.last_focused.set(count - 1);
                let centre = entry
                    .button
                    .compute_point(
                        &dock.root,
                        &gtk4::graphene::Point::new(entry.button.width() as f32 / 2.0, 0.0),
                    )
                    .map(|point| point.x() as i32)
                    .unwrap_or(0);
                dock.preview
                    .enter(&entry.id, centre, targets(&entry.windows.borrow()));
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
                let next = (entry.last_focused.get() + 1).rem_euclid(windows.len() as i64);
                entry.last_focused.set(next);
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
                dock.refresh_all();
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
                if !dock.drag.get().dragging {
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
                    && dock.drag.get().dragging
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
        let image = Paint::new(move |snapshot, width, height| {
            let Some(display) = gdk::Display::default() else {
                return;
            };
            let icons = gtk4::IconTheme::for_display(&display);
            let paintable = crate::platform::appicon::themed(
                &icons,
                &crate::platform::appicon::guess(&icons, &class),
                "image-missing",
                ICON_SIZE,
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
        image.set_size_request(BUTTON_BACKGROUND - BUTTON_PADDING * 2, ICON_SIZE);

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
                    (ICON_SIZE as f64 + DOT_GAP) as i32,
                    width,
                    DOT_HEIGHT,
                ))
            }
        });
        Centred::integral(&content).upcast()
    }

    fn dress(&self, entry: &AppButton) {
        if entry.separator() {
            return;
        }
        while let Some(child) = entry.dots.first_child() {
            entry.dots.remove(&child);
        }
        let windows = entry.windows.borrow();
        let active = windows.iter().any(|window| window.active);
        for _ in 0..windows.len().min(MAX_DOTS) {
            let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            dot.add_css_class(if active {
                "dock-dot-active"
            } else {
                "dock-dot"
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

    fn order(&self) -> Vec<usize> {
        let count = self.buttons.borrow().len();
        let drag = self.drag.get();
        if drag.index < 0 || drag.drop < 0 {
            return (0..count).collect();
        }
        let mut arranged: Vec<usize> = (0..count)
            .filter(|index| *index as i64 != drag.index)
            .collect();
        arranged.insert(
            (drag.drop as usize).min(arranged.len()),
            drag.index as usize,
        );
        arranged
    }

    fn layout(self: &Rc<Self>, animate: bool) {
        let buttons = self.buttons.borrow();
        let now = now(&self.area);
        let drag = self.drag.get();
        let mut cursor = 0.0;
        for index in self.order() {
            let entry = &buttons[index];
            let mut x = entry.x.get();
            if animate && entry.placed.replace(true) {
                x.retarget(cursor, now);
            } else {
                entry.placed.set(true);
                x.jump(cursor);
            }
            entry.x.set(x);
            let dragged = drag.dragging && drag.index == index as i64;
            entry.button.set_opacity(if dragged {
                DRAGGED_OPACITY
            } else if entry.separator() {
                0.4
            } else {
                1.0
            });
            cursor += entry.width() + APP_SPACING;
        }
        let total = (cursor - APP_SPACING).max(0.0);
        let mut width = self.width.get();
        if animate {
            width.retarget(total, now);
        } else {
            width.jump(total);
        }
        self.width.set(width);
        drop(buttons);
        self.run();
    }

    fn drop_position(&self, x: f64) -> i64 {
        let buttons = self.buttons.borrow();
        let drag = self.drag.get();
        let mut position = 0;
        let mut reach = 0.0;
        for (index, entry) in buttons.iter().enumerate() {
            if index as i64 == drag.index {
                continue;
            }
            let slot = entry.width();
            if x < reach + slot / 2.0 {
                break;
            }
            reach += slot + APP_SPACING;
            position += 1;
        }
        position
    }

    fn begin_drag(self: &Rc<Self>, entry: &Rc<AppButton>) {
        let index = self
            .buttons
            .borrow()
            .iter()
            .position(|button| Rc::ptr_eq(button, entry))
            .map(|index| index as i64)
            .unwrap_or(-1);
        self.drag.set(Drag {
            dragging: true,
            index,
            drop: -1,
            x: entry.x.get().target() + entry.width() / 2.0,
        });
        entry
            .button
            .insert_before(&self.apps, None::<&gtk4::Widget>);
    }

    fn update_drag(self: &Rc<Self>, x: f64) {
        let mut drag = self.drag.get();
        drag.x = x;
        self.drag.set(drag);
        let drop = self.drop_position(x);
        let mut drag = self.drag.get();
        drag.drop = drop;
        self.drag.set(drag);
        self.layout(true);
    }

    fn cancel_drag(&self) {
        self.release_dragged();
        self.drag.set(IDLE_DRAG);
    }

    fn release_dragged(&self) {
        let drag = self.drag.get();
        if !drag.dragging || drag.index < 0 {
            return;
        }
        if let Some(entry) = self.buttons.borrow().get(drag.index as usize) {
            let mut x = entry.x.get();
            x.jump(drag.x - entry.width() / 2.0);
            entry.x.set(x);
        }
    }

    fn commit_drag(self: &Rc<Self>) {
        let drag = self.drag.get();
        if drag.index < 0 || drag.drop < 0 {
            self.cancel_drag();
            self.layout(true);
            return;
        }
        let arranged = self.order();
        let buttons = self.buttons.borrow();
        let Some(separator) = arranged
            .iter()
            .position(|index| buttons[*index].separator())
        else {
            drop(buttons);
            self.cancel_drag();
            self.layout(true);
            return;
        };
        let pins: Vec<String> = arranged[..separator]
            .iter()
            .map(|index| {
                let entry = &buttons[*index];
                entry
                    .windows
                    .borrow()
                    .first()
                    .map(|window| window.class.clone())
                    .unwrap_or_else(|| entry.id.clone())
            })
            .collect();
        drop(buttons);
        if *self.config.dock_pinned.borrow() == pins {
            self.cancel_drag();
            self.layout(true);
            return;
        }
        self.release_dragged();
        let mut drag = self.drag.get();
        drag.dragging = false;
        self.drag.set(drag);
        self.config.set_dock_pins(pins);
        self.refresh_all();
    }

    fn refresh_all(&self) {
        for dock in DOCKS.with(|docks| docks.borrow().clone()) {
            if let Some(rebuild) = dock.rebuild.borrow().clone() {
                rebuild();
            }
        }
    }

    fn run(self: &Rc<Self>) {
        self.apply(now(&self.area));
        if self.ticking.replace(true) {
            return;
        }
        let dock = Rc::downgrade(self);
        self.area.add_tick_callback(move |_, clock| {
            let Some(dock) = dock.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            dock.apply(now);
            if dock.busy(now) {
                return glib::ControlFlow::Continue;
            }
            dock.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn busy(&self, now: i64) -> bool {
        self.top.get().running(now)
            || self.width.get().running(now)
            || self
                .buttons
                .borrow()
                .iter()
                .any(|entry| entry.x.get().running(now))
    }

    fn apply(&self, now: i64) {
        self.shift.set_offset(self.top.get().value(now) as f32);
        self.apps
            .set_size_request(self.width.get().value(now).round() as i32, -1);
        let drag = self.drag.get();
        for (index, entry) in self.buttons.borrow().iter().enumerate() {
            let x = if drag.dragging && drag.index == index as i64 {
                drag.x - entry.width() / 2.0
            } else {
                entry.x.get().value(now)
            };
            self.apps.move_(&entry.button, x, 0.0);
        }
        self.mask();
        self.swap_surfaces(now);
    }

    fn swap_surfaces(&self, now: i64) {
        if self.covered.get() && !self.locked.get() {
            self.reserve.hold(self.window.exclusive_zone());
        } else {
            self.reserve.release();
        }
        if self.locked.get() || self.covered.get() {
            self.trigger.set_visible(false);
            self.window.set_visible(false);
            return;
        }
        let hidden = !self.reveal() && !self.top.get().running(now);
        if !hidden {
            if !self.window.is_visible() {
                self.window.present();
            }
            self.trigger.set_visible(false);
            return;
        }
        if self.config.dock_hover_to_reveal {
            let width = self.area.measure(gtk4::Orientation::Horizontal, -1).1;
            let height = self.config.dock_hover_region.max(1);
            self.strip.set_size_request(width, height);
            self.trigger.set_default_size(width, height);
            if !self.trigger.is_visible() {
                self.trigger.present();
            }
        }
        self.window.set_visible(false);
    }

    fn mask(&self) {
        let Some(surface) = self.window.surface() else {
            return;
        };
        let top = self.top.get().value(now(&self.area)).round() as i32;
        let width = self.area.measure(gtk4::Orientation::Horizontal, -1).1;
        let left = (self.screen_width - width) / 2;
        let region = gtk4::cairo::RectangleInt::new(left, top, width, (self.height - top).max(0));
        surface.set_input_region(Some(&gtk4::cairo::Region::create_rectangle(&region)));
    }
}

fn pin_button(theme: &SharedTheme, height: i32) -> (GroupButton, gtk4::Label) {
    let symbol = text::symbol("keep", pixel_size::LARGER as f64);
    let button = GroupButton::new(theme, PIN_SIZE, PIN_SIZE);
    button.set_radii(rounding::NORMAL as f64, rounding::NORMAL as f64);
    button.set_content(&Centred::new(&symbol));
    let cell = (height - GAPS_OUT) as f64;
    button.set_margin_top(GAPS_OUT + ((cell - PIN_SIZE) / 2.0).ceil() as i32);
    button.set_valign(gtk4::Align::Start);
    (button, symbol)
}

fn overview_button(theme: &SharedTheme, height: i32) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_look(Look::default());
    button.set_radius(rounding::NORMAL as f64);
    let tall = height - GAPS_OUT;
    button.set_size_request(BUTTON_BACKGROUND, tall);
    button.set_background_size(BUTTON_BACKGROUND as f32, (tall - BUTTON_INSET * 2) as f32);
    button.set_margin_top(GAPS_OUT);
    button.set_valign(gtk4::Align::Fill);
    let symbol = text::symbol("apps", (BUTTON_BACKGROUND / 2) as f64);
    text::set_color(&symbol, "colOnLayer0");
    button.set_content(&Centred::new(&symbol), 0, 0);
    button
}

fn separator(top: i32, bottom: i32) -> gtk4::Widget {
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
    let grey = [0.299, 0.587, 0.114].map(|weight| weight * DESATURATION * base);
    let own = keep * base;
    let matrix = gtk4::graphene::Matrix::from_float([
        grey[0] + own,
        grey[0],
        grey[0],
        0.0,
        grey[1],
        grey[1] + own,
        grey[1],
        0.0,
        grey[2],
        grey[2],
        grey[2] + own,
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

fn taskbar(config: &Rc<Config>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let pinned = config.dock_pinned.borrow().clone();
    for id in &pinned {
        let id = id.to_lowercase();
        if entries.iter().any(|entry| entry.id == id) {
            continue;
        }
        entries.push(Entry {
            id,
            windows: Vec::new(),
        });
    }
    if !pinned.is_empty() {
        entries.push(Entry {
            id: SEPARATOR.to_owned(),
            windows: Vec::new(),
        });
    }

    let focused = hypr::json("activewindow")
        .and_then(|value| {
            value
                .get("address")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let clients = hypr::json("clients")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    for client in &clients {
        if client.get("mapped").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let class = client
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let ignored = config.dock_ignored.iter().any(|pattern| {
            glib::Regex::match_simple(
                pattern,
                &class,
                glib::RegexCompileFlags::CASELESS,
                glib::RegexMatchFlags::empty(),
            )
        });
        if ignored {
            continue;
        }
        let address = client
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let id = class.to_lowercase();
        let place = match entries.iter().position(|entry| entry.id == id) {
            Some(place) => place,
            None => {
                entries.push(Entry {
                    id: id.clone(),
                    windows: Vec::new(),
                });
                entries.len() - 1
            }
        };
        let active = address == focused;
        let title = client
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        entries[place].windows.push(Window {
            address,
            class,
            title,
            active,
        });
    }
    entries
}

fn targets(windows: &[Window]) -> Vec<Target> {
    windows
        .iter()
        .map(|window| Target {
            address: window.address.clone(),
            title: window.title.clone(),
        })
        .collect()
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

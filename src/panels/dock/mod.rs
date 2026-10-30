mod arrangement;
mod buttons;
pub mod preview;
mod taskbar;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::actions;
use crate::core::config::{self, Config};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::panels::dock::preview::Preview;
use crate::services::Services;
use crate::services::hyprland::urgentwindows::UrgentWindows;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Tween};
use crate::ui::reserve::Reserve;
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::coalesce;
use crate::ui::widgets::display::text::Shift;
use arrangement::{Drag, IDLE_DRAG, follow_drag};
use buttons::{AppButton, overview_button, separator};
use taskbar::{WindowList, collect_entries, fetch_windows, is_monitor_idle, targets};

const NAMESPACE: &str = "proscenio:dock";
const TRIGGER_NAMESPACE: &str = "proscenio:dockTrigger";
const RESERVE_NAMESPACE: &str = "proscenio:dockReserve";
const ELEVATION: i32 = 10;
const GAPS_OUT: i32 = 5;
const ROW_SPACING: i32 = 3;
const ROW_PADDING: i32 = 5;
const APP_SPACING: f64 = 2.0;
const ICON_PADDING: i32 = 15;
const BUTTON_INSET: i32 = GAPS_OUT + ROW_PADDING;
const DRAGGED_OPACITY: f64 = 0.85;
const MOVE_MILLIS: f64 = 200.0;
const HOVER_CHECK_DELAY: Duration = Duration::from_millis(250);
const ATTENTION_DURATION: Duration = Duration::from_secs(2);

thread_local! {
    static PINNED: RefCell<Option<Rc<Cell<bool>>>> = const { RefCell::new(None) };
    static DOCKS: RefCell<Vec<Rc<Dock>>> = const { RefCell::new(Vec::new()) };
    static REFRESH_QUEUE: Rc<dyn Fn()> = coalesce(Rc::new(refresh_docks));
}

#[derive(Clone, Copy)]
struct Metrics {
    icon_size: i32,
}

impl Metrics {
    fn background(self) -> i32 {
        self.icon_size + ICON_PADDING
    }

    fn button_height(self) -> i32 {
        self.background() + BUTTON_INSET * 2
    }

    fn height(self) -> i32 {
        self.button_height() + GAPS_OUT
    }
}

struct PointerControllers {
    hover: gtk4::EventControllerMotion,
    drag_over: gtk4::DropControllerMotion,
    approach: gtk4::EventControllerMotion,
    drag_approach: gtk4::DropControllerMotion,
}

impl PointerControllers {
    fn contains_pointer(&self) -> bool {
        self.hover.contains_pointer()
            || self.drag_over.contains_pointer()
            || self.approach.contains_pointer()
            || self.drag_approach.contains_pointer()
    }
}

struct Dock {
    config: Rc<Config>,
    theme: SharedTheme,
    window: gtk4::ApplicationWindow,
    trigger: gtk4::ApplicationWindow,
    strip: gtk4::Box,
    metrics: Metrics,
    screen_width: i32,
    is_pinned: Rc<Cell<bool>>,
    is_hovered: Cell<bool>,
    hover_check: Cell<Option<glib::SourceId>>,
    pointer_controllers: PointerControllers,
    connector: String,
    is_idle: Cell<bool>,
    area: gtk4::Box,
    shift: Shift,
    top: Cell<Tween>,
    apps: gtk4::Fixed,
    buttons: RefCell<Vec<Rc<AppButton>>>,
    drag: Cell<Drag>,
    width: Cell<Tween>,
    is_ticking: Cell<bool>,
    ignore_patterns: Vec<glib::Regex>,
    root: gtk4::Box,
    preview: Rc<Preview>,
    is_locked: Rc<Cell<bool>>,
    is_covered: Cell<bool>,
    reserve: Reserve,
    urgent_windows: UrgentWindows,
    is_attention_shown: Cell<bool>,
    attention_timer: Cell<Option<glib::SourceId>>,
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
    let metrics = Metrics {
        icon_size: config.dock_icon_size,
    };
    let height = metrics.height();
    let connector: String = monitor.connector().map(Into::into).unwrap_or_default();
    let is_pinned = PINNED.with(|shared| {
        shared
            .borrow_mut()
            .get_or_insert_with(|| Rc::new(Cell::new(config.dock_pinned_on_startup)))
            .clone()
    });

    let apps = gtk4::Fixed::new();
    apps.set_margin_top(GAPS_OUT);
    apps.set_valign(gtk4::Align::Fill);
    let overview = overview_button(theme, metrics);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    row.set_margin_start(ROW_PADDING);
    row.set_margin_end(ROW_PADDING);
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

    let hover = gtk4::EventControllerMotion::new();
    let drag_over = gtk4::DropControllerMotion::new();
    let approach_controller = gtk4::EventControllerMotion::new();
    let drag_approach = gtk4::DropControllerMotion::new();

    let dock = Rc::new(Dock {
        config: config.clone(),
        theme: theme.clone(),
        window: window.clone(),
        trigger: trigger.clone(),
        strip: strip.clone(),
        metrics,
        screen_width: monitor.geometry().width(),
        is_pinned,
        is_hovered: Cell::new(false),
        hover_check: Cell::new(None),
        pointer_controllers: PointerControllers {
            hover: hover.clone(),
            drag_over: drag_over.clone(),
            approach: approach_controller.clone(),
            drag_approach: drag_approach.clone(),
        },
        connector: connector.clone(),
        is_idle: Cell::new(true),
        area: area.clone(),
        shift,
        top: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_EFFECTS)),
        apps,
        buttons: RefCell::new(Vec::new()),
        drag: Cell::new(IDLE_DRAG),
        width: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_EFFECTS)),
        is_ticking: Cell::new(false),
        ignore_patterns: config
            .dock_ignored
            .iter()
            .filter_map(|pattern| {
                glib::Regex::new(
                    pattern,
                    glib::RegexCompileFlags::CASELESS,
                    glib::RegexMatchFlags::empty(),
                )
                .ok()
                .flatten()
            })
            .collect(),
        root: root.clone(),
        preview: Preview::new(theme, &root),
        is_locked: services.states.is_screen_locked.clone(),
        is_covered: Cell::new(services.fullscreen.covers(&connector)),
        reserve: Reserve::new(app, monitor, Edge::Bottom, RESERVE_NAMESPACE),
        urgent_windows: services.urgent_windows.clone(),
        is_attention_shown: Cell::new(false),
        attention_timer: Cell::new(None),
    });
    scope.keep(services.urgent_windows.subscribe({
        let dock = Rc::downgrade(&dock);
        move |address| {
            if let Some(dock) = dock.upgrade() {
                dock.show_for_attention(address);
            }
        }
    }));
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
                dock.is_covered.set(fullscreen.covers(&connector));
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
                refresh_docks();
            }
        }
    }));
    scope.hold(watch::config("/dock/pinnedOnStartup", {
        let dock = Rc::downgrade(&dock);
        move || {
            if let Some(dock) = dock.upgrade() {
                dock.is_pinned
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
            if let Some(dock) = dock.upgrade() {
                if let Some(timer) = dock.hover_check.take() {
                    timer.remove();
                }
                if let Some(timer) = dock.attention_timer.take() {
                    timer.remove();
                }
            }
            trigger.destroy();
            DOCKS.with(|docks| {
                docks
                    .borrow_mut()
                    .retain(|held| !dock.upgrade().is_some_and(|dock| Rc::ptr_eq(held, &dock)))
            });
        }
    });

    overview.connect_clicked(|_| actions::run("overviewWorkspacesToggle"));

    hover.connect_enter({
        let dock = Rc::downgrade(&dock);
        move |_, _, _| {
            if let Some(dock) = dock.upgrade() {
                dock.is_hovered.set(true);
                dock.place(true);
            }
        }
    });
    hover.connect_leave({
        let dock = Rc::downgrade(&dock);
        move |_| {
            if let Some(dock) = dock.upgrade() {
                dock.is_hovered.set(false);
                dock.place(true);
            }
        }
    });
    area.add_controller(hover);
    drag_over.connect_enter({
        let dock = Rc::downgrade(&dock);
        move |_, _, _| {
            if let Some(dock) = dock.upgrade() {
                dock.is_hovered.set(true);
                dock.place(true);
            }
        }
    });
    drag_over.connect_leave({
        let dock = Rc::downgrade(&dock);
        move |_| {
            if let Some(dock) = dock.upgrade() {
                dock.is_hovered.set(false);
                dock.place(true);
            }
        }
    });
    area.add_controller(drag_over);

    approach_controller.connect_motion({
        let dock = Rc::downgrade(&dock);
        move |_, _, _| {
            if let Some(dock) = dock.upgrade()
                && !dock.is_hovered.replace(true)
            {
                dock.place(true);
            }
        }
    });
    approach_controller.connect_leave({
        let dock = Rc::downgrade(&dock);
        move |_| {
            if let Some(dock) = dock.upgrade() {
                dock.schedule_hover_check();
            }
        }
    });
    trigger.add_controller(approach_controller);
    drag_approach.connect_enter({
        let dock = Rc::downgrade(&dock);
        move |_, _, _| {
            if let Some(dock) = dock.upgrade()
                && !dock.is_hovered.replace(true)
            {
                dock.place(true);
            }
        }
    });
    drag_approach.connect_leave({
        let dock = Rc::downgrade(&dock);
        move |_| {
            if let Some(dock) = dock.upgrade() {
                dock.schedule_hover_check();
            }
        }
    });
    trigger.add_controller(drag_approach);

    dock.refresh();
    dock.place(false);
    scope.keep(
        services
            .events
            .subscribe(|_, _| REFRESH_QUEUE.with(|queue| queue())),
    );

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
    fn should_reveal(&self) -> bool {
        self.is_pinned.get()
            || (self.config.dock_hover_to_reveal && self.is_hovered.get())
            || self.preview.is_requested()
            || self.is_idle.get()
            || self.is_attention_shown.get()
    }

    fn show_for_attention(self: &Rc<Self>, address: &str) {
        let has_window = self.buttons.borrow().iter().any(|button| {
            button
                .windows
                .borrow()
                .iter()
                .any(|window| window.address == address)
        });
        if !has_window {
            return;
        }
        if let Some(timer) = self.attention_timer.take() {
            timer.remove();
        }
        self.is_attention_shown.set(true);
        self.place(true);
        let dock = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(ATTENTION_DURATION, move || {
            let Some(dock) = dock.upgrade() else {
                return;
            };
            dock.attention_timer.take();
            dock.is_attention_shown.set(false);
            dock.place(true);
        });
        self.attention_timer.set(Some(source));
    }

    fn schedule_hover_check(self: &Rc<Self>) {
        if let Some(timer) = self.hover_check.take() {
            timer.remove();
        }
        let dock = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(HOVER_CHECK_DELAY, move || {
            let Some(dock) = dock.upgrade() else {
                return;
            };
            dock.hover_check.take();
            if !dock.pointer_controllers.contains_pointer() && dock.is_hovered.replace(false) {
                dock.place(true);
            }
        });
        self.hover_check.set(Some(source));
    }

    fn place(self: &Rc<Self>, should_animate: bool) {
        let height = self.metrics.height();
        self.window.set_exclusive_zone(if self.is_pinned.get() {
            height - GAPS_OUT - (ELEVATION - GAPS_OUT)
        } else {
            0
        });
        let target = if self.should_reveal() {
            0.0
        } else if self.config.dock_hover_to_reveal {
            (height - self.config.dock_hover_region) as f64
        } else {
            (height + 1) as f64
        };
        if self.should_reveal() {
            self.swap_surfaces(now(&self.area));
        }
        let mut top = self.top.get();
        if should_animate && self.window.is_mapped() {
            top.retarget(target, now(&self.area));
        } else {
            top.jump(target);
        }
        self.top.set(top);
        self.run();
    }

    fn refresh(self: &Rc<Self>) {
        self.refresh_with(&fetch_windows());
    }

    fn refresh_with(self: &Rc<Self>, windows: &WindowList) {
        let entries = collect_entries(
            &self.config.dock_pinned.borrow(),
            &self.ignore_patterns,
            windows,
            |address| self.urgent_windows.contains(address),
        );
        self.is_idle.set(is_monitor_idle(windows, &self.connector));
        let mut old = self.buttons.take();
        let dragged_id = usize::try_from(self.drag.get().index)
            .ok()
            .and_then(|index| old.get(index))
            .map(|button| button.id.clone());
        let mut kept: Vec<Rc<AppButton>> = Vec::new();
        for entry in &entries {
            let button = match old.iter().position(|button| button.id == entry.id) {
                Some(index) => old.remove(index),
                None => self.make_button(&entry.id),
            };
            button.windows.replace(entry.windows.clone());
            self.update_dots(&button);
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
        let ids: Vec<&str> = kept.iter().map(|button| button.id.as_str()).collect();
        let mut drag = follow_drag(self.drag.get(), dragged_id.as_deref(), &ids);
        self.buttons.replace(kept);
        self.drag.set(drag);
        if drag.drop_position >= 0 {
            drag.drop_position = self.drop_position(drag.x);
            self.drag.set(drag);
        }
        self.layout(self.window.is_mapped());
        self.place(true);
    }

    fn layout(self: &Rc<Self>, should_animate: bool) {
        let buttons = self.buttons.borrow();
        let now = now(&self.area);
        let drag = self.drag.get();
        let mut cursor = 0.0;
        for index in self.order() {
            let entry = &buttons[index];
            let mut x = entry.x.get();
            if should_animate && entry.is_placed.replace(true) {
                x.retarget(cursor, now);
            } else {
                entry.is_placed.set(true);
                x.jump(cursor);
            }
            entry.x.set(x);
            let is_dragged = drag.is_dragging && drag.index == index as i64;
            entry.button.set_opacity(if is_dragged {
                DRAGGED_OPACITY
            } else if entry.is_separator() {
                0.4
            } else {
                1.0
            });
            cursor += entry.width() + APP_SPACING;
        }
        let total = (cursor - APP_SPACING).max(0.0);
        let mut width = self.width.get();
        if should_animate {
            width.retarget(total, now);
        } else {
            width.jump(total);
        }
        self.width.set(width);
        drop(buttons);
        self.run();
    }

    fn run(self: &Rc<Self>) {
        self.apply(now(&self.area));
        if self.is_ticking.replace(true) {
            return;
        }
        let dock = Rc::downgrade(self);
        self.area.add_tick_callback(move |_, clock| {
            let Some(dock) = dock.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            dock.apply(now);
            if dock.is_busy(now) {
                return glib::ControlFlow::Continue;
            }
            dock.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn is_busy(&self, now: i64) -> bool {
        self.top.get().is_running(now)
            || self.width.get().is_running(now)
            || self
                .buttons
                .borrow()
                .iter()
                .any(|entry| entry.x.get().is_running(now))
    }

    fn apply(&self, now: i64) {
        self.shift.set_offset(self.top.get().value(now) as f32);
        self.apps
            .set_size_request(self.width.get().value(now).round() as i32, -1);
        let drag = self.drag.get();
        for (index, entry) in self.buttons.borrow().iter().enumerate() {
            let x = if drag.is_dragging && drag.index == index as i64 {
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
        if self.is_covered.get() && !self.is_locked.get() {
            self.reserve.hold(self.window.exclusive_zone());
        } else {
            self.reserve.release();
        }
        if self.is_locked.get() || self.is_covered.get() {
            self.trigger.set_visible(false);
            self.window.set_visible(false);
            return;
        }
        if !self.config.dock_hover_to_reveal {
            self.trigger.set_visible(false);
        }
        let is_hidden = !self.should_reveal() && !self.top.get().is_running(now);
        if !is_hidden {
            if !self.window.is_visible() {
                self.window.present();
            }
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
        let region =
            gtk4::cairo::RectangleInt::new(left, top, width, (self.metrics.height() - top).max(0));
        surface.set_input_region(Some(&gtk4::cairo::Region::create_rectangle(&region)));
    }
}

fn refresh_docks() {
    let docks = DOCKS.with(|docks| docks.borrow().clone());
    if docks.is_empty() {
        return;
    }
    let windows = fetch_windows();
    for dock in docks {
        dock.refresh_with(&windows);
    }
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_size_sets_the_button_and_dock_sizes() {
        let sizes = |icon_size| {
            let metrics = Metrics { icon_size };
            (
                metrics.background(),
                metrics.button_height(),
                metrics.height(),
            )
        };
        assert_eq!(sizes(35), (50, 70, 75));
        assert_eq!(sizes(60), (75, 95, 100));
    }
}

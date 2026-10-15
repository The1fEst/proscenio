pub mod laps;
pub mod month;
pub mod navrail;
pub mod pinned;
pub mod pomodoro;
pub mod todo;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::persistent;
use crate::core::scope::Scope;
use crate::platform::wayland::grab;
use crate::services::Services;
use crate::ui::anim::{EMPHASIZED_DECEL, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text::Shift;
use crate::ui::widgets::layout::centered::Centered;
use month::Month;
use navrail::NavRail;
use pomodoro::TimerPage;
use todo::TodoPage;

const TABS: [(&str, &str); 3] = [
    ("calendar_month", "Calendar"),
    ("done_outline", "To Do"),
    ("schedule", "Timer"),
];

const NAMESPACE: &str = "proscenio:calendarPanel";
const WIDTH: i32 = 420;
const ELEVATION: i32 = 10;
const MINIMUM: i32 = 350;
const RAIL_MARGIN: i32 = 10;
const RAIL_INSET: i32 = 5;
const SPACING: i32 = 20;
const SWITCH_DISTANCE: f32 = 10.0;
const SWITCH_MILLIS: f64 = 200.0;

pub struct Calendar {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    month: Rc<Month>,
    todo_page: Rc<TodoPage>,
    timer_page: Rc<TimerPage>,
    vertical: bool,
    clock: glib::WeakRef<gtk4::Widget>,
}

struct Pages {
    holder: gtk4::Box,
    shift: Shift,
    widgets: Vec<gtk4::Widget>,
    current_index: Cell<usize>,
    visible_index: Cell<usize>,
    phase: Cell<Tween>,
    is_leaving: Cell<bool>,
    is_downward: Cell<bool>,
    is_ticking: Cell<bool>,
    rail: Rc<NavRail>,
    resets: Vec<Box<dyn Fn()>>,
}

impl Pages {
    fn select(self: &Rc<Self>, index: usize) {
        let previous = self.current_index.replace(index);
        if previous == index {
            return;
        }
        self.rail.set_current(index);
        if !self.holder.is_mapped() {
            self.swap();
            return;
        }
        self.is_downward.set(index > previous);
        if self.is_leaving.get() && self.is_ticking.get() {
            return;
        }
        self.start(true);
    }

    fn swap(&self) {
        let index = self.current_index.get();
        if let Some(child) = self.holder.first_child() {
            self.holder.remove(&child);
        }
        (self.resets[index])();
        self.holder.append(&self.widgets[index]);
        self.visible_index.set(index);
    }

    fn start(self: &Rc<Self>, is_leaving: bool) {
        let now = self
            .holder
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        self.is_leaving.set(is_leaving);
        let mut phase = Tween::new(
            0.0,
            SWITCH_MILLIS,
            if is_leaving {
                EXPRESSIVE_EFFECTS
            } else {
                EMPHASIZED_DECEL
            },
        );
        phase.retarget(1.0, now);
        self.phase.set(phase);
        if self.is_ticking.replace(true) {
            return;
        }
        let pages = Rc::downgrade(self);
        self.holder.add_tick_callback(move |_, clock| {
            let Some(pages) = pages.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            let phase = pages.phase.get();
            let part = phase.value(now) as f32;
            let sign = if pages.is_downward.get() { -1.0 } else { 1.0 };
            if pages.is_leaving.get() {
                pages.shift.set_offset(sign * SWITCH_DISTANCE * part);
                pages.shift.set_opacity(1.0 - part as f64);
            } else {
                pages
                    .shift
                    .set_offset(-sign * SWITCH_DISTANCE * (1.0 - part));
                pages.shift.set_opacity(part as f64);
            }
            if phase.is_running(now) {
                return glib::ControlFlow::Continue;
            }
            if pages.is_leaving.get() {
                pages.swap();
                pages.is_ticking.set(false);
                pages.start(false);
                return glib::ControlFlow::Break;
            }
            if pages.visible_index.get() != pages.current_index.get() {
                pages.is_ticking.set(false);
                pages.start(true);
                return glib::ControlFlow::Break;
            }
            pages.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn step(self: &Rc<Self>, delta: i32) {
        let last = self.widgets.len() as i32 - 1;
        let index = (self.current_index.get() as i32 + delta).clamp(0, last) as usize;
        self.select(index);
        persistent::write(&["calendarPanel", "tab"], index.into());
    }
}

impl Calendar {
    pub fn set_clock(&self, clock: &impl IsA<gtk4::Widget>) {
        self.clock.set(Some(clock.as_ref()));
    }

    pub fn set_today(&self, now: &glib::DateTime) {
        self.month.set_today(now);
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
            return;
        }
        self.show();
    }

    pub fn open(self: &Rc<Self>) {
        if !self.window.is_visible() {
            self.show();
        }
    }

    pub fn close(&self) {
        if self.window.is_visible() {
            self.hide();
        }
    }

    fn show(self: &Rc<Self>) {
        if let Some(clock) = self.clock.upgrade() {
            if self.vertical {
                self.window
                    .set_margin(Edge::Top, self.center_beside(&clock));
            } else {
                self.window.set_margin(Edge::Left, self.center_on(&clock));
            }
        }
        self.window.set_visible(true);
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let calendar = Rc::downgrade(self);
        grab.hold(&surface, move || {
            if let Some(calendar) = calendar.upgrade() {
                calendar.hide();
            }
        });
    }

    fn hide(&self) {
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        self.window.set_visible(false);
    }

    fn center_on(&self, anchor: &impl IsA<gtk4::Widget>) -> i32 {
        let anchor = anchor.as_ref();
        let Some(root) = anchor.root() else {
            return 0;
        };
        let Some(bounds) = anchor.compute_bounds(&root) else {
            return 0;
        };
        (bounds.x() + (bounds.width() - WIDTH as f32) / 2.0) as i32
    }

    fn center_beside(&self, anchor: &impl IsA<gtk4::Widget>) -> i32 {
        let anchor = anchor.as_ref();
        let Some(root) = anchor.root() else {
            return 0;
        };
        let Some(bounds) = anchor.compute_bounds(&root) else {
            return 0;
        };
        let height = self.window.child().map_or(0, |panel| {
            panel.measure(gtk4::Orientation::Vertical, -1).1 - ELEVATION * 2
        });
        (bounds.y() + (bounds.height() - height as f32) / 2.0) as i32
    }
}

pub fn build(
    app: &gtk4::Application,
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Rc<Services>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Rc<Calendar> {
    let current_index = persistent::read(&["calendarPanel", "tab"])
        .and_then(|value| value.as_u64())
        .map(|tab| (tab as usize).min(TABS.len() - 1))
        .unwrap_or(0);

    let month = month::build(theme);
    let todo_page = todo::build(theme, &services.todo, scope);
    let timer_page = pomodoro::build(theme, &services.timer, scope);
    let widgets: Vec<gtk4::Widget> = vec![
        month.widget.clone(),
        todo_page.widget.clone(),
        timer_page.widget.clone(),
    ];
    let resets: Vec<Box<dyn Fn()>> = vec![
        Box::new({
            let month = month.clone();
            move || month.reset()
        }),
        Box::new({
            let todo_page = todo_page.clone();
            move || todo_page.reset()
        }),
        Box::new({
            let timer_page = timer_page.clone();
            move || timer_page.reset()
        }),
    ];

    let rail = NavRail::new(theme, &TABS, current_index);
    let rail_slot = Centered::integral(&rail.widget);
    rail_slot.set_vexpand(true);
    rail_slot.set_margin_top(RAIL_MARGIN);
    rail_slot.set_margin_start(RAIL_MARGIN + RAIL_INSET);
    rail_slot.set_margin_end(SPACING - RAIL_INSET);

    let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    holder.set_hexpand(true);
    holder.set_vexpand(true);
    let shift = Shift::new(&holder);
    shift.set_hexpand(true);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    content.add_css_class("calendar-content");
    content.set_overflow(gtk4::Overflow::Hidden);
    content.set_size_request(-1, MINIMUM);
    content.append(&rail_slot);
    content.append(&shift);

    let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    panel.add_css_class("calendar-panel");
    panel.set_size_request(WIDTH, -1);
    panel.set_halign(gtk4::Align::Start);
    panel.set_valign(gtk4::Align::Start);
    panel.set_margin_top(ELEVATION);
    panel.set_margin_bottom(ELEVATION);
    panel.set_margin_start(ELEVATION);
    panel.set_margin_end(ELEVATION);
    panel.append(&content);

    let pages = Rc::new(Pages {
        holder,
        shift,
        widgets,
        current_index: Cell::new(current_index),
        visible_index: Cell::new(current_index),
        phase: Cell::new(Tween::new(1.0, SWITCH_MILLIS, EXPRESSIVE_EFFECTS)),
        is_leaving: Cell::new(false),
        is_downward: Cell::new(true),
        is_ticking: Cell::new(false),
        rail: rail.clone(),
        resets,
    });
    pages.swap();
    rail.connect_pressed({
        let pages = Rc::downgrade(&pages);
        move |index| {
            if let Some(pages) = pages.upgrade() {
                pages.select(index);
                persistent::write(&["calendarPanel", "tab"], index.into());
            }
        }
    });

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&panel)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    if config.vertical {
        let side = if config.bottom {
            Edge::Right
        } else {
            Edge::Left
        };
        window.set_anchor(side, true);
        window.set_margin(side, config.vertical_bar_width());
    } else {
        window.set_anchor(Edge::Left, true);
        window.set_margin(Edge::Top, config.bar_height());
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_visible(false);

    let calendar = Rc::new(Calendar {
        window: window.clone(),
        grab: grab::Grab::new(&monitor.display()),
        month,
        todo_page,
        timer_page,
        vertical: config.vertical,
        clock: glib::WeakRef::new(),
    });

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_controller.connect_key_pressed({
        let calendar = Rc::downgrade(&calendar);
        let pages = pages.clone();
        move |_, key, _, modifiers| {
            let Some(calendar) = calendar.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let is_control_held = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            let tab_index = pages.current_index.get();
            let todo_page = &calendar.todo_page;
            match key {
                gdk::Key::Escape if tab_index == 1 && todo_page.is_adding() => {
                    todo_page.close_dialog()
                }
                gdk::Key::Escape => calendar.hide(),
                gdk::Key::Page_Down if is_control_held => pages.step(1),
                gdk::Key::Page_Up if is_control_held => pages.step(-1),
                gdk::Key::Page_Down if tab_index == 0 => calendar.month.step(1),
                gdk::Key::Page_Up if tab_index == 0 => calendar.month.step(-1),
                gdk::Key::Page_Down if tab_index == 1 && !todo_page.is_adding() => {
                    todo_page.step(1)
                }
                gdk::Key::Page_Up if tab_index == 1 && !todo_page.is_adding() => todo_page.step(-1),
                gdk::Key::n | gdk::Key::N if tab_index == 1 && !todo_page.is_adding() => {
                    todo_page.open_dialog()
                }
                gdk::Key::Page_Down if tab_index == 2 => calendar.timer_page.step(1),
                gdk::Key::Page_Up if tab_index == 2 => calendar.timer_page.step(-1),
                gdk::Key::space | gdk::Key::s | gdk::Key::S if tab_index == 2 => {
                    calendar.timer_page.toggle()
                }
                gdk::Key::r | gdk::Key::R if tab_index == 2 => calendar.timer_page.restart(),
                gdk::Key::l | gdk::Key::L if tab_index == 2 => calendar.timer_page.lap(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(key_controller);

    calendar
}

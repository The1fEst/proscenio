use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::panels::settings::Settings;
use crate::panels::sidebar::quicktoggle::{CELL_HEIGHT, Glyph, QuickToggle, Start};
use crate::panels::sidebar::toggles::{self, Menu};
use crate::services::Services;
use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::group::ButtonGroup;

const SPACING: f64 = 6.0;
const PADDING: f64 = 6.0;
const SECTION_SPACING: f64 = 12.0;
const DIVIDER_INSET: i32 = (CELL_HEIGHT / 2.0) as i32;
const THRESHOLD: f64 = 6.0;
const MOVE_MILLIS: f64 = 500.0;
const FADE_MILLIS: f64 = 200.0;
const HOLD: Duration = Duration::from_millis(800);

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Panel {
        pub theme: RefCell<Option<SharedTheme>>,
        pub height: Cell<Option<Tween>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Panel {
        const NAME: &'static str = "ProscenioQuickPanel";
        type Type = super::Panel;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Panel {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Panel {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            if orientation == gtk4::Orientation::Horizontal {
                let (minimum, natural, _, _) = child.measure(orientation, for_size);
                return (minimum, natural, -1, -1);
            }
            let height = self
                .height
                .get()
                .map(|tween| tween.value(self.now()))
                .unwrap_or_else(|| child.measure(orientation, for_size).1 as f64)
                .round()
                .max(0.0) as i32;
            (height, height, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                let natural = child.measure(gtk4::Orientation::Vertical, -1).1;
                child.allocate(width, natural, -1, None);
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let panel = self.obj();
            if let Some(shared) = self.theme.borrow().clone() {
                let (width, height) = (panel.width() as f32, panel.height() as f32);
                let radius = (rounding::NORMAL as f32).min(width / 2.0).min(height / 2.0);
                let bounds = graphene::Rect::new(0.0, 0.0, width, height);
                snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, radius));
                snapshot.append_color(&shared.borrow().colors.col_layer1, &bounds);
                snapshot.pop();
            }
            self.parent_snapshot(snapshot);
        }
    }

    impl Panel {
        pub fn now(&self) -> i64 {
            self.obj()
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time)
        }
    }
}

glib::wrapper! {
    pub struct Panel(ObjectSubclass<imp::Panel>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

struct Drag {
    kind: &'static str,
    target: Option<(bool, i32)>,
}

struct Press {
    x: f64,
    y: f64,
    button: u32,
    kind: Option<&'static str>,
    dragging: bool,
}

struct Shown {
    kind: &'static str,
    used: bool,
    toggle: Rc<QuickToggle>,
}

pub struct QuickPanel {
    root: Panel,
    used: gtk4::Box,
    divider: gtk4::Box,
    unused: gtk4::Box,
    services: Rc<Services>,
    theme: SharedTheme,
    config: Rc<Config>,
    cell: f64,
    columns: i32,
    close: Rc<dyn Fn()>,
    open_menu: Rc<dyn Fn(Menu)>,
    settings: Rc<Settings>,
    editing: Cell<bool>,
    drag: RefCell<Option<Drag>>,
    press: RefCell<Option<Press>>,
    hold: RefCell<Option<glib::SourceId>>,
    shown: RefCell<Vec<Shown>>,
    fade: Cell<Tween>,
    ticking: Cell<bool>,
}

pub struct Setup {
    pub width: f64,
    pub columns: i32,
    pub close: Rc<dyn Fn()>,
    pub open_menu: Rc<dyn Fn(Menu)>,
    pub settings: Rc<Settings>,
}

impl QuickPanel {
    pub fn new(
        services: &Rc<Services>,
        theme: &SharedTheme,
        config: &Rc<Config>,
        setup: Setup,
        scope: &Scope,
    ) -> Rc<Self> {
        let columns = setup.columns.max(1);
        let cell = (setup.width - PADDING * 2.0 - SPACING * columns as f64) / columns as f64;

        let used = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING as i32);
        used.set_halign(gtk4::Align::Start);

        let divider = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        divider.add_css_class("quick-divider");
        divider.set_size_request(-1, 1);
        divider.set_margin_start(DIVIDER_INSET);
        divider.set_margin_end(DIVIDER_INSET);
        divider.set_visible(false);

        let unused = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING as i32);
        unused.set_size_request(-1, CELL_HEIGHT as i32);
        unused.set_visible(false);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING as i32);
        content.set_margin_start(PADDING as i32);
        content.set_margin_end(PADDING as i32);
        content.set_margin_top(PADDING as i32);
        content.set_margin_bottom(PADDING as i32);
        content.append(&used);
        content.append(&divider);
        content.append(&unused);

        let root: Panel = glib::Object::new();
        root.imp().theme.replace(Some(theme.clone()));
        content.set_parent(&root);

        let panel = Rc::new(QuickPanel {
            root: root.clone(),
            used,
            divider,
            unused,
            services: services.clone(),
            theme: theme.clone(),
            config: config.clone(),
            cell,
            columns,
            close: setup.close,
            open_menu: setup.open_menu,
            settings: setup.settings,
            editing: Cell::new(false),
            drag: RefCell::new(None),
            press: RefCell::new(None),
            hold: RefCell::new(None),
            shown: RefCell::new(Vec::new()),
            fade: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            ticking: Cell::new(false),
        });

        panel.watch_input();
        panel.rebuild();

        let weak = Rc::downgrade(&panel);
        toggles::subscribe(services, scope, move || {
            if let Some(panel) = weak.upgrade() {
                panel.repaint();
            }
        });

        panel
    }

    pub fn widget(&self) -> gtk4::Widget {
        self.root.clone().upcast()
    }

    pub fn set_editing(self: &Rc<Self>, editing: bool) {
        if self.editing.replace(editing) == editing {
            return;
        }
        if !editing && let Some(drag) = self.drag.borrow_mut().as_mut() {
            drag.target = None;
        }
        self.root.set_cursor_from_name(editing.then_some("pointer"));
        if editing {
            self.divider.set_visible(true);
            self.unused.set_visible(true);
        }
        let now = self.root.imp().now();
        let mut fade = self.fade.get();
        fade.retarget(if editing { 1.0 } else { 0.0 }, now);
        self.fade.set(fade);
        self.rebuild();
    }

    fn stored(&self) -> Vec<(String, i32)> {
        self.config.toggles.borrow().clone()
    }

    fn store(&self, list: Vec<(String, i32)>) {
        self.config.set_quick_toggles(list);
    }

    fn with_inserted(list: &[(String, i32)], kind: &str, index: i32) -> Vec<(String, i32)> {
        let mut kept: Vec<(String, i32)> = list
            .iter()
            .filter(|(entry, _)| entry != kind)
            .cloned()
            .collect();
        let existing = list.iter().find(|(entry, _)| entry == kind).cloned();
        let at = if index < 0 {
            kept.len()
        } else {
            (index as usize).min(kept.len())
        };
        kept.insert(at, existing.unwrap_or_else(|| (kind.to_owned(), 1)));
        kept
    }

    fn rows(list: &[(&'static str, i32)], columns: i32) -> Vec<Vec<(&'static str, i32)>> {
        let mut rows = Vec::new();
        let mut row = Vec::new();
        let mut total = 0;
        for &(kind, size) in list {
            if total + size > columns {
                rows.push(std::mem::take(&mut row));
                total = 0;
            }
            row.push((kind, size));
            total += size;
        }
        if !row.is_empty() {
            rows.push(row);
        }
        rows
    }

    fn rebuild(self: &Rc<Self>) {
        let stored = self.stored();
        let displayed = match self.drag.borrow().as_ref() {
            Some(Drag {
                kind,
                target: Some((true, index)),
            }) => Self::with_inserted(&stored, kind, *index),
            _ => stored.clone(),
        };
        let used: Vec<(&'static str, i32)> = displayed
            .iter()
            .filter_map(|(kind, size)| Some((toggles::kind(kind)?, *size)))
            .collect();
        let unused: Vec<(&'static str, i32)> = toggles::AVAILABLE
            .iter()
            .filter(|kind| !stored.iter().any(|(entry, _)| entry == *kind))
            .map(|kind| (*kind, 1))
            .collect();

        let old: Vec<Shown> = self.shown.take();
        for shown in &old {
            shown.toggle.button.unparent();
        }
        for container in [&self.used, &self.unused] {
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
        }

        let animate = self.root.is_mapped();
        let dragged = self.drag.borrow().as_ref().map(|drag| drag.kind);
        let mut shown = Vec::new();
        for (list, is_used, container) in
            [(&used, true, &self.used), (&unused, false, &self.unused)]
        {
            for row in Self::rows(list, self.columns) {
                let group = ButtonGroup::new(&self.theme);
                group.set_spacing(SPACING);
                group.set_phantom_children(1);
                group.set_halign(gtk4::Align::Start);
                for (kind, size) in row {
                    let previous = old
                        .iter()
                        .position(|entry| entry.kind == kind && entry.used == is_used);
                    let toggle = match previous.map(|index| &old[index]) {
                        Some(entry) if entry.toggle.size == size => entry.toggle.clone(),
                        Some(entry) => self.make(
                            kind,
                            size,
                            Start {
                                slide_from: Some(entry.toggle.button.current_width()),
                                fade_in: false,
                            },
                        ),
                        None => self.make(
                            kind,
                            size,
                            Start {
                                slide_from: None,
                                fade_in: animate,
                            },
                        ),
                    };
                    toggle.set_editing(self.editing.get());
                    toggle.set_dragged(dragged == Some(kind));
                    toggle.show(&toggles::look(kind, &self.services));
                    group.append(&toggle.button);
                    shown.push(Shown {
                        kind,
                        used: is_used,
                        toggle,
                    });
                }
                container.append(&group);
            }
        }
        if self.editing.get() {
            let columns = self.columns as f64;
            let free_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            free_row.set_size_request(
                (self.cell * columns + SPACING * (columns - 1.0)) as i32,
                CELL_HEIGHT as i32,
            );
            self.used.append(&free_row);
        }
        self.shown.replace(shown);
        self.resize();
    }

    fn make(&self, kind: &'static str, size: i32, start: Start) -> Rc<QuickToggle> {
        let width = self.cell * size as f64 + SPACING * (size - 1) as f64;
        let main: Rc<dyn Fn()> = {
            let services = self.services.clone();
            let close = self.close.clone();
            Rc::new(move || toggles::act(kind, &services, &close))
        };
        let alt: Option<Rc<dyn Fn()>> = match (toggles::menu(kind), kind) {
            (Some(menu), _) => {
                let open_menu = self.open_menu.clone();
                Some(Rc::new(move || open_menu(menu)))
            }
            (None, "easyEffects") => {
                let services = self.services.clone();
                let close = self.close.clone();
                Some(Rc::new(move || {
                    services.easyeffects.configure();
                    close();
                }))
            }
            (None, "ethernet") => {
                let settings = self.settings.clone();
                let close = self.close.clone();
                Some(Rc::new(move || {
                    close();
                    settings.open(Some("network"));
                }))
            }
            (None, _) => None,
        };
        let glyph = if kind == "wireGuard" {
            Glyph::Custom("wireguard-symbolic")
        } else {
            Glyph::Symbol
        };
        let toggle = QuickToggle::new(&self.theme, width, size, alt.is_some(), glyph, start);
        toggle.connect_actions(main, alt);
        toggle
    }

    fn repaint(&self) {
        for shown in self.shown.borrow().iter() {
            shown
                .toggle
                .show(&toggles::look(shown.kind, &self.services));
        }
    }

    fn resize(self: &Rc<Self>) {
        let used = self.used.measure(gtk4::Orientation::Vertical, -1).1 as f64;
        let content = if self.editing.get() {
            let unused = self.unused.measure(gtk4::Orientation::Vertical, -1).1 as f64;
            used + SECTION_SPACING + 1.0 + SECTION_SPACING + unused
        } else {
            used
        };
        let target = content + PADDING * 2.0;
        let imp = self.root.imp();
        let mut height = imp
            .height
            .get()
            .unwrap_or_else(|| Tween::new(target, MOVE_MILLIS, EXPRESSIVE_DEFAULT));
        if self.root.is_mapped() {
            height.retarget(target, imp.now());
        } else {
            height.jump(target);
        }
        imp.height.set(Some(height));
        self.root.queue_resize();
        self.tick();
    }

    fn tick(self: &Rc<Self>) {
        if self.ticking.replace(true) {
            return;
        }
        let weak: Weak<Self> = Rc::downgrade(self);
        self.root.add_tick_callback(move |root, clock| {
            let Some(panel) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            let fade = panel.fade.get();
            let opacity = fade.value(now);
            panel.divider.set_opacity(opacity);
            panel.unused.set_opacity(opacity);
            if opacity <= 0.0 && !fade.running(now) {
                panel.divider.set_visible(false);
                panel.unused.set_visible(false);
            }
            root.queue_resize();
            let height = root.imp().height.get();
            if fade.running(now) || height.is_some_and(|tween| tween.running(now)) {
                return glib::ControlFlow::Continue;
            }
            panel.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn hit(&self, x: f64, y: f64) -> Option<&'static str> {
        self.shown.borrow().iter().find_map(|shown| {
            let bounds = shown.toggle.button.compute_bounds(&self.root)?;
            let inside = x >= bounds.x() as f64
                && y >= bounds.y() as f64
                && x < (bounds.x() + bounds.width()) as f64
                && y < (bounds.y() + bounds.height()) as f64;
            inside.then_some(shown.kind)
        })
    }

    fn used_width(&self) -> f64 {
        let mut width: f64 = 0.0;
        let mut child = self.used.first_child();
        while let Some(row) = child {
            width = width.max(row.width() as f64);
            child = row.next_sibling();
        }
        width
    }

    fn toggle_size(self: &Rc<Self>, kind: Option<&'static str>) {
        let Some(kind) = kind else {
            return;
        };
        let mut list = self.stored();
        let Some(entry) = list.iter_mut().find(|(entry, _)| entry == kind) else {
            return;
        };
        entry.1 = 3 - entry.1;
        self.store(list);
        self.rebuild();
    }

    fn update_drop_target(self: &Rc<Self>, x: f64, y: f64) {
        let Some(kind) = self.drag.borrow().as_ref().map(|drag| drag.kind) else {
            return;
        };
        let target = self.drop_target(kind, x, y);
        let changed = {
            let mut drag = self.drag.borrow_mut();
            let Some(drag) = drag.as_mut() else {
                return;
            };
            match target {
                Some(target) if drag.target != target => {
                    drag.target = target;
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.rebuild();
        }
    }

    fn drop_target(&self, kind: &'static str, x: f64, y: f64) -> Option<Option<(bool, i32)>> {
        if !self.editing.get() {
            return Some(None);
        }
        if let Some(bounds) = self.used.compute_bounds(&self.root) {
            let point = (x - bounds.x() as f64, y - bounds.y() as f64);
            let height = bounds.height() as f64;
            if point.0 >= 0.0 && point.1 >= 0.0 && point.0 <= self.used_width() && point.1 <= height
            {
                for shown in self.shown.borrow().iter() {
                    if !shown.used || shown.kind != kind {
                        continue;
                    }
                    let Some(cell) = shown.toggle.button.compute_bounds(&self.used) else {
                        continue;
                    };
                    let (left, top) = (cell.x() as f64, cell.y() as f64);
                    if point.0 >= left
                        && point.0 < left + cell.width() as f64
                        && point.1 >= top
                        && point.1 < top + cell.height() as f64
                    {
                        return None;
                    }
                }
                return Some(Some((true, self.insertion_index(kind, point))));
            }
        }
        if self.unused.get_visible()
            && let Some(bounds) = self.unused.compute_bounds(&self.root)
        {
            let point = (x - bounds.x() as f64, y - bounds.y() as f64);
            if point.0 >= 0.0
                && point.1 >= 0.0
                && point.0 <= bounds.width() as f64
                && point.1 <= bounds.height() as f64
            {
                return Some(Some((false, -1)));
            }
        }
        Some(None)
    }

    fn insertion_index(&self, kind: &'static str, point: (f64, f64)) -> i32 {
        let mut cells: Vec<(f64, f64, f64, f64)> = self
            .shown
            .borrow()
            .iter()
            .filter(|shown| shown.used && shown.kind != kind)
            .filter_map(|shown| {
                let bounds = shown.toggle.button.compute_bounds(&self.used)?;
                Some((
                    bounds.x() as f64,
                    bounds.y() as f64,
                    bounds.width() as f64,
                    bounds.height() as f64,
                ))
            })
            .collect();
        cells.sort_by(|first, second| {
            (first.1 - second.1)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    (first.0 - second.0)
                        .partial_cmp(&0.0)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });

        let step = CELL_HEIGHT + SPACING;
        let point_row = (point.1 / step).floor();
        let full = self.used_width() - 1.0;
        let mut index = 0;
        for (x, y, width, height) in cells {
            let cell_row = (y / step).round();
            if point_row < cell_row {
                break;
            }
            if point_row == cell_row {
                let passed = if width >= full {
                    point.1 >= y + height / 2.0
                } else {
                    point.0 >= x + width / 2.0
                };
                if !passed {
                    break;
                }
            }
            index += 1;
        }
        index
    }

    fn drop(self: &Rc<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let stored = self.stored();
        match drag.target {
            Some((true, index)) => self.store(Self::with_inserted(&stored, drag.kind, index)),
            Some((false, _)) => self.store(
                stored
                    .into_iter()
                    .filter(|(entry, _)| entry != drag.kind)
                    .collect(),
            ),
            None => {}
        }
        self.rebuild();
    }

    fn cancel(self: &Rc<Self>) {
        if self.drag.take().is_some() {
            self.rebuild();
        }
    }

    fn stop_hold(&self) {
        if let Some(source) = self.hold.borrow_mut().take() {
            source.remove();
        }
    }

    fn watch_input(self: &Rc<Self>) {
        let gesture = gtk4::GestureDrag::new();
        gesture.set_button(0);
        gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);

        gesture.connect_drag_begin({
            let weak = Rc::downgrade(self);
            move |gesture, x, y| {
                let Some(panel) = weak.upgrade() else {
                    return;
                };
                let button = gesture.current_button();
                if !panel.editing.get() || !(button == 1 || button == 3) {
                    gesture.set_state(gtk4::EventSequenceState::Denied);
                    return;
                }
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                let kind = panel.hit(x, y);
                panel.press.replace(Some(Press {
                    x,
                    y,
                    button,
                    kind,
                    dragging: false,
                }));
                if button == 3 {
                    panel.toggle_size(kind);
                    return;
                }
                let hold = Rc::downgrade(&panel);
                let source = glib::timeout_add_local_once(HOLD, move || {
                    let Some(panel) = hold.upgrade() else {
                        return;
                    };
                    panel.hold.borrow_mut().take();
                    let pending = panel
                        .press
                        .borrow()
                        .as_ref()
                        .filter(|press| !press.dragging)
                        .map(|press| press.kind);
                    if let Some(kind) = pending {
                        panel.toggle_size(kind);
                    }
                });
                if let Some(previous) = panel.hold.replace(Some(source)) {
                    previous.remove();
                }
            }
        });

        gesture.connect_drag_update({
            let weak = Rc::downgrade(self);
            move |_, dx, dy| {
                let Some(panel) = weak.upgrade() else {
                    return;
                };
                let (x, y, begin) = {
                    let mut press = panel.press.borrow_mut();
                    let Some(press) = press.as_mut() else {
                        return;
                    };
                    let Some(kind) = press.kind else {
                        return;
                    };
                    if press.button != 1 {
                        return;
                    }
                    let begin = !press.dragging && dx.hypot(dy) > THRESHOLD;
                    if begin {
                        press.dragging = true;
                    }
                    if !press.dragging {
                        return;
                    }
                    (press.x + dx, press.y + dy, begin.then_some(kind))
                };
                if let Some(kind) = begin {
                    panel.stop_hold();
                    panel.drag.replace(Some(Drag { kind, target: None }));
                    panel.rebuild();
                }
                panel.update_drop_target(x, y);
            }
        });

        gesture.connect_drag_end({
            let weak = Rc::downgrade(self);
            move |_, _, _| {
                let Some(panel) = weak.upgrade() else {
                    return;
                };
                panel.stop_hold();
                let press = panel.press.take();
                if press.is_some_and(|press| press.button == 1 && press.dragging) {
                    panel.drop();
                }
            }
        });

        gesture.connect_cancel({
            let weak = Rc::downgrade(self);
            move |_, _| {
                let Some(panel) = weak.upgrade() else {
                    return;
                };
                panel.stop_hold();
                if panel.press.take().is_some_and(|press| press.dragging) {
                    panel.cancel();
                }
            }
        });

        self.root.add_controller(gesture);
    }
}

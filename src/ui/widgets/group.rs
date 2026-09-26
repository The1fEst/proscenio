use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, rounding, transparentize};
use crate::ui::widgets::ripple::Token;

const COLOR_MILLIS: f64 = 200.0;
const BOUNCE_MILLIS: f64 = 400.0;
const RADIUS_MILLIS: f64 = 200.0;
const SLIDE_MILLIS: f64 = 500.0;
const HOLD: Duration = Duration::from_millis(800);

#[derive(Clone, Copy)]
pub struct Look {
    pub background: Token,
    pub hover: Token,
    pub active: Token,
    pub toggled: Token,
    pub toggled_hover: Token,
    pub toggled_active: Token,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            background: |theme| transparentize(theme.colors.col_layer1_hover, 1.0),
            hover: |theme| theme.colors.col_layer1_hover,
            active: |theme| theme.colors.col_layer1_active,
            toggled: |theme| theme.colors.col_primary,
            toggled_hover: |theme| theme.colors.col_primary_hover,
            toggled_active: |theme| theme.colors.col_primary_active,
        }
    }
}

type Action = Rc<dyn Fn()>;

mod imp {
    use super::*;

    pub struct GroupButton {
        pub theme: RefCell<Option<SharedTheme>>,
        pub look: Cell<Look>,
        pub toggled: Cell<bool>,
        pub enabled: Cell<bool>,
        pub hovered: Cell<bool>,
        pub down: Cell<bool>,
        pub held: Cell<bool>,
        pub base_width: Cell<f64>,
        pub base: Cell<Tween>,
        pub base_height: Cell<f64>,
        pub clicked_width: Cell<Option<f64>>,
        pub clicked_height: Cell<Option<f64>>,
        pub bounce: Cell<bool>,
        pub animate_size: Cell<bool>,
        pub animate_size_hovered_only: Cell<bool>,
        pub width: Cell<Tween>,
        pub height: Cell<Tween>,
        pub radius: Cell<f64>,
        pub radius_pressed: Cell<f64>,
        pub left_radius: Cell<Tween>,
        pub right_radius: Cell<Tween>,
        pub sides: Cell<Option<(f64, f64)>>,
        pub fill: Cell<Option<bool>>,
        pub content: RefCell<Option<gtk4::Widget>>,
        pub shown: Cell<Option<RGBA>>,
        pub fade_from: Cell<Option<RGBA>>,
        pub fade_start: Cell<i64>,
        pub ticking: Cell<bool>,
        pub hold: RefCell<Option<glib::SourceId>>,
        pub clicked: RefCell<Vec<Action>>,
        pub pressed: RefCell<Vec<Action>>,
        pub alt: RefCell<Option<Action>>,
        pub middle: RefCell<Option<Action>>,
        pub listeners: RefCell<Vec<Action>>,
    }

    impl Default for GroupButton {
        fn default() -> Self {
            let small = rounding::SMALL as f64;
            GroupButton {
                theme: RefCell::new(None),
                look: Cell::new(Look::default()),
                toggled: Cell::new(false),
                enabled: Cell::new(true),
                hovered: Cell::new(false),
                down: Cell::new(false),
                held: Cell::new(false),
                base_width: Cell::new(0.0),
                base: Cell::new(Tween::new(0.0, SLIDE_MILLIS, EXPRESSIVE_DEFAULT)),
                base_height: Cell::new(0.0),
                clicked_width: Cell::new(None),
                clicked_height: Cell::new(None),
                bounce: Cell::new(true),
                animate_size: Cell::new(true),
                animate_size_hovered_only: Cell::new(false),
                width: Cell::new(Tween::new(0.0, BOUNCE_MILLIS, EXPRESSIVE_DEFAULT)),
                height: Cell::new(Tween::new(0.0, BOUNCE_MILLIS, EXPRESSIVE_DEFAULT)),
                radius: Cell::new(small),
                radius_pressed: Cell::new(small),
                left_radius: Cell::new(Tween::new(small, RADIUS_MILLIS, EXPRESSIVE_EFFECTS)),
                right_radius: Cell::new(Tween::new(small, RADIUS_MILLIS, EXPRESSIVE_EFFECTS)),
                sides: Cell::new(None),
                fill: Cell::new(None),
                content: RefCell::new(None),
                shown: Cell::new(None),
                fade_from: Cell::new(None),
                fade_start: Cell::new(0),
                ticking: Cell::new(false),
                hold: RefCell::new(None),
                clicked: RefCell::new(Vec::new()),
                pressed: RefCell::new(Vec::new()),
                alt: RefCell::new(None),
                middle: RefCell::new(None),
                listeners: RefCell::new(Vec::new()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GroupButton {
        const NAME: &'static str = "ProscenioGroupButton";
        type Type = super::GroupButton;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for GroupButton {
        fn constructed(&self) {
            self.parent_constructed();
            let button = self.obj();
            button.set_cursor_from_name(Some("pointer"));

            let motion = gtk4::EventControllerMotion::new();
            motion.connect_enter({
                let button = button.downgrade();
                move |_, _, _| {
                    if let Some(button) = button.upgrade()
                        && button.imp().enabled.get()
                    {
                        button.imp().hovered.set(true);
                        button.imp().changed();
                    }
                }
            });
            motion.connect_leave({
                let button = button.downgrade();
                move |_| {
                    if let Some(button) = button.upgrade() {
                        button.imp().hovered.set(false);
                        button.imp().changed();
                    }
                }
            });
            button.add_controller(motion);

            let click = gtk4::GestureClick::new();
            click.set_button(0);
            click.connect_pressed({
                let button = button.downgrade();
                move |gesture, _, _, _| {
                    let Some(button) = button.upgrade() else {
                        return;
                    };
                    let imp = button.imp();
                    if !imp.enabled.get() {
                        gesture.set_state(gtk4::EventSequenceState::Denied);
                        return;
                    }
                    match gesture.current_button() {
                        3 => {
                            let action = imp.alt.borrow().clone();
                            if let Some(action) = action {
                                action();
                            }
                        }
                        2 => {
                            let action = imp.middle.borrow().clone();
                            if let Some(action) = action {
                                action();
                            }
                        }
                        _ => {
                            gesture.set_state(gtk4::EventSequenceState::Claimed);
                            imp.held.set(false);
                            imp.set_down(true);
                            let actions: Vec<Action> = imp.pressed.borrow().clone();
                            for action in actions {
                                action();
                            }
                            let weak = button.downgrade();
                            let source = glib::timeout_add_local_once(HOLD, move || {
                                let Some(button) = weak.upgrade() else {
                                    return;
                                };
                                let imp = button.imp();
                                imp.hold.borrow_mut().take();
                                imp.held.set(true);
                                let action = imp.alt.borrow().clone();
                                if let Some(action) = action {
                                    action();
                                    imp.set_down(false);
                                }
                            });
                            if let Some(previous) = imp.hold.borrow_mut().replace(source) {
                                previous.remove();
                            }
                        }
                    }
                }
            });
            click.connect_released({
                let button = button.downgrade();
                move |gesture, _, x, y| {
                    let Some(button) = button.upgrade() else {
                        return;
                    };
                    let imp = button.imp();
                    imp.stop_hold();
                    imp.set_down(false);
                    if gesture.current_button() != 1 || imp.held.get() {
                        return;
                    }
                    let inside = x >= 0.0
                        && y >= 0.0
                        && x < button.width() as f64
                        && y < button.height() as f64;
                    if !inside {
                        return;
                    }
                    let actions: Vec<Action> = imp.clicked.borrow().clone();
                    for action in actions {
                        action();
                    }
                }
            });
            click.connect_cancel({
                let button = button.downgrade();
                move |_, _| {
                    if let Some(button) = button.upgrade() {
                        button.imp().stop_hold();
                        button.imp().set_down(false);
                    }
                }
            });
            button.add_controller(click);
        }

        fn dispose(&self) {
            self.stop_hold();
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for GroupButton {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let size = match orientation {
                gtk4::Orientation::Horizontal => self.current_width(),
                _ => self.height.get().value(self.now()),
            }
            .round()
            .max(0.0) as i32;
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            crate::ui::widgets::row::present_popovers(&*self.obj());
            if let Some(child) = self.content.borrow().as_ref() {
                child.allocate(width, height, -1, None);
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let button = self.obj();
            let (width, height) = (button.width() as f32, button.height() as f32);
            let now = self.now();
            let limit = width.min(height) / 2.0;
            let left = (self.left_radius.get().value(now) as f32).clamp(0.0, limit);
            let right = (self.right_radius.get().value(now) as f32).clamp(0.0, limit);
            let bounds = graphene::Rect::new(0.0, 0.0, width, height);
            let shape = gsk::RoundedRect::new(
                bounds,
                graphene::Size::new(left, left),
                graphene::Size::new(right, right),
                graphene::Size::new(right, right),
                graphene::Size::new(left, left),
            );
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&self.current_colour(now), &bounds);
            snapshot.pop();
            self.parent_snapshot(snapshot);
        }
    }

    impl GroupButton {
        pub fn now(&self) -> i64 {
            self.obj()
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time)
        }

        fn stop_hold(&self) {
            if let Some(source) = self.hold.borrow_mut().take() {
                source.remove();
            }
        }

        pub fn set_down(&self, down: bool) {
            if self.down.replace(down) == down {
                return;
            }
            if down && let Some(group) = self.obj().parent().and_downcast::<super::ButtonGroup>() {
                group.set_click_index(index_in_parent(&*self.obj()));
            }
            self.resize();
            self.changed();
        }

        pub fn clicked_width(&self) -> f64 {
            if let Some(width) = self.clicked_width.get() {
                return width;
            }
            let button = self.obj();
            let at_side = button.prev_sibling().is_none() || button.next_sibling().is_none();
            self.base_width.get() + if at_side { 10.0 } else { 20.0 }
        }

        pub fn current_width(&self) -> f64 {
            let now = self.now();
            let width = self.width.get();
            if width.running(now) || self.down.get() {
                width.value(now)
            } else {
                self.base.get().value(now)
            }
        }

        pub fn current_height(&self) -> f64 {
            self.height.get().value(self.now())
        }

        pub fn resize(&self) {
            let now = self.now();
            let pressed = self.down.get() && self.bounce.get();
            let width = if pressed {
                self.clicked_width()
            } else {
                self.base_width.get()
            };
            let height = match self.clicked_height.get() {
                Some(clicked) if pressed => clicked,
                _ => self.base_height.get(),
            };
            let animate = self.animate_size.get()
                && (!self.animate_size_hovered_only.get() || self.hovered.get());
            let mut width_tween = self.width.get();
            let mut height_tween = self.height.get();
            if animate {
                width_tween.retarget(width, now);
                height_tween.retarget(height, now);
            } else {
                width_tween.jump(width);
                height_tween.jump(height);
            }
            self.width.set(width_tween);
            self.height.set(height_tween);

            let radius = self.effective_radius();
            let (left_target, right_target) = self.sides.get().unwrap_or((radius, radius));
            let mut left = self.left_radius.get();
            let mut right = self.right_radius.get();
            left.retarget(left_target, now);
            right.retarget(right_target, now);
            self.left_radius.set(left);
            self.right_radius.set(right);

            self.obj().queue_resize();
            if let Some(parent) = self.obj().parent() {
                parent.queue_resize();
            }
            self.tick();
        }

        pub fn effective_radius(&self) -> f64 {
            if self.down.get() {
                self.radius_pressed.get()
            } else {
                self.radius.get()
            }
        }

        fn target_colour(&self) -> RGBA {
            let Some(shared) = self.theme.borrow().clone() else {
                return RGBA::new(0.0, 0.0, 0.0, 0.0);
            };
            let theme = shared.borrow();
            let look = self.look.get();
            if !self.enabled.get() {
                return (look.background)(&theme);
            }
            let (down, hovered) = (self.down.get(), self.hovered.get());
            let token = match (self.toggled.get(), down, hovered) {
                (true, true, _) => look.toggled_active,
                (true, false, true) => look.toggled_hover,
                (true, false, false) => look.toggled,
                (false, true, _) => look.active,
                (false, false, true) => look.hover,
                (false, false, false) => look.background,
            };
            token(&theme)
        }

        fn current_colour(&self, now: i64) -> RGBA {
            let target = self.target_colour();
            let Some(from) = self.fade_from.get() else {
                self.shown.set(Some(target));
                return target;
            };
            let part =
                ((now - self.fade_start.get()) as f64 / (COLOR_MILLIS * 1000.0)).clamp(0.0, 1.0);
            if part >= 1.0 {
                self.fade_from.set(None);
                self.shown.set(Some(target));
                return target;
            }
            let eased = EXPRESSIVE_EFFECTS.at(part) as f32;
            let colour = RGBA::new(
                from.red() + (target.red() - from.red()) * eased,
                from.green() + (target.green() - from.green()) * eased,
                from.blue() + (target.blue() - from.blue()) * eased,
                from.alpha() + (target.alpha() - from.alpha()) * eased,
            );
            self.shown.set(Some(colour));
            colour
        }

        pub fn changed(&self) {
            if let Some(shown) = self.shown.get() {
                self.fade_from.set(Some(shown));
                self.fade_start.set(self.now());
                self.tick();
            }
            self.obj().queue_draw();
            let listeners: Vec<Action> = self.listeners.borrow().clone();
            for listener in listeners {
                listener();
            }
        }

        fn animating(&self, now: i64) -> bool {
            self.fade_from.get().is_some()
                || self.base.get().running(now)
                || self.width.get().running(now)
                || self.height.get().running(now)
                || self.left_radius.get().running(now)
                || self.right_radius.get().running(now)
        }

        fn tick(&self) {
            if self.ticking.replace(true) {
                return;
            }
            self.obj().add_tick_callback(|button, clock| {
                let imp = button.imp();
                let now = clock.frame_time();
                if imp.width.get().running(now)
                    || imp.height.get().running(now)
                    || imp.base.get().running(now)
                {
                    button.queue_resize();
                    if let Some(parent) = button.parent() {
                        parent.queue_resize();
                    }
                }
                button.queue_draw();
                if imp.animating(now) {
                    return glib::ControlFlow::Continue;
                }
                imp.ticking.set(false);
                button.queue_resize();
                glib::ControlFlow::Break
            });
        }
    }

    #[derive(Default)]
    pub struct ButtonGroup {
        pub theme: RefCell<Option<SharedTheme>>,
        pub vertical: Cell<bool>,
        pub spacing: Cell<f64>,
        pub padding: Cell<f64>,
        pub colour: Cell<Option<Token>>,
        pub click_index: Cell<i32>,
        pub phantoms: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ButtonGroup {
        const NAME: &'static str = "ProscenioButtonGroup";
        type Type = super::ButtonGroup;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for ButtonGroup {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ButtonGroup {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let padding = self.padding.get() * 2.0;
            let size = if orientation == self.along() {
                self.content_length() + padding
            } else {
                self.visible_children()
                    .iter()
                    .map(|child| child.measure(orientation, -1).1)
                    .max()
                    .unwrap_or(0) as f64
                    + padding
            }
            .round() as i32;
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let padding = self.padding.get();
            let spacing = self.spacing.get();
            let visible = self.visible_children();
            if visible.is_empty() {
                return;
            }
            let vertical = self.vertical.get();
            let (length, across) = if vertical {
                (height, width)
            } else {
                (width, height)
            };
            let inner_across = across as f64 - padding * 2.0;
            let available = length as f64 - padding * 2.0 - spacing * (visible.len() as f64 - 1.0);

            let hints: Vec<(f64, f64, f64)> = visible
                .iter()
                .map(|child| {
                    let preferred = match child.downcast_ref::<super::GroupButton>() {
                        Some(button) if vertical => button.imp().current_height(),
                        Some(button) => button.imp().current_width(),
                        None => child.measure(self.along(), -1).1 as f64,
                    };
                    if self.fills(child) {
                        (0.0, preferred, f64::INFINITY)
                    } else {
                        (preferred, preferred, preferred)
                    }
                })
                .collect();
            let sizes = distribute(&hints, available);

            let crossing = if vertical {
                gtk4::Orientation::Horizontal
            } else {
                gtk4::Orientation::Vertical
            };
            let mut start = padding;
            for (child, size) in visible.iter().zip(sizes) {
                let natural = child.measure(crossing, -1).1 as f64;
                let child_across = if self.fills(child) {
                    inner_across
                } else {
                    natural.min(inner_across)
                };
                let offset = padding + ((inner_across - child_across) / 2.0 + 0.5).floor();
                let near = start.round();
                let far = (start + size).round();
                let extent = (far - near).max(0.0) as i32;
                let (child_width, child_height, x, y) = if vertical {
                    (child_across.round() as i32, extent, offset, near)
                } else {
                    (extent, child_across.round() as i32, near, offset)
                };
                child.allocate(
                    child_width,
                    child_height,
                    -1,
                    Some(
                        gtk4::gsk::Transform::new()
                            .translate(&graphene::Point::new(x as f32, y as f32)),
                    ),
                );
                start += size + spacing;
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            if let Some(token) = self.colour.get()
                && let Some(shared) = self.theme.borrow().clone()
            {
                let group = self.obj();
                let (width, height) = (group.width() as f32, group.height() as f32);
                let padding = self.padding.get();
                let limit = width.min(height) / 2.0;
                let corner = |child: Option<gtk4::Widget>| {
                    let radius = child
                        .and_downcast::<super::GroupButton>()
                        .map(|button| button.imp().effective_radius() + padding)
                        .unwrap_or(rounding::SMALL as f64);
                    (radius as f32).min(limit)
                };
                let first = corner(group.first_child());
                let last = corner(group.last_child());
                let (top_left, top_right, bottom_right, bottom_left) = if self.vertical.get() {
                    (first, first, last, last)
                } else {
                    (first, last, last, first)
                };
                let bounds = graphene::Rect::new(0.0, 0.0, width, height);
                let shape = gsk::RoundedRect::new(
                    bounds,
                    graphene::Size::new(top_left, top_left),
                    graphene::Size::new(top_right, top_right),
                    graphene::Size::new(bottom_right, bottom_right),
                    graphene::Size::new(bottom_left, bottom_left),
                );
                snapshot.push_rounded_clip(&shape);
                snapshot.append_color(&token(&shared.borrow()), &bounds);
                snapshot.pop();
            }
            self.parent_snapshot(snapshot);
        }
    }

    impl ButtonGroup {
        pub fn visible_children(&self) -> Vec<gtk4::Widget> {
            let mut children = Vec::new();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                if widget.get_visible() && !widget.is::<gtk4::Popover>() {
                    children.push(widget.clone());
                }
                child = widget.next_sibling();
            }
            children
        }

        fn along(&self) -> gtk4::Orientation {
            if self.vertical.get() {
                gtk4::Orientation::Vertical
            } else {
                gtk4::Orientation::Horizontal
            }
        }

        fn content_length(&self) -> f64 {
            let mut total = 0.0;
            let mut count = self.phantoms.get();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                count += 1;
                if widget.get_visible() {
                    total += match widget.downcast_ref::<super::GroupButton>() {
                        Some(button) if self.vertical.get() => button.imp().base_height.get(),
                        Some(button) => {
                            let imp = button.imp();
                            imp.base.get().value(imp.now())
                        }
                        None => widget.measure(self.along(), -1).1 as f64,
                    };
                }
                child = widget.next_sibling();
            }
            total + self.spacing.get() * (count as f64 - 1.0).max(0.0)
        }

        fn fills(&self, child: &gtk4::Widget) -> bool {
            let Some(button) = child.downcast_ref::<super::GroupButton>() else {
                return false;
            };
            if let Some(fill) = button.imp().fill.get() {
                return fill;
            }
            let index = index_in_parent(child);
            let click = self.click_index.get();
            click - 1 <= index && index <= click + 1
        }
    }

    fn distribute(hints: &[(f64, f64, f64)], target: f64) -> Vec<f64> {
        let minimum: f64 = hints.iter().map(|hint| hint.0).sum();
        let preferred: f64 = hints.iter().map(|hint| hint.1).sum();
        if target <= minimum {
            return hints.iter().map(|hint| hint.0).collect();
        }
        if target < preferred {
            let available = target - minimum;
            let desired_total = preferred - minimum;
            let factors: Vec<f64> = hints
                .iter()
                .map(|hint| {
                    let desired = hint.1 - hint.0;
                    if desired <= 0.0 {
                        return 0.0;
                    }
                    desired * (available / desired_total).powf(desired / desired_total)
                })
                .collect();
            let sum: f64 = factors.iter().sum();
            return hints
                .iter()
                .zip(factors)
                .map(|(hint, factor)| {
                    if sum <= 0.0 {
                        hint.0
                    } else {
                        hint.0 + available * factor / sum
                    }
                })
                .collect();
        }
        let growing = hints.iter().filter(|hint| hint.2 > hint.1).count();
        if growing == 0 {
            return hints.iter().map(|hint| hint.1).collect();
        }
        let extra = (target - preferred) / growing as f64;
        hints
            .iter()
            .map(|hint| {
                if hint.2 > hint.1 {
                    (hint.1 + extra).min(hint.2)
                } else {
                    hint.1
                }
            })
            .collect()
    }

    pub fn index_in_parent(widget: &impl IsA<gtk4::Widget>) -> i32 {
        let mut index = 0;
        let mut sibling = widget.as_ref().prev_sibling();
        while let Some(previous) = sibling {
            index += 1;
            sibling = previous.prev_sibling();
        }
        index
    }
}

glib::wrapper! {
    pub struct GroupButton(ObjectSubclass<imp::GroupButton>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl GroupButton {
    pub fn new(theme: &SharedTheme, base_width: f64, base_height: f64) -> Self {
        let button: GroupButton = glib::Object::new();
        let imp = button.imp();
        imp.theme.replace(Some(theme.clone()));
        imp.base_width.set(base_width);
        imp.base
            .set(Tween::new(base_width, SLIDE_MILLIS, EXPRESSIVE_DEFAULT));
        imp.base_height.set(base_height);
        imp.width
            .set(Tween::new(base_width, BOUNCE_MILLIS, EXPRESSIVE_DEFAULT));
        imp.height
            .set(Tween::new(base_height, BOUNCE_MILLIS, EXPRESSIVE_DEFAULT));
        button
    }

    pub fn set_look(&self, look: Look) {
        self.imp().look.set(look);
        self.imp().changed();
    }

    pub fn set_toggled(&self, toggled: bool) {
        if self.imp().toggled.replace(toggled) != toggled {
            self.imp().changed();
        }
    }

    pub fn toggled(&self) -> bool {
        self.imp().toggled.get()
    }

    pub fn set_enabled(&self, enabled: bool) {
        let imp = self.imp();
        if imp.enabled.replace(enabled) == enabled {
            return;
        }
        if !enabled {
            imp.hovered.set(false);
            imp.set_down(false);
        }
        self.set_cursor_from_name(enabled.then_some("pointer"));
        if let Some(content) = imp.content.borrow().as_ref() {
            tree_cursor(content, enabled.then_some("pointer"));
        }
        imp.changed();
    }

    pub fn enabled(&self) -> bool {
        self.imp().enabled.get()
    }

    pub fn hovered(&self) -> bool {
        self.imp().hovered.get()
    }

    pub fn down(&self) -> bool {
        self.imp().down.get()
    }

    pub fn set_base_size(&self, width: f64, height: f64) {
        let imp = self.imp();
        imp.base_width.set(width);
        let mut base = imp.base.get();
        base.jump(width);
        imp.base.set(base);
        imp.base_height.set(height);
        imp.resize();
    }

    pub fn slide_base_width(&self, from: f64, to: f64) {
        let imp = self.imp();
        imp.base_width.set(to);
        let mut base = imp.base.get();
        base.jump(from);
        base.retarget(to, imp.now());
        imp.base.set(base);
        let mut width = imp.width.get();
        width.jump(to);
        imp.width.set(width);
        imp.resize();
    }

    pub fn current_width(&self) -> f64 {
        self.imp().current_width()
    }

    pub fn content(&self) -> Option<gtk4::Widget> {
        self.imp().content.borrow().clone()
    }

    pub fn set_clicked_width(&self, width: f64) {
        self.imp().clicked_width.set(Some(width));
    }

    pub fn set_clicked_height(&self, height: f64) {
        self.imp().clicked_height.set(Some(height));
    }

    pub fn set_bounce(&self, bounce: bool) {
        self.imp().bounce.set(bounce);
    }

    pub fn set_size_animation(&self, enabled: bool, hovered_only: bool) {
        self.imp().animate_size.set(enabled);
        self.imp().animate_size_hovered_only.set(hovered_only);
    }

    pub fn set_radii(&self, radius: f64, pressed: f64) {
        let imp = self.imp();
        imp.radius.set(radius);
        imp.radius_pressed.set(pressed);
        imp.resize();
    }

    pub fn set_side_radii(&self, left: f64, right: f64) {
        self.imp().sides.set(Some((left, right)));
        self.imp().resize();
    }

    pub fn jump_radius(&self) {
        let imp = self.imp();
        let radius = imp.effective_radius();
        let (left_target, right_target) = imp.sides.get().unwrap_or((radius, radius));
        let mut left = imp.left_radius.get();
        let mut right = imp.right_radius.get();
        left.jump(left_target);
        right.jump(right_target);
        imp.left_radius.set(left);
        imp.right_radius.set(right);
        self.queue_draw();
    }

    pub fn set_fill(&self, fill: bool) {
        self.imp().fill.set(Some(fill));
    }

    pub fn set_content(&self, child: &impl IsA<gtk4::Widget>) {
        if let Some(previous) = self.imp().content.replace(Some(child.clone().upcast())) {
            previous.unparent();
        }
        child.set_parent(self);
        pointer_cursor(child.as_ref());
    }

    pub fn connect_clicked(&self, action: impl Fn() + 'static) {
        self.imp().clicked.borrow_mut().push(Rc::new(action));
    }

    pub fn connect_down(&self, action: impl Fn() + 'static) {
        self.imp().pressed.borrow_mut().push(Rc::new(action));
    }

    pub fn connect_alt(&self, action: impl Fn() + 'static) {
        self.imp().alt.replace(Some(Rc::new(action)));
    }

    pub fn connect_middle(&self, action: impl Fn() + 'static) {
        self.imp().middle.replace(Some(Rc::new(action)));
    }

    pub fn connect_changed(&self, action: impl Fn() + 'static) {
        self.imp().listeners.borrow_mut().push(Rc::new(action));
    }

    pub fn radius(&self) -> f64 {
        self.imp().effective_radius()
    }
}

fn tree_cursor(widget: &gtk4::Widget, name: Option<&str>) {
    if !widget.is::<gtk4::Popover>() {
        widget.set_cursor_from_name(name);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        tree_cursor(&current, name);
        child = current.next_sibling();
    }
}

pub fn pointer_cursor(widget: &gtk4::Widget) {
    if widget.cursor().is_none() && !widget.is::<gtk4::Popover>() {
        widget.set_cursor_from_name(Some("pointer"));
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        pointer_cursor(&current);
        child = current.next_sibling();
    }
}

glib::wrapper! {
    pub struct ButtonGroup(ObjectSubclass<imp::ButtonGroup>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl ButtonGroup {
    pub fn new(theme: &SharedTheme) -> Self {
        let group: ButtonGroup = glib::Object::new();
        let imp = group.imp();
        imp.theme.replace(Some(theme.clone()));
        imp.spacing.set(5.0);
        imp.click_index.set(-1);
        group
    }

    pub fn set_vertical(&self, vertical: bool) {
        self.imp().vertical.set(vertical);
        self.queue_resize();
    }

    pub fn set_spacing(&self, spacing: f64) {
        self.imp().spacing.set(spacing);
        self.queue_resize();
    }

    pub fn set_phantom_children(&self, count: i32) {
        self.imp().phantoms.set(count);
        self.queue_resize();
    }

    pub fn set_padding(&self, padding: f64) {
        self.imp().padding.set(padding);
        self.queue_resize();
    }

    pub fn set_colour(&self, colour: Token) {
        self.imp().colour.set(Some(colour));
        self.queue_draw();
    }

    pub fn append(&self, child: &impl IsA<gtk4::Widget>) {
        child.set_parent(self);
        self.queue_resize();
    }

    fn set_click_index(&self, index: i32) {
        self.imp().click_index.set(index);
        self.queue_allocate();
    }
}

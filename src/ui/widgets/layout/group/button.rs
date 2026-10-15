use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::{ButtonGroup, index_in_parent, pointer_cursor, tree_cursor};
use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, rounding, transparentize};
use crate::ui::widgets::input::ripple::Token;

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
        pub is_toggled: Cell<bool>,
        pub is_enabled: Cell<bool>,
        pub is_hovered: Cell<bool>,
        pub is_down: Cell<bool>,
        pub is_held: Cell<bool>,
        pub base_width: Cell<f64>,
        pub base: Cell<Tween>,
        pub base_height: Cell<f64>,
        pub clicked_width: Cell<Option<f64>>,
        pub clicked_height: Cell<Option<f64>>,
        pub bounces: Cell<bool>,
        pub is_size_animated: Cell<bool>,
        pub is_size_animation_hover_only: Cell<bool>,
        pub width: Cell<Tween>,
        pub height: Cell<Tween>,
        pub radius: Cell<f64>,
        pub radius_pressed: Cell<f64>,
        pub left_radius: Cell<Tween>,
        pub right_radius: Cell<Tween>,
        pub sides: Cell<Option<(f64, f64)>>,
        pub fill_override: Cell<Option<bool>>,
        pub content: RefCell<Option<gtk4::Widget>>,
        pub displayed_color: Cell<Option<RGBA>>,
        pub fade_from: Cell<Option<RGBA>>,
        pub fade_start: Cell<i64>,
        pub is_ticking: Cell<bool>,
        pub hold: RefCell<Option<glib::SourceId>>,
        pub on_clicked: RefCell<Vec<Action>>,
        pub on_down: RefCell<Vec<Action>>,
        pub on_alt: RefCell<Option<Action>>,
        pub on_middle: RefCell<Option<Action>>,
        pub on_changed: RefCell<Vec<Action>>,
    }

    impl Default for GroupButton {
        fn default() -> Self {
            let small = rounding::SMALL as f64;
            GroupButton {
                theme: RefCell::new(None),
                look: Cell::new(Look::default()),
                is_toggled: Cell::new(false),
                is_enabled: Cell::new(true),
                is_hovered: Cell::new(false),
                is_down: Cell::new(false),
                is_held: Cell::new(false),
                base_width: Cell::new(0.0),
                base: Cell::new(Tween::new(0.0, SLIDE_MILLIS, EXPRESSIVE_DEFAULT)),
                base_height: Cell::new(0.0),
                clicked_width: Cell::new(None),
                clicked_height: Cell::new(None),
                bounces: Cell::new(true),
                is_size_animated: Cell::new(true),
                is_size_animation_hover_only: Cell::new(false),
                width: Cell::new(Tween::new(0.0, BOUNCE_MILLIS, EXPRESSIVE_DEFAULT)),
                height: Cell::new(Tween::new(0.0, BOUNCE_MILLIS, EXPRESSIVE_DEFAULT)),
                radius: Cell::new(small),
                radius_pressed: Cell::new(small),
                left_radius: Cell::new(Tween::new(small, RADIUS_MILLIS, EXPRESSIVE_EFFECTS)),
                right_radius: Cell::new(Tween::new(small, RADIUS_MILLIS, EXPRESSIVE_EFFECTS)),
                sides: Cell::new(None),
                fill_override: Cell::new(None),
                content: RefCell::new(None),
                displayed_color: Cell::new(None),
                fade_from: Cell::new(None),
                fade_start: Cell::new(0),
                is_ticking: Cell::new(false),
                hold: RefCell::new(None),
                on_clicked: RefCell::new(Vec::new()),
                on_down: RefCell::new(Vec::new()),
                on_alt: RefCell::new(None),
                on_middle: RefCell::new(None),
                on_changed: RefCell::new(Vec::new()),
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

            let motion_controller = gtk4::EventControllerMotion::new();
            motion_controller.connect_enter({
                let button = button.downgrade();
                move |_, _, _| {
                    if let Some(button) = button.upgrade()
                        && button.imp().is_enabled.get()
                    {
                        button.imp().is_hovered.set(true);
                        button.imp().notify_changed();
                    }
                }
            });
            motion_controller.connect_leave({
                let button = button.downgrade();
                move |_| {
                    if let Some(button) = button.upgrade() {
                        button.imp().is_hovered.set(false);
                        button.imp().notify_changed();
                    }
                }
            });
            button.add_controller(motion_controller);

            let click = gtk4::GestureClick::new();
            click.set_button(0);
            click.connect_pressed({
                let button = button.downgrade();
                move |gesture, _, _, _| {
                    let Some(button) = button.upgrade() else {
                        return;
                    };
                    let imp = button.imp();
                    if !imp.is_enabled.get() {
                        gesture.set_state(gtk4::EventSequenceState::Denied);
                        return;
                    }
                    match gesture.current_button() {
                        3 => {
                            let action = imp.on_alt.borrow().clone();
                            if let Some(action) = action {
                                action();
                            }
                        }
                        2 => {
                            let action = imp.on_middle.borrow().clone();
                            if let Some(action) = action {
                                action();
                            }
                        }
                        _ => {
                            gesture.set_state(gtk4::EventSequenceState::Claimed);
                            imp.is_held.set(false);
                            imp.set_down(true);
                            let actions: Vec<Action> = imp.on_down.borrow().clone();
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
                                imp.is_held.set(true);
                                let action = imp.on_alt.borrow().clone();
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
                    if gesture.current_button() != 1 || imp.is_held.get() {
                        return;
                    }
                    let is_inside = x >= 0.0
                        && y >= 0.0
                        && x < button.width() as f64
                        && y < button.height() as f64;
                    if !is_inside {
                        return;
                    }
                    let actions: Vec<Action> = imp.on_clicked.borrow().clone();
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
            crate::ui::widgets::layout::row::present_popovers(&*self.obj());
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
            snapshot.append_color(&self.current_color(now), &bounds);
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

        pub fn set_down(&self, is_down: bool) {
            if self.is_down.replace(is_down) == is_down {
                return;
            }
            if is_down && let Some(group) = self.obj().parent().and_downcast::<ButtonGroup>() {
                group.set_click_index(index_in_parent(&*self.obj()));
            }
            self.resize();
            self.notify_changed();
        }

        pub fn clicked_width(&self) -> f64 {
            if let Some(width) = self.clicked_width.get() {
                return width;
            }
            let button = self.obj();
            let is_at_side = button.prev_sibling().is_none() || button.next_sibling().is_none();
            self.base_width.get() + if is_at_side { 10.0 } else { 20.0 }
        }

        pub fn current_width(&self) -> f64 {
            let now = self.now();
            let width = self.width.get();
            if width.is_running(now) || self.is_down.get() {
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
            let shows_pressed_size = self.is_down.get() && self.bounces.get();
            let width = if shows_pressed_size {
                self.clicked_width()
            } else {
                self.base_width.get()
            };
            let height = match self.clicked_height.get() {
                Some(clicked) if shows_pressed_size => clicked,
                _ => self.base_height.get(),
            };
            let should_animate = self.is_size_animated.get()
                && (!self.is_size_animation_hover_only.get() || self.is_hovered.get());
            let mut width_tween = self.width.get();
            let mut height_tween = self.height.get();
            if should_animate {
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
            if self.is_down.get() {
                self.radius_pressed.get()
            } else {
                self.radius.get()
            }
        }

        fn target_color(&self) -> RGBA {
            let Some(shared) = self.theme.borrow().clone() else {
                return RGBA::new(0.0, 0.0, 0.0, 0.0);
            };
            let theme = shared.borrow();
            let look = self.look.get();
            if !self.is_enabled.get() {
                return (look.background)(&theme);
            }
            let (is_down, is_hovered) = (self.is_down.get(), self.is_hovered.get());
            let token = match (self.is_toggled.get(), is_down, is_hovered) {
                (true, true, _) => look.toggled_active,
                (true, false, true) => look.toggled_hover,
                (true, false, false) => look.toggled,
                (false, true, _) => look.active,
                (false, false, true) => look.hover,
                (false, false, false) => look.background,
            };
            token(&theme)
        }

        fn current_color(&self, now: i64) -> RGBA {
            let target = self.target_color();
            let Some(from) = self.fade_from.get() else {
                self.displayed_color.set(Some(target));
                return target;
            };
            let part =
                ((now - self.fade_start.get()) as f64 / (COLOR_MILLIS * 1000.0)).clamp(0.0, 1.0);
            if part >= 1.0 {
                self.fade_from.set(None);
                self.displayed_color.set(Some(target));
                return target;
            }
            let eased_progress = EXPRESSIVE_EFFECTS.at(part) as f32;
            let color = RGBA::new(
                from.red() + (target.red() - from.red()) * eased_progress,
                from.green() + (target.green() - from.green()) * eased_progress,
                from.blue() + (target.blue() - from.blue()) * eased_progress,
                from.alpha() + (target.alpha() - from.alpha()) * eased_progress,
            );
            self.displayed_color.set(Some(color));
            color
        }

        pub fn notify_changed(&self) {
            if let Some(displayed_color) = self.displayed_color.get() {
                self.fade_from.set(Some(displayed_color));
                self.fade_start.set(self.now());
                self.tick();
            }
            self.obj().queue_draw();
            let listeners: Vec<Action> = self.on_changed.borrow().clone();
            for listener in listeners {
                listener();
            }
        }

        fn is_animating(&self, now: i64) -> bool {
            self.fade_from.get().is_some()
                || self.base.get().is_running(now)
                || self.width.get().is_running(now)
                || self.height.get().is_running(now)
                || self.left_radius.get().is_running(now)
                || self.right_radius.get().is_running(now)
        }

        fn tick(&self) {
            if self.is_ticking.replace(true) {
                return;
            }
            self.obj().add_tick_callback(|button, clock| {
                let imp = button.imp();
                let now = clock.frame_time();
                if imp.width.get().is_running(now)
                    || imp.height.get().is_running(now)
                    || imp.base.get().is_running(now)
                {
                    button.queue_resize();
                    if let Some(parent) = button.parent() {
                        parent.queue_resize();
                    }
                }
                button.queue_draw();
                if imp.is_animating(now) {
                    return glib::ControlFlow::Continue;
                }
                imp.is_ticking.set(false);
                button.queue_resize();
                glib::ControlFlow::Break
            });
        }
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
        self.imp().notify_changed();
    }

    pub fn set_toggled(&self, is_toggled: bool) {
        if self.imp().is_toggled.replace(is_toggled) != is_toggled {
            self.imp().notify_changed();
        }
    }

    pub fn is_toggled(&self) -> bool {
        self.imp().is_toggled.get()
    }

    pub fn set_enabled(&self, is_enabled: bool) {
        let imp = self.imp();
        if imp.is_enabled.replace(is_enabled) == is_enabled {
            return;
        }
        if !is_enabled {
            imp.is_hovered.set(false);
            imp.set_down(false);
        }
        self.set_cursor_from_name(is_enabled.then_some("pointer"));
        if let Some(content) = imp.content.borrow().as_ref() {
            tree_cursor(content, is_enabled.then_some("pointer"));
        }
        imp.notify_changed();
    }

    pub fn is_enabled(&self) -> bool {
        self.imp().is_enabled.get()
    }

    pub fn is_hovered(&self) -> bool {
        self.imp().is_hovered.get()
    }

    pub fn is_down(&self) -> bool {
        self.imp().is_down.get()
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

    pub fn set_bounce(&self, bounces: bool) {
        self.imp().bounces.set(bounces);
    }

    pub fn set_size_animation(&self, is_enabled: bool, is_hover_only: bool) {
        self.imp().is_size_animated.set(is_enabled);
        self.imp().is_size_animation_hover_only.set(is_hover_only);
    }

    pub fn set_radii(&self, radius: f64, radius_pressed: f64) {
        let imp = self.imp();
        imp.radius.set(radius);
        imp.radius_pressed.set(radius_pressed);
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

    pub fn set_fill(&self, fills: bool) {
        self.imp().fill_override.set(Some(fills));
    }

    pub fn set_content(&self, child: &impl IsA<gtk4::Widget>) {
        if let Some(previous) = self.imp().content.replace(Some(child.clone().upcast())) {
            previous.unparent();
        }
        child.set_parent(self);
        pointer_cursor(child.as_ref());
    }

    pub fn connect_clicked(&self, action: impl Fn() + 'static) {
        self.imp().on_clicked.borrow_mut().push(Rc::new(action));
    }

    pub fn connect_down(&self, action: impl Fn() + 'static) {
        self.imp().on_down.borrow_mut().push(Rc::new(action));
    }

    pub fn connect_alt(&self, action: impl Fn() + 'static) {
        self.imp().on_alt.replace(Some(Rc::new(action)));
    }

    pub fn connect_middle(&self, action: impl Fn() + 'static) {
        self.imp().on_middle.replace(Some(Rc::new(action)));
    }

    pub fn connect_changed(&self, action: impl Fn() + 'static) {
        self.imp().on_changed.borrow_mut().push(Rc::new(action));
    }

    pub fn radius(&self) -> f64 {
        self.imp().effective_radius()
    }
}

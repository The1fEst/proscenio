use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::anim::{EXPRESSIVE_EFFECTS, Ease, Motion, STANDARD_DECEL};
use crate::ui::theme::{SharedTheme, Theme, rounding, transparentize};
use crate::ui::widgets::trimmedbin::TrimmedBin;

const COLOR_MILLIS: f64 = 200.0;

pub type Token = fn(&Theme) -> RGBA;

#[derive(Clone, Copy)]
pub struct Look {
    pub background: Token,
    pub hover: Token,
    pub toggled: Token,
    pub toggled_hover: Token,
    pub ripple: Token,
    pub ripple_toggled: Token,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            background: |theme| transparentize(theme.colors.col_layer1_hover, 1.0),
            hover: |theme| theme.colors.col_layer1_hover,
            toggled: |theme| theme.colors.col_primary,
            toggled_hover: |theme| theme.colors.col_primary_hover,
            ripple: |theme| theme.colors.col_layer1_active,
            ripple_toggled: |theme| theme.colors.col_primary_active,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub background: RGBA,
    pub hover: RGBA,
    pub ripple: RGBA,
}

type Action = Box<dyn Fn(&gtk4::GestureClick)>;

mod imp {
    use super::*;

    pub struct RippleButton {
        pub theme: RefCell<Option<SharedTheme>>,
        pub look: Cell<Look>,
        pub palette: Cell<Option<Palette>>,
        pub toggled: Cell<bool>,
        pub hovered: Cell<bool>,
        pub down: Cell<bool>,
        pub radius: Cell<f64>,
        pub radius_pressed: Cell<Option<f64>>,
        pub radius_motion: RefCell<Option<Rc<Motion>>>,
        pub ripple_enabled: Cell<bool>,
        pub background_size: Cell<Option<(f32, f32)>>,
        pub background_inset: Cell<(f32, f32)>,
        pub ripple_duration: Cell<f64>,
        pub shown: Cell<Option<RGBA>>,
        pub fade_from: Cell<Option<RGBA>>,
        pub fade_start: Cell<i64>,
        pub ripple_x: Cell<f64>,
        pub ripple_y: Cell<f64>,
        pub ripple_reach: Cell<f64>,
        pub ripple_start: Cell<i64>,
        pub ripple_opacity: Cell<f64>,
        pub ripple_fade_from: Cell<f64>,
        pub ripple_fade_start: Cell<i64>,
        pub ticking: Cell<bool>,
        pub alt: RefCell<Option<Action>>,
        pub middle: RefCell<Option<Action>>,
        pub down_action: RefCell<Option<Box<dyn Fn()>>>,
        pub release_action: RefCell<Option<Box<dyn Fn()>>>,
        pub click: RefCell<Option<gtk4::GestureClick>>,
        pub pressed_inside: Cell<bool>,
    }

    impl Default for RippleButton {
        fn default() -> Self {
            RippleButton {
                theme: RefCell::new(None),
                look: Cell::new(Look::default()),
                palette: Cell::new(None),
                toggled: Cell::new(false),
                hovered: Cell::new(false),
                down: Cell::new(false),
                radius: Cell::new(rounding::SMALL as f64),
                radius_pressed: Cell::new(None),
                radius_motion: RefCell::new(None),
                ripple_enabled: Cell::new(true),
                background_size: Cell::new(None),
                background_inset: Cell::new((0.0, 0.0)),
                ripple_duration: Cell::new(1200.0),
                shown: Cell::new(None),
                fade_from: Cell::new(None),
                fade_start: Cell::new(0),
                ripple_x: Cell::new(0.0),
                ripple_y: Cell::new(0.0),
                ripple_reach: Cell::new(0.0),
                ripple_start: Cell::new(-1),
                ripple_opacity: Cell::new(0.0),
                ripple_fade_from: Cell::new(0.0),
                ripple_fade_start: Cell::new(-1),
                ticking: Cell::new(false),
                alt: RefCell::new(None),
                middle: RefCell::new(None),
                down_action: RefCell::new(None),
                release_action: RefCell::new(None),
                click: RefCell::new(None),
                pressed_inside: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RippleButton {
        const NAME: &'static str = "ProscenioRippleButton";
        type Type = super::RippleButton;
        type ParentType = gtk4::Button;
    }

    impl ObjectImpl for RippleButton {
        fn dispose(&self) {
            let mut child = self.obj().first_child();
            while let Some(current) = child {
                child = current.next_sibling();
                if current.is::<gtk4::Popover>() {
                    current.unparent();
                }
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            let button = self.obj();
            button.add_css_class("ripple-button");
            button.set_cursor_from_name(Some("pointer"));
            button.set_size_request(-1, 30);

            let motion = gtk4::EventControllerMotion::new();
            motion.connect_enter({
                let button = button.downgrade();
                move |_, _, _| {
                    if let Some(button) = button.upgrade() {
                        button.imp().hovered.set(true);
                        button.imp().retarget();
                    }
                }
            });
            motion.connect_leave({
                let button = button.downgrade();
                move |_| {
                    if let Some(button) = button.upgrade() {
                        button.imp().hovered.set(false);
                        button.imp().retarget();
                    }
                }
            });
            button.add_controller(motion);

            let click = gtk4::GestureClick::new();
            click.set_button(0);
            click.set_propagation_phase(gtk4::PropagationPhase::Capture);
            click.connect_pressed({
                let button = button.downgrade();
                move |gesture, _, x, y| {
                    let Some(button) = button.upgrade() else {
                        return;
                    };
                    let imp = button.imp();
                    match gesture.current_button() {
                        3 => {
                            if let Some(action) = imp.alt.borrow().as_ref() {
                                action(gesture);
                            }
                        }
                        2 => {
                            if let Some(action) = imp.middle.borrow().as_ref() {
                                action(gesture);
                            }
                        }
                        _ => {
                            let inside = presses_nested_control(&button, x, y);
                            imp.pressed_inside.set(inside);
                            if inside {
                                return;
                            }
                            imp.down.set(true);
                            if let Some(action) = imp.down_action.borrow().as_ref() {
                                action();
                            }
                            if imp.ripple_enabled.get() {
                                imp.start_ripple(x, y);
                            }
                            button.queue_draw();
                        }
                    }
                }
            });
            click.connect_released({
                let button = button.downgrade();
                move |gesture, _, _, _| {
                    let Some(button) = button.upgrade() else {
                        return;
                    };
                    let imp = button.imp();
                    imp.down.set(false);
                    if gesture.current_button() != 1 || imp.pressed_inside.get() {
                        return;
                    }
                    if let Some(action) = imp.release_action.borrow().as_ref() {
                        action();
                    }
                    if imp.ripple_enabled.get() {
                        imp.fade_ripple();
                    }
                    button.queue_draw();
                }
            });
            click.connect_cancel({
                let button = button.downgrade();
                move |_, _| {
                    let Some(button) = button.upgrade() else {
                        return;
                    };
                    let imp = button.imp();
                    imp.down.set(false);
                    if imp.ripple_enabled.get() {
                        imp.fade_ripple();
                    }
                    button.queue_draw();
                }
            });
            self.click.replace(Some(click.clone()));
            button.add_controller(click);

            button.connect_sensitive_notify(|button| {
                button.set_opacity(if button.is_sensitive() { 1.0 } else { 0.4 });
                button.imp().retarget();
            });
        }
    }

    impl WidgetImpl for RippleButton {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let bounds = self.background();
            let (width, height) = (bounds.width(), bounds.height());
            let radius = (self.effective_radius() as f32)
                .min(width / 2.0)
                .min(height / 2.0);
            let shape = gsk::RoundedRect::from_rect(bounds, radius);

            let colour = self.current_background();
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&colour, &bounds);
            self.snapshot_ripple(snapshot);
            snapshot.pop();

            self.parent_snapshot(snapshot);
        }
    }

    impl ButtonImpl for RippleButton {}

    impl RippleButton {
        fn target(&self) -> RGBA {
            let enabled = if self.obj().is_sensitive() { 0.0 } else { 1.0 };
            if let Some(palette) = self.palette.get() {
                let colour = if self.hovered.get() {
                    palette.hover
                } else {
                    palette.background
                };
                return transparentize(colour, enabled);
            }
            let Some(shared) = self.theme.borrow().clone() else {
                return RGBA::new(0.0, 0.0, 0.0, 0.0);
            };
            let theme = shared.borrow();
            let look = self.look.get();
            let colour = match (self.toggled.get(), self.hovered.get()) {
                (true, true) => (look.toggled_hover)(&theme),
                (true, false) => (look.toggled)(&theme),
                (false, true) => (look.hover)(&theme),
                (false, false) => (look.background)(&theme),
            };
            transparentize(colour, enabled)
        }

        pub fn ripple_colour(&self) -> RGBA {
            if let Some(palette) = self.palette.get() {
                return palette.ripple;
            }
            let Some(shared) = self.theme.borrow().clone() else {
                return RGBA::new(0.0, 0.0, 0.0, 0.0);
            };
            let theme = shared.borrow();
            let look = self.look.get();
            if self.toggled.get() {
                (look.ripple_toggled)(&theme)
            } else {
                (look.ripple)(&theme)
            }
        }

        pub fn effective_radius(&self) -> f64 {
            let radius = match self.radius_motion.borrow().as_ref() {
                Some(motion) => motion.get(),
                None => self.radius.get(),
            };
            if self.down.get() {
                self.radius_pressed.get().unwrap_or(radius)
            } else {
                radius
            }
        }

        fn now(&self) -> i64 {
            self.obj()
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time)
        }

        fn current_background(&self) -> RGBA {
            let target = self.target();
            let Some(from) = self.fade_from.get() else {
                self.shown.set(Some(target));
                return target;
            };
            let part = ((self.now() - self.fade_start.get()) as f64 / (COLOR_MILLIS * 1000.0))
                .clamp(0.0, 1.0);
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

        pub fn retarget(&self) {
            if let Some(shown) = self.shown.get() {
                self.fade_from.set(Some(shown));
                self.fade_start.set(self.now());
                self.tick();
            }
            self.obj().queue_draw();
        }

        fn background(&self) -> graphene::Rect {
            let button = self.obj();
            let (width, height) = (button.width() as f32, button.height() as f32);
            match self.background_size.get() {
                Some((inner_width, inner_height)) => graphene::Rect::new(
                    ((width - inner_width) / 2.0).round(),
                    ((height - inner_height) / 2.0).round(),
                    inner_width,
                    inner_height,
                ),
                None => {
                    let (horizontal, vertical) = self.background_inset.get();
                    graphene::Rect::new(
                        horizontal,
                        vertical,
                        width - horizontal * 2.0,
                        height - vertical * 2.0,
                    )
                }
            }
        }

        fn start_ripple(&self, x: f64, y: f64) {
            let width = self.obj().width() as f64;
            let background = self.background();
            let top = background.y() as f64;
            let bottom = top + background.height() as f64;
            let reach = [(0.0, top), (0.0, bottom), (width, top), (width, bottom)]
                .iter()
                .map(|(cx, cy)| cx * cx + cy * cy)
                .fold(0.0, f64::max)
                .sqrt();
            self.ripple_x.set(background.x() as f64 + x);
            self.ripple_y.set(y);
            self.ripple_reach.set(reach);
            self.ripple_fade_start.set(-1);
            self.ripple_opacity.set(1.0);
            self.ripple_start.set(self.now());
            self.tick();
        }

        fn fade_ripple(&self) {
            self.ripple_fade_from.set(self.ripple_opacity.get());
            self.ripple_fade_start.set(self.now());
            self.tick();
        }

        fn ripple_size(&self, now: i64) -> f64 {
            if self.ripple_start.get() < 0 {
                return 0.0;
            }
            let part = ((now - self.ripple_start.get()) as f64
                / (self.ripple_duration.get() * 1000.0))
                .clamp(0.0, 1.0);
            STANDARD_DECEL.at(part) * self.ripple_reach.get() * 2.0
        }

        fn fading_opacity(&self, now: i64) -> f64 {
            if self.ripple_fade_start.get() < 0 {
                return self.ripple_opacity.get();
            }
            let part = ((now - self.ripple_fade_start.get()) as f64
                / (self.ripple_duration.get() * 2.0 * 1000.0))
                .clamp(0.0, 1.0);
            let opacity = self.ripple_fade_from.get() * (1.0 - STANDARD_DECEL.at(part));
            self.ripple_opacity.set(opacity);
            opacity
        }

        fn snapshot_ripple(&self, snapshot: &gtk4::Snapshot) {
            let now = self.now();
            let size = self.ripple_size(now);
            let opacity = self.fading_opacity(now);
            if size <= 0.0 || opacity <= 0.0 {
                return;
            }
            let colour = self.ripple_colour();
            let solid = RGBA::new(
                colour.red(),
                colour.green(),
                colour.blue(),
                colour.alpha() * opacity as f32,
            );
            let clear = RGBA::new(colour.red(), colour.green(), colour.blue(), 0.0);
            let half = (size / 2.0) as f32;
            let centre =
                graphene::Point::new(self.ripple_x.get() as f32, self.ripple_y.get() as f32);
            let bounds =
                graphene::Rect::new(centre.x() - half, centre.y() - half, half * 2.0, half * 2.0);
            snapshot.append_radial_gradient(
                &bounds,
                &centre,
                half * 2.0,
                half * 2.0,
                0.0,
                1.0,
                &[
                    gsk::ColorStop::new(0.0, solid),
                    gsk::ColorStop::new(0.3, solid),
                    gsk::ColorStop::new(0.5, clear),
                    gsk::ColorStop::new(1.0, clear),
                ],
            );
        }

        fn animating(&self, now: i64) -> bool {
            let colour = self.fade_from.get().is_some();
            let growing = self.ripple_start.get() >= 0
                && ((now - self.ripple_start.get()) as f64) < self.ripple_duration.get() * 1000.0;
            let fading = self.ripple_fade_start.get() >= 0 && self.ripple_opacity.get() > 0.0;
            colour || growing || fading
        }

        fn tick(&self) {
            if self.ticking.replace(true) {
                return;
            }
            self.obj().add_tick_callback(|button, clock| {
                button.queue_draw();
                let imp = button.imp();
                if imp.animating(clock.frame_time()) {
                    return glib::ControlFlow::Continue;
                }
                imp.ticking.set(false);
                glib::ControlFlow::Break
            });
        }
    }
}

fn presses_nested_control(button: &RippleButton, x: f64, y: f64) -> bool {
    let outer: &gtk4::Widget = button.upcast_ref();
    let mut current = button.pick(x, y, gtk4::PickFlags::DEFAULT);
    while let Some(widget) = current {
        if &widget == outer {
            return false;
        }
        if widget.is::<gtk4::Button>() || widget.is_focusable() {
            return true;
        }
        current = widget.parent();
    }
    false
}

glib::wrapper! {
    pub struct RippleButton(ObjectSubclass<imp::RippleButton>)
        @extends gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl RippleButton {
    pub fn new(theme: &SharedTheme) -> Self {
        let button: RippleButton = glib::Object::new();
        button.imp().theme.replace(Some(theme.clone()));
        button
    }

    pub fn set_look(&self, look: Look) {
        self.imp().look.set(look);
        self.imp().retarget();
    }

    pub fn set_palette(&self, palette: Palette) {
        if self.imp().palette.replace(Some(palette)) != Some(palette) {
            self.imp().retarget();
        }
    }

    pub fn set_toggled(&self, toggled: bool) {
        if self.imp().toggled.replace(toggled) != toggled {
            self.imp().retarget();
        }
    }

    pub fn toggled(&self) -> bool {
        self.imp().toggled.get()
    }

    pub fn set_radius(&self, radius: f64) {
        self.imp().radius.set(radius);
        if let Some(motion) = self.imp().radius_motion.borrow().as_ref() {
            motion.to(radius);
        }
        self.queue_draw();
    }

    pub fn animate_radius(&self, millis: f64, ease: Ease) {
        let motion = Motion::new(self, self.imp().radius.get(), millis, ease);
        self.imp().radius_motion.replace(Some(motion));
    }

    pub fn set_radius_pressed(&self, radius: f64) {
        self.imp().radius_pressed.set(Some(radius));
    }

    pub fn set_ripple_enabled(&self, enabled: bool) {
        self.imp().ripple_enabled.set(enabled);
    }

    pub fn set_click_phase(&self, phase: gtk4::PropagationPhase) {
        let ours = self.imp().click.borrow().clone();
        let controllers = self.observe_controllers();
        for index in 0..controllers.n_items() {
            let Some(gesture) = controllers.item(index).and_downcast::<gtk4::GestureClick>() else {
                continue;
            };
            if Some(&gesture) != ours.as_ref() {
                gesture.set_propagation_phase(phase);
            }
        }
    }

    pub fn set_background_size(&self, width: f32, height: f32) {
        self.imp().background_size.set(Some((width, height)));
        self.queue_draw();
    }

    pub fn set_background_inset(&self, inset: f32) {
        self.set_background_insets(inset, inset);
    }

    pub fn set_background_insets(&self, horizontal: f32, vertical: f32) {
        self.imp().background_inset.set((horizontal, vertical));
        self.queue_draw();
    }

    pub fn set_ripple_duration(&self, millis: f64) {
        self.imp().ripple_duration.set(millis);
    }

    pub fn set_natural_width_trim(&self, trim: i32) {
        self.set_layout_manager(Some(TrimmedBin::new(trim)));
    }

    pub fn set_content(&self, child: &impl IsA<gtk4::Widget>, horizontal: i32, vertical: i32) {
        child.set_margin_start(horizontal);
        child.set_margin_end(horizontal);
        child.set_margin_top(vertical);
        child.set_margin_bottom(vertical);
        self.set_child(Some(child));
        crate::ui::widgets::group::pointer_cursor(child.as_ref());
    }

    pub fn connect_alt(&self, action: impl Fn(&gtk4::GestureClick) + 'static) {
        self.imp().alt.replace(Some(Box::new(action)));
    }

    pub fn connect_middle(&self, action: impl Fn(&gtk4::GestureClick) + 'static) {
        self.imp().middle.replace(Some(Box::new(action)));
    }

    pub fn connect_down(&self, action: impl Fn() + 'static) {
        self.imp().down_action.replace(Some(Box::new(action)));
    }

    pub fn connect_release(&self, action: impl Fn() + 'static) {
        self.imp().release_action.replace(Some(Box::new(action)));
    }

    pub fn hovered(&self) -> bool {
        self.imp().hovered.get()
    }

    pub fn down(&self) -> bool {
        self.imp().down.get()
    }
}

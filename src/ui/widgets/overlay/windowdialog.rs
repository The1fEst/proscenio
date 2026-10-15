use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::anim::{EMPHASIZED_DECEL, EXPRESSIVE_EFFECTS, Ease, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;

pub const PADDING: f64 = rounding::LARGE as f64;
const WIDTH: f64 = 350.0;
const TRAVEL: f64 = 60.0;
const MILLIS: f64 = 200.0;
const SPACING: f64 = 16.0;
const SCRIM_RADIUS: f32 = 19.0;
const EMPHASIZED_ACCEL: Ease = Ease::Bezier(0.3, 0.0, 0.8, 0.15);

#[derive(Clone, Copy, Default)]
pub struct Place {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
    pub fills_width: bool,
    pub fills_height: bool,
}

impl Place {
    pub fn wide() -> Self {
        Place {
            fills_width: true,
            ..Place::default()
        }
    }

    pub fn bleed(top: f64, bottom: f64) -> Self {
        Place {
            top,
            bottom,
            left: -PADDING,
            right: -PADDING,
            fills_width: true,
            fills_height: false,
        }
    }
}

fn compute_width(place: &Place, width: i32, measure: impl FnOnce() -> (f64, f64)) -> i32 {
    if width < 0 {
        return -1;
    }
    let inner = width as f64 - place.left - place.right;
    let (minimum, natural) = measure();
    let child_width = if place.fills_width {
        inner
    } else {
        natural.min(inner)
    };
    child_width.max(minimum).round() as i32
}

fn compute_height(place: &Place, (minimum, natural): (f64, f64)) -> f64 {
    let size = if place.fills_height { minimum } else { natural };
    (size + place.top + place.bottom).max(0.0)
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DialogColumn {
        pub children: RefCell<Vec<(gtk4::Widget, Place)>>,
        pub spacing: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DialogColumn {
        const NAME: &'static str = "ProscenioDialogColumn";
        type Type = super::DialogColumn;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for DialogColumn {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for DialogColumn {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let visible_children = self.visible_children();
            let size = match orientation {
                gtk4::Orientation::Horizontal => visible_children
                    .iter()
                    .map(|(child, place)| {
                        (child.measure(orientation, -1).1 as f64 + place.left + place.right)
                            .max(0.0)
                    })
                    .fold(0.0, f64::max),
                _ => {
                    visible_children
                        .iter()
                        .map(|(child, place)| self.hint(child, place, for_size))
                        .sum::<f64>()
                        + self.spacing.get() * (visible_children.len() as f64 - 1.0).max(0.0)
                }
            }
            .max(0.0)
            .round() as i32;
            (0, size, -1, -1)
        }

        fn request_mode(&self) -> gtk4::SizeRequestMode {
            gtk4::SizeRequestMode::HeightForWidth
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            crate::ui::widgets::layout::row::present_popovers(&*self.obj());
            let visible_children = self.visible_children();
            let spacing = self.spacing.get();
            let total_hint: f64 = visible_children
                .iter()
                .map(|(child, place)| self.hint(child, place, width))
                .sum();
            let fill_count = visible_children
                .iter()
                .filter(|(_, place)| place.fills_height)
                .count();
            let spare = (height as f64
                - total_hint
                - spacing * (visible_children.len() as f64 - 1.0).max(0.0))
            .max(0.0);
            let extra_height = if fill_count > 0 {
                spare / fill_count as f64
            } else {
                0.0
            };

            let mut top = 0.0;
            for (child, place) in &visible_children {
                let child_width = self.child_width(child, place, width);
                let cell = if place.fills_height {
                    self.hint(child, place, width) + extra_height
                } else {
                    self.hint(child, place, width)
                };
                let inner = cell - place.top - place.bottom;
                let child_height = if place.fills_height {
                    inner
                } else {
                    (child.measure(gtk4::Orientation::Vertical, child_width).1 as f64).min(inner)
                };
                let y = (top + place.top + (inner - child_height) / 2.0).round();
                child.allocate(
                    child_width,
                    child_height.max(0.0).round() as i32,
                    -1,
                    Some(
                        gsk::Transform::new()
                            .translate(&graphene::Point::new(place.left as f32, y as f32)),
                    ),
                );
                top += cell + spacing;
            }
        }
    }

    impl DialogColumn {
        fn hint(&self, child: &gtk4::Widget, place: &Place, width: i32) -> f64 {
            let for_width = self.child_width(child, place, width);
            let (minimum, natural, _, _) = child.measure(gtk4::Orientation::Vertical, for_width);
            compute_height(place, (minimum as f64, natural as f64))
        }

        fn child_width(&self, child: &gtk4::Widget, place: &Place, width: i32) -> i32 {
            compute_width(place, width, || {
                let (minimum, natural, _, _) = child.measure(gtk4::Orientation::Horizontal, -1);
                (minimum as f64, natural as f64)
            })
        }

        fn visible_children(&self) -> Vec<(gtk4::Widget, Place)> {
            self.children
                .borrow()
                .iter()
                .filter(|(child, _)| child.get_visible())
                .cloned()
                .collect()
        }
    }

    pub struct Dialog {
        pub theme: RefCell<Option<SharedTheme>>,
        pub fixed_height: Cell<Option<f64>>,
        pub width: Cell<f64>,
        pub scrim_radius: Cell<f32>,
        pub is_shown: Cell<bool>,
        pub height: Cell<Tween>,
        pub offset: Cell<Tween>,
        pub scrim: Cell<Tween>,
        pub content: RefCell<Option<gtk4::Widget>>,
        pub is_ticking: Cell<bool>,
        pub on_hide_finished: RefCell<Option<Box<dyn FnOnce()>>>,
    }

    impl Default for Dialog {
        fn default() -> Self {
            Dialog {
                theme: RefCell::new(None),
                fixed_height: Cell::new(None),
                width: Cell::new(WIDTH),
                scrim_radius: Cell::new(SCRIM_RADIUS),
                is_shown: Cell::new(false),
                height: Cell::new(Tween::new(0.0, MILLIS, EMPHASIZED_DECEL)),
                offset: Cell::new(Tween::new(-TRAVEL, MILLIS, EMPHASIZED_DECEL)),
                scrim: Cell::new(Tween::new(0.0, MILLIS, EXPRESSIVE_EFFECTS)),
                content: RefCell::new(None),
                is_ticking: Cell::new(false),
                on_hide_finished: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Dialog {
        const NAME: &'static str = "ProscenioWindowDialog";
        type Type = super::Dialog;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Dialog {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Dialog {
        fn measure(&self, _orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let Some(content) = self.content.borrow().clone() else {
                return;
            };
            self.follow_content();
            let (x, y, _, background) = self.geometry(width, height);
            let inner = (background - PADDING * 2.0).max(0.0);
            content.allocate(
                (self.width.get() - PADDING * 2.0) as i32,
                inner.round() as i32,
                -1,
                Some(gsk::Transform::new().translate(&graphene::Point::new(
                    (x + PADDING) as f32,
                    (y + PADDING) as f32,
                ))),
            );
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let Some(shared) = self.theme.borrow().clone() else {
                return;
            };
            let theme = shared.borrow();
            let dialog = self.obj();
            let (width, height) = (dialog.width(), dialog.height());
            let now = self.now();

            let full = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            let scrim = transparentize(
                theme.colors.col_scrim,
                1.0 - self.scrim.get().value(now) as f32,
            );
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(full, self.scrim_radius.get()));
            snapshot.append_color(&scrim, &full);
            snapshot.pop();

            let (x, y, _, background) = self.geometry(width, height);
            if background <= 0.0 {
                return;
            }
            let bounds = graphene::Rect::new(
                x as f32,
                y as f32,
                self.width.get() as f32,
                background as f32,
            );
            let radius = (rounding::LARGE as f32).min(background as f32 / 2.0);
            let shape = gsk::RoundedRect::from_rect(bounds, radius);
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&theme.m3.surface_container_high, &bounds);
            drop(theme);
            self.parent_snapshot(snapshot);
            snapshot.pop();
        }
    }

    impl Dialog {
        pub fn now(&self) -> i64 {
            self.obj()
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time)
        }

        pub fn target(&self) -> f64 {
            self.fixed_height.get().unwrap_or_else(|| {
                let inner = (self.width.get() - PADDING * 2.0) as i32;
                self.content
                    .borrow()
                    .as_ref()
                    .map(|content| content.measure(gtk4::Orientation::Vertical, inner).1 as f64)
                    .unwrap_or(0.0)
                    + PADDING * 2.0
            })
        }

        fn follow_content(&self) {
            if !self.is_shown.get() {
                return;
            }
            let target = self.target();
            let now = self.now();
            let mut height = self.height.get();
            if (height.target() - target).abs() < 0.5 {
                return;
            }
            if height.is_running(now) {
                height.retarget(target, now);
            } else {
                height.jump(target);
            }
            self.height.set(height);
        }

        pub fn geometry(&self, width: i32, height: i32) -> (f64, f64, f64, f64) {
            let now = self.now();
            let round_half = |size: f64| (size / 2.0 + 0.5).floor();
            let background_width = self.width.get();
            let x = round_half(width as f64) - round_half(background_width);
            let target_y = height as f64 / 2.0 - self.target() / 2.0;
            let y = target_y + self.offset.get().value(now);
            (x, y, background_width, self.height.get().value(now))
        }
    }
}

glib::wrapper! {
    pub struct DialogColumn(ObjectSubclass<imp::DialogColumn>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl DialogColumn {
    pub fn new(spacing: f64) -> Self {
        let column: DialogColumn = glib::Object::new();
        column.imp().spacing.set(spacing);
        column
    }

    pub fn add(&self, child: &impl IsA<gtk4::Widget>, place: Place) {
        child.set_parent(self);
        self.imp()
            .children
            .borrow_mut()
            .push((child.clone().upcast(), place));
        self.queue_resize();
    }
}

glib::wrapper! {
    pub struct Dialog(ObjectSubclass<imp::Dialog>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Default)]
struct ClosedCallbacks(RefCell<Vec<Box<dyn Fn()>>>);

impl ClosedCallbacks {
    fn add(&self, callback: impl Fn() + 'static) {
        self.0.borrow_mut().push(Box::new(callback));
    }

    fn run(&self) {
        for callback in self.0.take() {
            callback();
        }
    }
}

pub struct WindowDialog {
    pub root: Dialog,
    pub column: DialogColumn,
    on_dismissed: RefCell<Option<Rc<dyn Fn()>>>,
    on_closing: ClosedCallbacks,
    owned_values: RefCell<Vec<Rc<dyn std::any::Any>>>,
}

impl WindowDialog {
    pub fn new(theme: &SharedTheme, background_height: Option<f64>) -> Rc<Self> {
        let root: Dialog = glib::Object::new();
        let column = DialogColumn::new(SPACING);
        column.set_opacity(0.0);
        column.set_parent(&root);
        let imp = root.imp();
        imp.theme.replace(Some(theme.clone()));
        imp.fixed_height.set(background_height);
        imp.content.replace(Some(column.clone().upcast()));
        root.set_focusable(true);

        let dialog = Rc::new(WindowDialog {
            root: root.clone(),
            column,
            on_dismissed: RefCell::new(None),
            on_closing: ClosedCallbacks::default(),
            owned_values: RefCell::new(Vec::new()),
        });

        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.connect_pressed({
            let dialog = Rc::downgrade(&dialog);
            move |gesture, _, x, y| {
                let Some(dialog) = dialog.upgrade() else {
                    return;
                };
                let root = &dialog.root;
                let (left, top, width, height) = root.imp().geometry(root.width(), root.height());
                let is_inside = x >= left && x < left + width && y >= top && y < top + height;
                if is_inside {
                    return;
                }
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                dialog.dismiss();
            }
        });
        root.add_controller(click);

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed({
            let dialog = Rc::downgrade(&dialog);
            move |_, key, _, _| {
                if key != gtk4::gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
                glib::Propagation::Stop
            }
        });
        root.add_controller(key_controller);

        dialog
    }

    pub fn set_background_width(&self, width: f64) {
        self.root.imp().width.set(width);
        self.root.queue_resize();
    }

    pub fn set_scrim_radius(&self, radius: f32) {
        self.root.imp().scrim_radius.set(radius);
        self.root.queue_draw();
    }

    pub fn connect_dismiss(&self, action: impl Fn() + 'static) {
        self.on_dismissed.replace(Some(Rc::new(action)));
    }

    pub fn connect_closed(&self, action: impl Fn() + 'static) {
        self.on_closing.add(action);
    }

    pub fn keep(&self, value: Rc<dyn std::any::Any>) {
        self.owned_values.borrow_mut().push(value);
    }

    pub fn dismiss(&self) {
        let action = self.on_dismissed.borrow().clone();
        if let Some(action) = action {
            action();
        }
    }

    pub fn remove_from(&self, overlay: &gtk4::Overlay) {
        self.on_closing.run();
        overlay.remove_overlay(&self.root);
    }

    pub fn show(&self, is_shown: bool, on_hide_finished: impl FnOnce() + 'static) {
        let imp = self.root.imp();
        imp.is_shown.set(is_shown);
        let now = imp.now();
        let ease = if is_shown {
            EMPHASIZED_DECEL
        } else {
            EMPHASIZED_ACCEL
        };
        let target = imp.target();
        let mut height = imp.height.get();
        height.set_timing(MILLIS, ease);
        height.retarget(if is_shown { target } else { 0.0 }, now);
        imp.height.set(height);
        let mut offset = imp.offset.get();
        offset.set_timing(MILLIS, ease);
        offset.retarget(if is_shown { 0.0 } else { -TRAVEL }, now);
        imp.offset.set(offset);
        let mut scrim = imp.scrim.get();
        scrim.retarget(if is_shown { 1.0 } else { 0.0 }, now);
        imp.scrim.set(scrim);
        if !is_shown {
            imp.on_hide_finished
                .replace(Some(Box::new(on_hide_finished)));
            self.on_closing.run();
        }
        if is_shown {
            self.root.grab_focus();
        }
        let column = self.column.clone();
        let opacity = Cell::new(Tween::new(column.opacity(), MILLIS, EXPRESSIVE_EFFECTS));
        let mut next = opacity.get();
        next.retarget(if is_shown { 1.0 } else { 0.0 }, now);
        opacity.set(next);
        self.root.add_tick_callback(move |root, clock| {
            let imp = root.imp();
            let now = clock.frame_time();
            column.set_opacity(opacity.get().value(now));
            root.queue_allocate();
            root.queue_draw();
            let is_busy = imp.height.get().is_running(now)
                || imp.offset.get().is_running(now)
                || imp.scrim.get().is_running(now)
                || opacity.get().is_running(now);
            if is_busy {
                return glib::ControlFlow::Continue;
            }
            if !imp.is_shown.get()
                && let Some(on_hide_finished) = imp.on_hide_finished.take()
            {
                on_hide_finished();
            }
            glib::ControlFlow::Break
        });
    }
}

pub fn title(text: &str) -> gtk4::Label {
    let label = text::styled(text);
    text::set_font(&label, Family::Title, pixel_size::TITLE as f64, "wght=550");
    text::set_color(&label, "colOnSurface");
    label.set_wrap(true);
    label.set_xalign(0.0);
    label
}

pub fn section_header(text: &str) -> gtk4::Label {
    let label = text::styled(text);
    text::set_font(&label, Family::Title, pixel_size::LARGE as f64, "wght=550");
    label.set_xalign(0.0);
    label
}

const PAUSE_MILLIS: f64 = 520.0;
const SLIDE_MILLIS: f64 = 1240.0;

pub fn progress(theme: &SharedTheme) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_content_height(4);
    area.set_draw_func({
        let theme = theme.clone();
        move |area, cr, width, height| {
            let color = theme.borrow().colors.col_primary;
            let paint = |alpha: f64, x: f64, span: f64| {
                cr.set_source_rgba(
                    color.red() as f64,
                    color.green() as f64,
                    color.blue() as f64,
                    color.alpha() as f64 * alpha,
                );
                cr.rectangle(x, 0.0, span, height as f64);
                let _ = cr.fill();
            };
            let width = width as f64;
            paint(0.25, 0.0, width);
            let time = area
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time) as f64
                / 1000.0
                % (SLIDE_MILLIS + PAUSE_MILLIS);
            for part in [
                (time / SLIDE_MILLIS).min(1.0),
                ((time - PAUSE_MILLIS) / SLIDE_MILLIS).max(0.0),
            ] {
                let value = Ease::OutCubic.at(part);
                let x = value * width;
                paint(1.0, x, value * (width - x));
            }
        }
    });
    area.add_tick_callback(|area, _| {
        area.queue_draw();
        glib::ControlFlow::Continue
    });
    area
}

pub fn separator() -> gtk4::Widget {
    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    line.add_css_class("dialog-separator");
    line.set_size_request(-1, 1);
    line.upcast()
}

pub fn separator_place() -> Place {
    Place::bleed(-8.0, -8.0)
}

pub fn button_row() -> (gtk4::Box, Place) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    (
        row,
        Place {
            top: 0.0,
            bottom: -8.0,
            left: -8.0,
            right: -8.0,
            fills_width: true,
            fills_height: false,
        },
    )
}

pub fn spacer() -> gtk4::Widget {
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    spacer.upcast()
}

pub fn button(theme: &SharedTheme, text: &str) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_radius(rounding::FULL as f64);
    button.set_look(Look {
        background: |theme| transparentize(theme.colors.col_layer3, 1.0),
        hover: |theme| theme.colors.col_layer3_hover,
        ripple: |theme| theme.colors.col_layer3_active,
        ..Look::default()
    });
    let label = text::styled(text);
    text::set_color(&label, "colPrimary");
    label.add_css_class("color-fade");
    button.set_content(&Centered::new(&label), 14, 0);
    button.set_size_request(-1, 36);
    button.set_valign(gtk4::Align::Center);
    button
}

pub fn list_item(theme: &SharedTheme, is_active: bool) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_radius(0.0);
    button.set_overflow(gtk4::Overflow::Hidden);
    set_list_item_active(&button, is_active);
    button
}

pub fn set_list_item_active(button: &RippleButton, is_active: bool) {
    button.set_look(Look {
        background: |theme| transparentize(theme.colors.col_layer3, 1.0),
        hover: if is_active {
            |theme| transparentize(theme.colors.col_layer3, 1.0)
        } else {
            |theme| theme.colors.col_layer3_hover
        },
        ripple: |theme| theme.colors.col_layer3_active,
        ..Look::default()
    });
    button.set_cursor_from_name(if is_active { None } else { Some("pointer") });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_callbacks_run_once_however_often_the_dialog_closes() {
        let count = Rc::new(Cell::new(0));
        let callbacks = ClosedCallbacks::default();
        callbacks.add({
            let count = count.clone();
            move || count.set(count.get() + 1)
        });
        callbacks.run();
        callbacks.run();
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn rows_are_measured_at_the_width_they_are_given_but_never_below_their_minimum() {
        let place = Place {
            left: 10.0,
            right: 20.0,
            ..Place::default()
        };
        let wide_place = Place {
            fills_width: true,
            ..place
        };
        assert_eq!(compute_width(&place, 300, || (0.0, 500.0)), 270);
        assert_eq!(compute_width(&place, 300, || (0.0, 100.0)), 100);
        assert_eq!(compute_width(&wide_place, 300, || (0.0, 100.0)), 270);
        assert_eq!(compute_width(&place, -1, || (0.0, 100.0)), -1);
        assert_eq!(compute_width(&place, 300, || (290.0, 500.0)), 290);
        assert_eq!(compute_width(&wide_place, 300, || (290.0, 500.0)), 290);
        assert_eq!(compute_width(&place, 20, || (0.0, 100.0)), 0);
    }

    #[test]
    fn a_filling_row_keeps_at_least_its_minimum_height_without_spare_room() {
        let place = Place {
            top: 10.0,
            bottom: 20.0,
            ..Place::default()
        };
        assert_eq!(compute_height(&place, (40.0, 150.0)), 180.0);
        for (top, bottom) in [(10.0, 20.0), (0.0, 0.0), (-8.0, -8.0)] {
            let tall_place = Place {
                top,
                bottom,
                fills_height: true,
                ..Place::default()
            };
            for minimum in [0.0, 10.0, 40.0] {
                let inner = compute_height(&tall_place, (minimum, 150.0)) - top - bottom;
                assert!(inner >= minimum, "{inner} for {minimum} at {top}, {bottom}");
            }
        }
    }
}

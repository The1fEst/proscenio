use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::anim::{EMPHASIZED_DECEL, EXPRESSIVE_EFFECTS, Ease, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text::{self, Family};

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
    pub fill_width: bool,
    pub fill_height: bool,
}

impl Place {
    pub fn wide() -> Self {
        Place {
            fill_width: true,
            ..Place::default()
        }
    }

    pub fn bleed(top: f64, bottom: f64) -> Self {
        Place {
            top,
            bottom,
            left: -PADDING,
            right: -PADDING,
            fill_width: true,
            fill_height: false,
        }
    }
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
            let visible = self.visible();
            let size = match orientation {
                gtk4::Orientation::Horizontal => visible
                    .iter()
                    .map(|(child, place)| {
                        (child.measure(orientation, -1).1 as f64 + place.left + place.right)
                            .max(0.0)
                    })
                    .fold(0.0, f64::max),
                _ => {
                    visible
                        .iter()
                        .map(|(child, place)| self.hint(child, place, for_size))
                        .sum::<f64>()
                        + self.spacing.get() * (visible.len() as f64 - 1.0).max(0.0)
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
            crate::ui::widgets::row::present_popovers(&*self.obj());
            let visible = self.visible();
            let spacing = self.spacing.get();
            let fixed: f64 = visible
                .iter()
                .map(|(child, place)| self.hint(child, place, width))
                .sum();
            let fills = visible
                .iter()
                .filter(|(_, place)| place.fill_height)
                .count();
            let spare =
                (height as f64 - fixed - spacing * (visible.len() as f64 - 1.0).max(0.0)).max(0.0);
            let share = if fills > 0 { spare / fills as f64 } else { 0.0 };

            let mut top = 0.0;
            for (child, place) in &visible {
                let inner_width = width as f64 - place.left - place.right;
                let child_width = if place.fill_width {
                    inner_width
                } else {
                    (child.measure(gtk4::Orientation::Horizontal, -1).1 as f64).min(inner_width)
                };
                let cell = if place.fill_height {
                    self.hint(child, place, width) + share
                } else {
                    self.hint(child, place, width)
                };
                let inner = cell - place.top - place.bottom;
                let child_height = if place.fill_height {
                    inner
                } else {
                    (child
                        .measure(gtk4::Orientation::Vertical, child_width as i32)
                        .1 as f64)
                        .min(inner)
                };
                let y = (top + place.top + (inner - child_height) / 2.0).round();
                child.allocate(
                    child_width.max(0.0).round() as i32,
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
            let for_width = if width >= 0 && place.fill_width {
                (width as f64 - place.left - place.right).max(0.0) as i32
            } else {
                -1
            };
            let natural = if place.fill_height {
                0.0
            } else {
                child.measure(gtk4::Orientation::Vertical, for_width).1 as f64
            };
            (natural + place.top + place.bottom).max(0.0)
        }

        fn visible(&self) -> Vec<(gtk4::Widget, Place)> {
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
        pub fixed: Cell<Option<f64>>,
        pub width: Cell<f64>,
        pub scrim_radius: Cell<f32>,
        pub shown: Cell<bool>,
        pub height: Cell<Tween>,
        pub offset: Cell<Tween>,
        pub scrim: Cell<Tween>,
        pub content: RefCell<Option<gtk4::Widget>>,
        pub ticking: Cell<bool>,
        pub closed: RefCell<Option<Box<dyn FnOnce()>>>,
    }

    impl Default for Dialog {
        fn default() -> Self {
            Dialog {
                theme: RefCell::new(None),
                fixed: Cell::new(None),
                width: Cell::new(WIDTH),
                scrim_radius: Cell::new(SCRIM_RADIUS),
                shown: Cell::new(false),
                height: Cell::new(Tween::new(0.0, MILLIS, EMPHASIZED_DECEL)),
                offset: Cell::new(Tween::new(-TRAVEL, MILLIS, EMPHASIZED_DECEL)),
                scrim: Cell::new(Tween::new(0.0, MILLIS, EXPRESSIVE_EFFECTS)),
                content: RefCell::new(None),
                ticking: Cell::new(false),
                closed: RefCell::new(None),
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
            self.fixed.get().unwrap_or_else(|| {
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
            if !self.shown.get() {
                return;
            }
            let target = self.target();
            let now = self.now();
            let mut height = self.height.get();
            if (height.target() - target).abs() < 0.5 {
                return;
            }
            if height.running(now) {
                height.retarget(target, now);
            } else {
                height.jump(target);
            }
            self.height.set(height);
        }

        pub fn geometry(&self, width: i32, height: i32) -> (f64, f64, f64, f64) {
            let now = self.now();
            let half = |size: f64| (size / 2.0 + 0.5).floor();
            let background_width = self.width.get();
            let x = half(width as f64) - half(background_width);
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

pub struct WindowDialog {
    pub root: Dialog,
    pub column: DialogColumn,
    dismissed: RefCell<Option<Rc<dyn Fn()>>>,
    hidden: RefCell<Vec<Box<dyn Fn()>>>,
    kept: RefCell<Vec<Rc<dyn std::any::Any>>>,
}

impl WindowDialog {
    pub fn new(theme: &SharedTheme, background_height: Option<f64>) -> Rc<Self> {
        let root: Dialog = glib::Object::new();
        let column = DialogColumn::new(SPACING);
        column.set_opacity(0.0);
        column.set_parent(&root);
        let imp = root.imp();
        imp.theme.replace(Some(theme.clone()));
        imp.fixed.set(background_height);
        imp.content.replace(Some(column.clone().upcast()));
        root.set_focusable(true);

        let dialog = Rc::new(WindowDialog {
            root: root.clone(),
            column,
            dismissed: RefCell::new(None),
            hidden: RefCell::new(Vec::new()),
            kept: RefCell::new(Vec::new()),
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
                let inside = x >= left && x < left + width && y >= top && y < top + height;
                if inside {
                    return;
                }
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                dialog.dismiss();
            }
        });
        root.add_controller(click);

        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed({
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
        root.add_controller(keys);

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
        self.dismissed.replace(Some(Rc::new(action)));
    }

    pub fn connect_closed(&self, action: impl Fn() + 'static) {
        self.hidden.borrow_mut().push(Box::new(action));
    }

    pub fn keep(&self, value: Rc<dyn std::any::Any>) {
        self.kept.borrow_mut().push(value);
    }

    pub fn dismiss(&self) {
        let action = self.dismissed.borrow().clone();
        if let Some(action) = action {
            action();
        }
    }

    pub fn show(&self, shown: bool, closed: impl FnOnce() + 'static) {
        let imp = self.root.imp();
        imp.shown.set(shown);
        let now = imp.now();
        let ease = if shown {
            EMPHASIZED_DECEL
        } else {
            EMPHASIZED_ACCEL
        };
        let target = imp.target();
        let mut height = imp.height.get();
        height.set_timing(MILLIS, ease);
        height.retarget(if shown { target } else { 0.0 }, now);
        imp.height.set(height);
        let mut offset = imp.offset.get();
        offset.set_timing(MILLIS, ease);
        offset.retarget(if shown { 0.0 } else { -TRAVEL }, now);
        imp.offset.set(offset);
        let mut scrim = imp.scrim.get();
        scrim.retarget(if shown { 1.0 } else { 0.0 }, now);
        imp.scrim.set(scrim);
        if !shown {
            imp.closed.replace(Some(Box::new(closed)));
            for action in self.hidden.borrow().iter() {
                action();
            }
        }
        if shown {
            self.root.grab_focus();
        }
        let column = self.column.clone();
        let opacity = Cell::new(Tween::new(column.opacity(), MILLIS, EXPRESSIVE_EFFECTS));
        let mut next = opacity.get();
        next.retarget(if shown { 1.0 } else { 0.0 }, now);
        opacity.set(next);
        self.root.add_tick_callback(move |root, clock| {
            let imp = root.imp();
            let now = clock.frame_time();
            column.set_opacity(opacity.get().value(now));
            root.queue_allocate();
            root.queue_draw();
            let busy = imp.height.get().running(now)
                || imp.offset.get().running(now)
                || imp.scrim.get().running(now)
                || opacity.get().running(now);
            if busy {
                return glib::ControlFlow::Continue;
            }
            if !imp.shown.get()
                && let Some(closed) = imp.closed.take()
            {
                closed();
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
            let colour = theme.borrow().colors.col_primary;
            let paint = |alpha: f64, x: f64, span: f64| {
                cr.set_source_rgba(
                    colour.red() as f64,
                    colour.green() as f64,
                    colour.blue() as f64,
                    colour.alpha() as f64 * alpha,
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
            fill_width: true,
            fill_height: false,
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
    button.set_content(&Centred::new(&label), 14, 0);
    button.set_size_request(-1, 36);
    button.set_valign(gtk4::Align::Center);
    button
}

pub fn list_item(theme: &SharedTheme, active: bool) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_radius(0.0);
    button.set_overflow(gtk4::Overflow::Hidden);
    set_list_item_active(&button, active);
    button
}

pub fn set_list_item_active(button: &RippleButton, active: bool) {
    button.set_look(Look {
        background: |theme| transparentize(theme.colors.col_layer3, 1.0),
        hover: if active {
            |theme| transparentize(theme.colors.col_layer3, 1.0)
        } else {
            |theme| theme.colors.col_layer3_hover
        },
        ripple: |theme| theme.colors.col_layer3_active,
        ..Look::default()
    });
    button.set_cursor_from_name(if active { None } else { Some("pointer") });
}

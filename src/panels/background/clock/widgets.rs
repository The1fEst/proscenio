use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};

use super::{FADE_MILLIS, RESIZE_MILLIS, steer};
use crate::ui::anim::{EMPHASIZED, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text::{self, Family};

const STATUS_PADDING: i32 = 5;
const STATUS_TEXT_SPACING: i32 = 4;
const DIGITAL_SPACING: i32 = 4;
const CLOCK_TEXT_SIZE: f64 = 20.0;

pub(super) fn clock_text() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.add_css_class("raised");
    text::set_font(&label, Family::Expressive, CLOCK_TEXT_SIZE, "wght=350");
    label
}

pub(super) fn paint_label(label: &gtk4::Label, color: RGBA) {
    let attributes = label.attributes().unwrap_or_default();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
    attributes.change(pango::AttrColor::new_foreground(
        channel(color.red()),
        channel(color.green()),
        channel(color.blue()),
    ));
    attributes.change(pango::AttrInt::new_foreground_alpha(
        channel(color.alpha()).max(1),
    ));
    label.set_attributes(Some(&attributes));
}

pub(super) fn status_row(icon: &str, label: &str) -> (gtk4::Box, [gtk4::Label; 2]) {
    let symbol = text::symbol(icon, pixel_size::HUGE as f64);
    symbol.add_css_class("raised");
    symbol.set_valign(gtk4::Align::Center);
    let words = clock_text();
    text::set_font(
        &words,
        Family::Expressive,
        pixel_size::LARGE as f64,
        "wght=400",
    );
    words.set_text(label);
    words.set_valign(gtk4::Align::Center);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, STATUS_TEXT_SPACING);
    row.append(&symbol);
    row.append(&words);
    row.set_visible(false);
    (row, [symbol, words])
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Digital {
        pub offsets: RefCell<Vec<i32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Digital {
        const NAME: &'static str = "ProscenioDigitalClock";
        type Type = super::Digital;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Digital {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Digital {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let rows = self.rows();
            let size = match orientation {
                gtk4::Orientation::Horizontal => rows
                    .iter()
                    .map(|(child, _)| child.measure(orientation, -1).1)
                    .max()
                    .unwrap_or(0),
                _ => rows
                    .iter()
                    .enumerate()
                    .map(|(index, (child, offset))| {
                        let gap = if index == 0 {
                            0
                        } else {
                            DIGITAL_SPACING + offset
                        };
                        child.measure(orientation, -1).1 + gap
                    })
                    .sum::<i32>()
                    .max(0),
            };
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let mut y = 0;
            for (index, (child, offset)) in self.rows().into_iter().enumerate() {
                if index > 0 {
                    y += DIGITAL_SPACING + offset;
                }
                let height = child.measure(gtk4::Orientation::Vertical, width).1;
                let place = gtk4::gsk::Transform::new()
                    .translate(&gtk4::graphene::Point::new(0.0, y as f32));
                child.allocate(width, height, -1, Some(place));
                y += height;
            }
        }
    }

    impl Digital {
        fn rows(&self) -> Vec<(gtk4::Widget, i32)> {
            let offsets = self.offsets.borrow();
            let mut rows = Vec::new();
            let mut child = self.obj().first_child();
            let mut index = 0;
            while let Some(widget) = child {
                if widget.is_visible() {
                    rows.push((widget.clone(), offsets.get(index).copied().unwrap_or(0)));
                }
                index += 1;
                child = widget.next_sibling();
            }
            rows
        }
    }

    pub struct Badge {
        pub width: Cell<Tween>,
        pub height: Cell<Tween>,
        pub opacity: Cell<Tween>,
        pub is_sized: Cell<bool>,
    }

    impl Default for Badge {
        fn default() -> Self {
            Badge {
                width: Cell::new(Tween::new(0.0, RESIZE_MILLIS, EMPHASIZED)),
                height: Cell::new(Tween::new(0.0, RESIZE_MILLIS, EMPHASIZED)),
                opacity: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
                is_sized: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Badge {
        const NAME: &'static str = "ProscenioClockBadge";
        type Type = super::Badge;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Badge {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.add_css_class("clock-status");
            obj.set_overflow(gtk4::Overflow::Hidden);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Badge {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let obj = self.obj();
            let now = obj
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time);
            let Some(child) = obj.first_child() else {
                return (0, 0, -1, -1);
            };
            let natural = child.measure(orientation, -1).1 + STATUS_PADDING * 2;
            let tween = match orientation {
                gtk4::Orientation::Horizontal => &self.width,
                _ => &self.height,
            };
            let mut value = tween.get();
            if self.is_sized.get() && obj.is_mapped() {
                value.retarget(natural as f64, now);
            } else {
                value.jump(natural as f64);
            }
            tween.set(value);
            let size = value.value(now).round() as i32;
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            self.is_sized.set(true);
            let Some(child) = self.obj().first_child() else {
                return;
            };
            let natural_width = child.measure(gtk4::Orientation::Horizontal, -1).1;
            let natural_height = child.measure(gtk4::Orientation::Vertical, -1).1;
            let place = gtk4::gsk::Transform::new().translate(&gtk4::graphene::Point::new(
                ((width - natural_width) / 2) as f32,
                ((height - natural_height) / 2) as f32,
            ));
            child.allocate(natural_width, natural_height, -1, Some(place));
        }
    }

    #[derive(Default)]
    pub struct Scaled {
        pub scale: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Scaled {
        const NAME: &'static str = "ProscenioScaled";
        type Type = super::Scaled;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Scaled {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Scaled {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().first_child() {
                Some(child) => child.measure(orientation, for_size),
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let Some(child) = self.obj().first_child() else {
                return;
            };
            let scale = self.scale.get() as f32;
            let (half_width, half_height) = (width as f32 / 2.0, height as f32 / 2.0);
            let place = gtk4::gsk::Transform::new()
                .translate(&gtk4::graphene::Point::new(half_width, half_height))
                .scale(scale, scale)
                .translate(&gtk4::graphene::Point::new(-half_width, -half_height));
            child.allocate(width, height, baseline, Some(place));
        }
    }
}

glib::wrapper! {
    pub struct Digital(ObjectSubclass<imp::Digital>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Digital {
    pub(super) fn new(rows: &[(gtk4::Widget, i32)]) -> Self {
        let digital: Self = glib::Object::new();
        for (row, _) in rows {
            row.set_parent(&digital);
        }
        digital
            .imp()
            .offsets
            .replace(rows.iter().map(|(_, offset)| *offset).collect());
        digital
    }
}

glib::wrapper! {
    pub struct Badge(ObjectSubclass<imp::Badge>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Badge {
    pub(super) fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let badge: Self = glib::Object::new();
        child.set_parent(&badge);
        badge
    }

    pub(super) fn set_shown(&self, is_shown: bool, now: i64, should_animate: bool) {
        let imp = self.imp();
        let mut opacity = imp.opacity.get();
        steer(
            &mut opacity,
            if is_shown { 1.0 } else { 0.0 },
            now,
            should_animate,
        );
        imp.opacity.set(opacity);
        self.set_opacity(opacity.value(now));
    }

    pub(super) fn is_running(&self, now: i64) -> bool {
        let imp = self.imp();
        imp.width.get().is_running(now)
            || imp.height.get().is_running(now)
            || imp.opacity.get().is_running(now)
    }

    pub(super) fn step(&self, now: i64) {
        let imp = self.imp();
        if imp.width.get().is_running(now) || imp.height.get().is_running(now) {
            self.queue_resize();
        }
        self.set_opacity(imp.opacity.get().value(now));
    }
}

glib::wrapper! {
    pub struct Scaled(ObjectSubclass<imp::Scaled>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Scaled {
    pub(in crate::panels::background) fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let scaled: Self = glib::Object::new();
        scaled.imp().scale.set(1.0);
        child.set_parent(&scaled);
        scaled
    }

    pub(in crate::panels::background) fn set_scale(&self, scale: f64) {
        if (self.imp().scale.replace(scale) - scale).abs() > f64::EPSILON {
            self.queue_allocate();
        }
    }
}

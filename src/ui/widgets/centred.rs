use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Centred {
        pub rotation: std::cell::Cell<f32>,
        pub fill_width: std::cell::Cell<bool>,
        pub integral: std::cell::Cell<bool>,
        pub optical: std::cell::Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Centred {
        const NAME: &'static str = "ProscenioCentred";
        type Type = super::Centred;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Centred {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Centred {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let qt = child
                .downcast_ref::<gtk4::Label>()
                .filter(|_| orientation == gtk4::Orientation::Vertical)
                .and_then(qt_metrics);
            if let Some((height, _)) = qt {
                let height = height as i32;
                return (height, height, -1, -1);
            }
            let (minimum, natural, _, _) = child.measure(orientation, for_size);
            let overshoot = match orientation {
                gtk4::Orientation::Horizontal => {
                    child.downcast_ref::<gtk4::Label>().map_or(0, ink_overshoot)
                }
                _ => 0,
            };
            (minimum, natural + overshoot, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            crate::ui::widgets::row::present_popovers(&*self.obj());
            let Some(child) = self.obj().first_child() else {
                return;
            };
            let child_width = if self.fill_width.get() {
                width
            } else {
                child.measure(gtk4::Orientation::Horizontal, -1).1
            };
            let child_height = child.measure(gtk4::Orientation::Vertical, child_width).1;
            let offset = |outer: i32, inner: i32| {
                if self.integral.get() {
                    half(outer) - half(inner)
                } else {
                    ((outer - inner) as f32 / 2.0 + 0.5).floor()
                }
            };
            let label = child.downcast_ref::<gtk4::Label>();
            let room = match label {
                Some(label) if !self.fill_width.get() => width - ink_overshoot(label),
                _ => width,
            };
            let x = match label {
                Some(label) if self.integral.get() => {
                    let (_, logical) = label.layout().extents();
                    let exact = logical.width() as f32 / gtk4::pango::SCALE as f32;
                    (half(room) - half_exact(exact)).round()
                }
                _ => offset(room, child_width),
            };
            let y = match label {
                Some(label) if self.optical.get() => {
                    let figure = label.create_pango_layout(Some("0"));
                    figure.set_attributes(label.attributes().as_ref());
                    let (ink, _) = figure.extents();
                    let scale = gtk4::pango::SCALE as f32;
                    let middle = (ink.y() as f32 + ink.height() as f32 / 2.0) / scale;
                    (height as f32 / 2.0 - middle).round()
                }
                Some(label) if !self.integral.get() => {
                    let baseline = label.layout().baseline() as f32 / gtk4::pango::SCALE as f32;
                    let (qt_height, qt_baseline) =
                        qt_metrics(label).unwrap_or((child_height as f32, baseline));
                    ((height as f32 - qt_height) / 2.0 + qt_baseline).round() - baseline.round()
                }
                Some(label) => match qt_metrics(label) {
                    Some((qt_height, _)) => half(height) - half(qt_height as i32),
                    None => offset(height, child_height),
                },
                None => offset(height, child_height),
            };
            let (half_width, half_height) = (child_width as f32 / 2.0, child_height as f32 / 2.0);
            let place = gtk4::gsk::Transform::new()
                .translate(&gtk4::graphene::Point::new(x + half_width, y + half_height))
                .rotate(self.rotation.get())
                .translate(&gtk4::graphene::Point::new(-half_width, -half_height));
            child.allocate(child_width, child_height, -1, Some(place));
        }
    }
}

fn ink_right_and_logical_right(label: &gtk4::Label) -> Option<(f64, f64)> {
    if label.ellipsize() != gtk4::pango::EllipsizeMode::None || label.wraps() {
        return None;
    }
    let layout = label.layout();
    if layout.line_count() != 1 {
        return None;
    }
    let (ink, logical) = layout.extents();
    let scale = gtk4::pango::SCALE as f64;
    Some((
        (ink.x() + ink.width()) as f64 / scale,
        (logical.x() + logical.width()) as f64 / scale,
    ))
}

fn ink_overshoot(label: &gtk4::Label) -> i32 {
    ink_right_and_logical_right(label).map_or(0, |(ink, logical)| {
        (ink.ceil() - logical.ceil()).max(0.0) as i32
    })
}

fn half(size: i32) -> f32 {
    (size as f32 / 2.0 + 0.5).floor()
}

fn half_exact(size: f32) -> f32 {
    if size as i32 % 2 == 1 {
        (size + 1.0) / 2.0
    } else {
        size / 2.0
    }
}

fn qt_metrics(label: &gtk4::Label) -> Option<(f32, f32)> {
    layout_qt_metrics(&label.layout())
}

pub fn layout_qt_metrics(layout: &gtk4::pango::Layout) -> Option<(f32, f32)> {
    if layout.line_count() != 1 {
        return None;
    }
    let run = layout.iter().run_readonly()?;
    let metrics = run.item().analysis().font().metrics(None);
    let truncate = |units: i32| (units as f32 / gtk4::pango::SCALE as f32 * 64.0).floor() / 64.0;
    let (ascent, descent) = (truncate(metrics.ascent()), truncate(metrics.descent()));
    Some(((ascent + descent).ceil(), ascent))
}

glib::wrapper! {
    pub struct Centred(ObjectSubclass<imp::Centred>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Centred {
    pub fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let centred: Centred = glib::Object::new();
        child.set_parent(&centred);
        centred
    }

    pub fn filling_width(child: &impl IsA<gtk4::Widget>) -> Self {
        let centred = Self::new(child);
        centred.imp().fill_width.set(true);
        centred
    }

    pub fn integral(child: &impl IsA<gtk4::Widget>) -> Self {
        let centred = Self::new(child);
        centred.imp().integral.set(true);
        centred
    }

    pub fn optical(label: &gtk4::Label) -> Self {
        let centred = Self::new(label);
        centred.imp().optical.set(true);
        centred
    }

    pub fn exact_width(&self) -> Option<f64> {
        let label = self.first_child()?.downcast::<gtk4::Label>().ok()?;
        let (ink, logical) = ink_right_and_logical_right(&label)?;
        let margins =
            self.margin_start() + self.margin_end() + label.margin_start() + label.margin_end();
        Some(ink.max(logical) + margins as f64)
    }

    pub fn set_rotation(&self, degrees: f32) {
        self.imp().rotation.set(degrees);
        self.queue_allocate();
    }

    pub fn rotate_to(&self, degrees: f32) {
        let now = self
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let current = self.imp().rotation.get() as f64;
        let mut tween =
            crate::ui::anim::Tween::new(current, 200.0, crate::ui::anim::EXPRESSIVE_EFFECTS);
        tween.retarget(degrees as f64, now);
        if !self.is_mapped() {
            self.set_rotation(degrees);
            return;
        }
        self.add_tick_callback(move |centred, clock| {
            let now = clock.frame_time();
            centred.set_rotation(tween.value(now) as f32);
            if tween.running(now) {
                return glib::ControlFlow::Continue;
            }
            glib::ControlFlow::Break
        });
    }
}

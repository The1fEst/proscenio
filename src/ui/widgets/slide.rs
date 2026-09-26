use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_FAST, Tween};

const MOVE_MILLIS: f64 = 500.0;
const DISMISS_THRESHOLD: f64 = 70.0;

mod imp {
    use super::*;

    pub struct Slide {
        pub offset: Cell<Tween>,
        pub scale: Cell<Tween>,
        pub height: Cell<Tween>,
        pub ticking: Cell<bool>,
        pub finished: RefCell<Option<Box<dyn FnOnce()>>>,
    }

    impl Default for Slide {
        fn default() -> Self {
            Slide {
                offset: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_FAST)),
                scale: Cell::new(Tween::new(1.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                height: Cell::new(Tween::new(1.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                ticking: Cell::new(false),
                finished: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Slide {
        const NAME: &'static str = "ProscenioSlide";
        type Type = super::Slide;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Slide {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Slide {
        fn request_mode(&self) -> gtk4::SizeRequestMode {
            self.obj()
                .first_child()
                .map_or(gtk4::SizeRequestMode::ConstantSize, |child| {
                    child.request_mode()
                })
        }

        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let (minimum, natural, _, _) = child.measure(orientation, for_size);
            if orientation == gtk4::Orientation::Horizontal {
                return (minimum, natural, -1, -1);
            }
            let factor = self.height.get().value(self.now()).clamp(0.0, 1.0);
            let size = (natural as f64 * factor).round() as i32;
            (size.min(minimum), size, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let Some(child) = self.obj().first_child() else {
                return;
            };
            let natural = child.measure(gtk4::Orientation::Vertical, width).1;
            let now = self.now();
            let offset = self.offset.get().value(now) as f32;
            let scale = self.scale.get().value(now) as f32;
            let (half_width, half_height) = (width as f32 / 2.0, natural as f32 / 2.0);
            let place = gtk4::gsk::Transform::new()
                .translate(&graphene::Point::new(offset + half_width, half_height))
                .scale(scale, scale)
                .translate(&graphene::Point::new(-half_width, -half_height));
            child.allocate(width, natural, -1, Some(place));
        }
    }

    impl Slide {
        pub fn now(&self) -> i64 {
            self.obj()
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time)
        }
    }
}

glib::wrapper! {
    pub struct Slide(ObjectSubclass<imp::Slide>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Slide {
    pub fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let slide: Slide = glib::Object::new();
        child.set_parent(&slide);
        slide
    }

    pub fn offset(&self) -> f64 {
        self.imp().offset.get().value(self.imp().now())
    }

    pub fn set_offset(&self, offset: f64, animate: bool) {
        let imp = self.imp();
        let mut tween = imp.offset.get();
        tween.set_timing(MOVE_MILLIS, EXPRESSIVE_FAST);
        if animate && self.is_mapped() {
            tween.retarget(offset, imp.now());
        } else {
            tween.jump(offset);
        }
        imp.offset.set(tween);
        self.run();
    }

    pub fn slide_out(&self, left: bool, overshoot: f64, finished: impl FnOnce() + 'static) {
        let imp = self.imp();
        let target = (self.width() as f64 + overshoot) * if left { -1.0 } else { 1.0 };
        let mut tween = imp.offset.get();
        tween.set_timing(MOVE_MILLIS, EXPRESSIVE_DEFAULT);
        let from = tween.value(imp.now());
        tween.jump(from);
        tween.retarget(target, imp.now());
        imp.offset.set(tween);
        imp.finished.replace(Some(Box::new(finished)));
        self.run();
    }

    pub fn pop_in(&self) {
        let imp = self.imp();
        let now = imp.now();
        let mut scale = imp.scale.get();
        scale.jump(0.0);
        scale.retarget(1.0, now);
        imp.scale.set(scale);
        self.set_opacity(0.0);
        self.run();
    }

    pub fn collapse(&self, finished: impl FnOnce() + 'static) {
        let imp = self.imp();
        let now = imp.now();
        let mut height = imp.height.get();
        height.retarget(0.0, now);
        imp.height.set(height);
        let mut offset = imp.offset.get();
        if offset.value(now).abs() < 1.0 {
            offset.set_timing(MOVE_MILLIS, EXPRESSIVE_DEFAULT);
            offset.retarget(self.width() as f64 + 20.0, now);
            imp.offset.set(offset);
        }
        imp.finished.replace(Some(Box::new(finished)));
        self.run();
    }

    fn run(&self) {
        self.queue_allocate();
        let imp = self.imp();
        if imp.ticking.replace(true) {
            return;
        }
        let collapsing = imp.height.get().target() < 1.0;
        self.add_tick_callback(move |slide, clock| {
            let imp = slide.imp();
            let now = clock.frame_time();
            let (offset, scale, height) = (imp.offset.get(), imp.scale.get(), imp.height.get());
            if scale.running(now) {
                slide.set_opacity(scale.value(now).clamp(0.0, 1.0));
            } else if slide.opacity() < 1.0 && !collapsing {
                slide.set_opacity(1.0);
            }
            if height.target() < 1.0 {
                slide.set_opacity((1.0 - height_progress(&height, now)).clamp(0.0, 1.0));
                slide.queue_resize();
            }
            slide.queue_allocate();
            if offset.running(now) || scale.running(now) || height.running(now) {
                return glib::ControlFlow::Continue;
            }
            imp.ticking.set(false);
            if let Some(finished) = imp.finished.take() {
                finished();
            }
            glib::ControlFlow::Break
        });
    }
}

fn height_progress(height: &Tween, now: i64) -> f64 {
    1.0 - height.value(now)
}

pub struct DragList {
    entries: RefCell<Vec<Slide>>,
    index: Cell<i32>,
    distance: Cell<f64>,
    dragging: Cell<bool>,
}

impl DragList {
    pub fn new() -> Rc<Self> {
        Rc::new(DragList {
            entries: RefCell::new(Vec::new()),
            index: Cell::new(-1),
            distance: Cell::new(0.0),
            dragging: Cell::new(false),
        })
    }

    pub fn clear(&self) {
        self.entries.borrow_mut().clear();
        self.index.set(-1);
        self.distance.set(0.0);
    }

    pub fn push(&self, slide: &Slide) -> i32 {
        let mut entries = self.entries.borrow_mut();
        entries.push(slide.clone());
        entries.len() as i32 - 1
    }

    fn update(&self) {
        let index = self.index.get();
        let distance = self.distance.get();
        for (position, slide) in self.entries.borrow().iter().enumerate() {
            let difference = (index - position as i32).abs();
            let offset = if index < 0 {
                0.0
            } else if difference == 0 {
                distance
            } else if distance.abs() > DISMISS_THRESHOLD {
                0.0
            } else if difference == 1 {
                distance * 0.3
            } else if difference == 2 {
                distance * 0.1
            } else {
                0.0
            };
            let own = difference == 0 && self.dragging.get();
            slide.set_offset(offset, !own);
        }
    }

    pub fn reset(&self) {
        self.distance.set(0.0);
        self.update();
        self.index.set(-1);
    }

    fn position(&self, slide: &Slide) -> i32 {
        self.entries
            .borrow()
            .iter()
            .position(|entry| entry == slide)
            .map_or(-1, |index| index as i32)
    }

    pub fn attach(
        self: &Rc<Self>,
        target: &impl IsA<gtk4::Widget>,
        slide: &Slide,
        interactive: Rc<dyn Fn() -> bool>,
        dismiss: Rc<dyn Fn(bool)>,
    ) {
        let drag = gtk4::GestureDrag::new();
        drag.set_button(gtk4::gdk::BUTTON_PRIMARY);
        drag.connect_drag_begin({
            let interactive = interactive.clone();
            move |gesture, _, _| {
                if !interactive() {
                    gesture.set_state(gtk4::EventSequenceState::Denied);
                }
            }
        });
        drag.connect_drag_update({
            let list = Rc::downgrade(self);
            let slide = slide.downgrade();
            move |gesture, dx, dy| {
                let (Some(list), Some(slide)) = (list.upgrade(), slide.upgrade()) else {
                    return;
                };
                if !list.dragging.get() {
                    let threshold = gesture
                        .widget()
                        .map(|widget| widget.settings().gtk_dnd_drag_threshold())
                        .unwrap_or(8) as f64;
                    if dx.hypot(dy) < threshold {
                        return;
                    }
                    list.dragging.set(true);
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    list.index.set(list.position(&slide));
                }
                list.distance.set(dx);
                list.update();
            }
        });
        drag.connect_drag_end({
            let list = Rc::downgrade(self);
            move |_, dx, _| {
                let Some(list) = list.upgrade() else {
                    return;
                };
                if !list.dragging.replace(false) {
                    return;
                }
                if dx.abs() > DISMISS_THRESHOLD {
                    let index = list.index.get();
                    for (position, other) in list.entries.borrow().iter().enumerate() {
                        if position as i32 != index {
                            other.set_offset(0.0, true);
                        }
                    }
                    list.index.set(-1);
                    list.distance.set(0.0);
                    dismiss(dx < 0.0);
                } else {
                    list.reset();
                }
            }
        });
        target.add_controller(drag);
    }
}

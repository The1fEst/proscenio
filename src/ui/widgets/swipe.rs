use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use crate::ui::anim::{Ease, Tween};

const MOVE_MILLIS: f64 = 250.0;
const IN_OUT_QUAD: Ease = Ease::Bezier(0.455, 0.03, 0.515, 0.955);
const DRAG_DISTANCE: f64 = 10.0;
const SNAP_ONE_VELOCITY: f64 = 30.0;
const VELOCITY_WINDOW: i64 = 100_000;

type Changed = Box<dyn Fn(usize)>;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    pub struct Swipe {
        pub spacing: Cell<i32>,
        pub position: Cell<Tween>,
        pub ticking: Cell<bool>,
        pub start: Cell<f64>,
        pub dragging: Cell<bool>,
        pub samples: RefCell<Vec<(i64, f64)>>,
        pub changed: RefCell<Option<Changed>>,
    }

    impl Default for Swipe {
        fn default() -> Self {
            Swipe {
                spacing: Cell::new(0),
                position: Cell::new(Tween::new(0.0, MOVE_MILLIS, IN_OUT_QUAD)),
                ticking: Cell::new(false),
                start: Cell::new(0.0),
                dragging: Cell::new(false),
                samples: RefCell::new(Vec::new()),
                changed: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Swipe {
        const NAME: &'static str = "ProscenioSwipe";
        type Type = super::Swipe;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Swipe {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_overflow(gtk4::Overflow::Hidden);

            let drag = gtk4::GestureDrag::new();
            drag.set_propagation_phase(gtk4::PropagationPhase::Capture);
            drag.connect_drag_begin(|gesture, _, _| {
                let Some(swipe) = gesture.widget().and_downcast::<super::Swipe>() else {
                    return;
                };
                let imp = swipe.imp();
                imp.start.set(imp.position.get().value(swipe.now()));
                imp.dragging.set(false);
                imp.samples.borrow_mut().clear();
            });
            drag.connect_drag_update(|gesture, dx, dy| {
                let Some(swipe) = gesture.widget().and_downcast::<super::Swipe>() else {
                    return;
                };
                swipe.follow(gesture, dx, dy);
            });
            drag.connect_drag_end(|gesture, dx, _| {
                let Some(swipe) = gesture.widget().and_downcast::<super::Swipe>() else {
                    return;
                };
                swipe.settle(dx);
            });
            obj.add_controller(drag);
        }

        fn dispose(&self) {
            self.changed.take();
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Swipe {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let mut sizes = (0, 0);
            let mut child = self.obj().first_child();
            while let Some(page) = child {
                let (minimum, natural, _, _) = page.measure(orientation, for_size);
                sizes = (sizes.0.max(minimum), sizes.1.max(natural));
                child = page.next_sibling();
            }
            (sizes.0, sizes.1, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let position = self.position.get().value(obj.now());
            let stride = (width + self.spacing.get()) as f64;
            let mut index = 0.0;
            let mut child = obj.first_child();
            while let Some(page) = child {
                let x = ((index - position) * stride).round() as f32;
                let place =
                    gtk4::gsk::Transform::new().translate(&gtk4::graphene::Point::new(x, 0.0));
                page.allocate(width, height, -1, Some(place));
                index += 1.0;
                child = page.next_sibling();
            }
        }
    }
}

glib::wrapper! {
    pub struct Swipe(ObjectSubclass<imp::Swipe>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Swipe {
    pub fn new(spacing: i32) -> Self {
        let swipe: Self = glib::Object::new();
        swipe.imp().spacing.set(spacing);
        swipe
    }

    pub fn append(&self, page: &impl IsA<gtk4::Widget>) {
        page.set_parent(self);
    }

    pub fn connect_changed(&self, action: impl Fn(usize) + 'static) {
        self.imp().changed.replace(Some(Box::new(action)));
    }

    pub fn show(&self, index: usize, animate: bool) {
        let imp = self.imp();
        let mut position = imp.position.get();
        if animate && self.is_mapped() {
            position.retarget(index as f64, self.now());
        } else {
            position.jump(index as f64);
        }
        imp.position.set(position);
        self.queue_allocate();
        if imp.ticking.replace(true) {
            return;
        }
        self.add_tick_callback(|swipe, clock| {
            swipe.queue_allocate();
            let imp = swipe.imp();
            if imp.position.get().running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            imp.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn now(&self) -> i64 {
        self.frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn last(&self) -> f64 {
        let mut count = 0;
        let mut child = self.first_child();
        while let Some(page) = child {
            count += 1;
            child = page.next_sibling();
        }
        (count - 1).max(0) as f64
    }

    fn stride(&self) -> f64 {
        (self.width() + self.imp().spacing.get()).max(1) as f64
    }

    fn follow(&self, gesture: &gtk4::GestureDrag, dx: f64, dy: f64) {
        let imp = self.imp();
        if !imp.dragging.get() {
            if dx.abs() < DRAG_DISTANCE && dy.abs() < DRAG_DISTANCE {
                return;
            }
            if dy.abs() > dx.abs() {
                gesture.set_state(gtk4::EventSequenceState::Denied);
                return;
            }
            imp.dragging.set(true);
            gesture.set_state(gtk4::EventSequenceState::Claimed);
        }
        let now = glib::monotonic_time();
        {
            let mut samples = imp.samples.borrow_mut();
            samples.push((now, dx));
            samples.retain(|(time, _)| now - time <= VELOCITY_WINDOW);
        }
        let position = (imp.start.get() - dx / self.stride()).clamp(0.0, self.last());
        let mut tween = imp.position.get();
        tween.jump(position);
        imp.position.set(tween);
        self.queue_allocate();
    }

    fn settle(&self, dx: f64) {
        let imp = self.imp();
        if !imp.dragging.replace(false) {
            return;
        }
        let velocity = {
            let samples = imp.samples.borrow();
            match (samples.first(), samples.last()) {
                (Some(first), Some(last)) if last.0 > first.0 => {
                    (last.1 - first.1) / ((last.0 - first.0) as f64 / 1_000_000.0)
                }
                _ => 0.0,
            }
        };
        let position = (imp.start.get() - dx / self.stride()).clamp(0.0, self.last());
        let target = if velocity < -SNAP_ONE_VELOCITY {
            position.floor() + 1.0
        } else if velocity > SNAP_ONE_VELOCITY {
            position.ceil() - 1.0
        } else {
            position.round()
        }
        .clamp(0.0, self.last()) as usize;
        self.show(target, true);
        if let Some(changed) = imp.changed.borrow().as_ref() {
            changed(target);
        }
    }
}

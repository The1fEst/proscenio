use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use crate::ui::anim::{EXPRESSIVE_DEFAULT, Tween};

const MOVE_MILLIS: f64 = 500.0;
const REMOVE_OVERSHOOT: f64 = 20.0;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    pub struct Row {
        pub widget: gtk4::Widget,
        pub y: Cell<Tween>,
        pub pop: Cell<Tween>,
        pub leave: Cell<Tween>,
        pub placed: Cell<bool>,
    }

    #[derive(Default)]
    pub struct Laps {
        pub spacing: Cell<i32>,
        pub rows: RefCell<Vec<Row>>,
        pub leaving: RefCell<Vec<Row>>,
        pub ticking: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Laps {
        const NAME: &'static str = "ProscenioLaps";
        type Type = super::Laps;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Laps {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Laps {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let rows = self.rows.borrow();
            let size = match orientation {
                gtk4::Orientation::Horizontal => rows
                    .iter()
                    .map(|row| row.widget.measure(orientation, -1).1)
                    .max()
                    .unwrap_or(0),
                _ => {
                    let heights: i32 = rows
                        .iter()
                        .map(|row| row.widget.measure(orientation, for_size).1)
                        .sum();
                    heights + self.spacing.get() * (rows.len() as i32 - 1).max(0)
                }
            };
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let now = self.now();
            let animate = self.obj().is_mapped();
            let mut target = 0.0;
            for row in self.rows.borrow().iter() {
                let height = row.widget.measure(gtk4::Orientation::Vertical, width).1;
                let mut y = row.y.get();
                if animate && row.placed.replace(true) {
                    y.retarget(target, now);
                } else {
                    y.jump(target);
                }
                row.y.set(y);
                place(row, width, height, 0.0, now);
                target += (height + self.spacing.get()) as f64;
            }
            for row in self.leaving.borrow().iter() {
                let height = row.widget.measure(gtk4::Orientation::Vertical, width).1;
                let shift = (width as f64 + REMOVE_OVERSHOOT) * row.leave.get().value(now);
                place(row, width, height, shift, now);
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let now = self.now();
            let obj = self.obj();
            for row in self.rows.borrow().iter() {
                let opacity = row.pop.get().value(now).clamp(0.0, 1.0);
                paint(&obj, snapshot, &row.widget, opacity);
            }
            for row in self.leaving.borrow().iter() {
                let opacity = (1.0 - row.leave.get().value(now)).clamp(0.0, 1.0);
                paint(&obj, snapshot, &row.widget, opacity);
            }
        }
    }

    impl Laps {
        pub fn now(&self) -> i64 {
            self.obj()
                .frame_clock()
                .map(|clock| clock.frame_time())
                .unwrap_or_else(glib::monotonic_time)
        }

        pub fn running(&self, now: i64) -> bool {
            let moving = |row: &Row| {
                row.y.get().running(now)
                    || row.pop.get().running(now)
                    || row.leave.get().running(now)
            };
            self.rows.borrow().iter().any(moving) || self.leaving.borrow().iter().any(moving)
        }
    }

    fn place(row: &Row, width: i32, height: i32, shift: f64, now: i64) {
        let scale = row.pop.get().value(now) as f32;
        let (half_width, half_height) = (width as f32 / 2.0, height as f32 / 2.0);
        let y = row.y.get().value(now) as f32;
        let place = gtk4::gsk::Transform::new()
            .translate(&graphene::Point::new(
                shift as f32 + half_width,
                y + half_height,
            ))
            .scale(scale, scale)
            .translate(&graphene::Point::new(-half_width, -half_height));
        row.widget.allocate(width, height, -1, Some(place));
    }

    fn paint(parent: &super::Laps, snapshot: &gtk4::Snapshot, child: &gtk4::Widget, opacity: f64) {
        if opacity <= 0.0 {
            return;
        }
        if opacity >= 1.0 {
            parent.snapshot_child(child, snapshot);
            return;
        }
        snapshot.push_opacity(opacity);
        parent.snapshot_child(child, snapshot);
        snapshot.pop();
    }
}

glib::wrapper! {
    pub struct Laps(ObjectSubclass<imp::Laps>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Laps {
    pub fn new(spacing: i32) -> Self {
        let laps: Self = glib::Object::new();
        laps.imp().spacing.set(spacing);
        laps
    }

    pub fn prepend(&self, row: &impl IsA<gtk4::Widget>) {
        let imp = self.imp();
        let mut pop = Tween::new(1.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT);
        if self.is_mapped() {
            pop.jump(0.0);
            pop.retarget(1.0, imp.now());
        }
        row.set_parent(self);
        imp.rows.borrow_mut().insert(
            0,
            imp::Row {
                widget: row.clone().upcast(),
                y: std::cell::Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                pop: std::cell::Cell::new(pop),
                leave: std::cell::Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                placed: std::cell::Cell::new(false),
            },
        );
        self.run();
    }

    pub fn clear(&self) {
        let imp = self.imp();
        let rows = imp.rows.take();
        if !self.is_mapped() {
            for row in rows {
                row.widget.unparent();
            }
            self.queue_resize();
            return;
        }
        let now = imp.now();
        for row in &rows {
            let mut leave = row.leave.get();
            leave.retarget(1.0, now);
            row.leave.set(leave);
        }
        imp.leaving.borrow_mut().extend(rows);
        self.run();
    }

    fn run(&self) {
        self.queue_resize();
        let imp = self.imp();
        if imp.ticking.replace(true) {
            return;
        }
        self.add_tick_callback(|laps, clock| {
            let imp = laps.imp();
            laps.queue_allocate();
            laps.queue_draw();
            if imp.running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            for row in imp.leaving.take() {
                row.widget.unparent();
            }
            imp.ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

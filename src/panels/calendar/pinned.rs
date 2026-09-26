use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct Pinned {
        pub size: Cell<(i32, i32)>,
        pub places: RefCell<Vec<(gtk4::Widget, f32, f32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Pinned {
        const NAME: &'static str = "ProscenioPinned";
        type Type = super::Pinned;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Pinned {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Pinned {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (width, height) = self.size.get();
            let size = match orientation {
                gtk4::Orientation::Horizontal => width,
                _ => height,
            };
            (size, size, -1, -1)
        }

        fn size_allocate(&self, _width: i32, _height: i32, _baseline: i32) {
            for (child, x, y) in self.places.borrow().iter() {
                let width = child.measure(gtk4::Orientation::Horizontal, -1).1;
                let height = child.measure(gtk4::Orientation::Vertical, width).1;
                let place =
                    gtk4::gsk::Transform::new().translate(&gtk4::graphene::Point::new(*x, *y));
                child.allocate(width, height, -1, Some(place));
            }
        }
    }
}

glib::wrapper! {
    pub struct Pinned(ObjectSubclass<imp::Pinned>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Pinned {
    pub fn new(width: i32, height: i32) -> Self {
        let pinned: Self = glib::Object::new();
        pinned.imp().size.set((width, height));
        pinned
    }

    pub fn put(&self, child: &impl IsA<gtk4::Widget>, x: f64, y: f64) {
        child.set_parent(self);
        self.imp()
            .places
            .borrow_mut()
            .push((child.clone().upcast(), x as f32, y as f32));
        self.queue_allocate();
    }
}

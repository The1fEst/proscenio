use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FixedWidth {
        pub width: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FixedWidth {
        const NAME: &'static str = "ProscenioFixedWidth";
        type Type = super::FixedWidth;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for FixedWidth {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_overflow(gtk4::Overflow::Hidden);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for FixedWidth {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let width = self.width.get();
            if orientation == gtk4::Orientation::Horizontal {
                return (width, width, -1, -1);
            }
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let (minimum, natural, _, _) = child.measure(orientation, width);
            (minimum, natural, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                child.allocate(width, height, -1, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct FixedWidth(ObjectSubclass<imp::FixedWidth>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl FixedWidth {
    pub fn new(width: i32) -> Self {
        let fixed: FixedWidth = glib::Object::new();
        fixed.imp().width.set(width);
        fixed
    }

    pub fn width(&self) -> i32 {
        self.imp().width.get()
    }

    pub fn set_width(&self, width: i32) {
        if self.imp().width.replace(width) != width {
            self.queue_resize();
        }
    }

    pub fn set_child(&self, child: &impl IsA<gtk4::Widget>) {
        while let Some(previous) = self.first_child() {
            previous.unparent();
        }
        child.set_parent(self);
    }
}

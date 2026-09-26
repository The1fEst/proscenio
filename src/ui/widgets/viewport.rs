use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Viewport {
        pub size: Cell<(i32, i32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Viewport {
        const NAME: &'static str = "ProscenioViewport";
        type Type = super::Viewport;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Viewport {
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

    impl WidgetImpl for Viewport {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (width, height) = self.size.get();
            let size = match orientation {
                gtk4::Orientation::Horizontal => width,
                _ => height,
            };
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let mut next = self.obj().first_child();
            while let Some(child) = next {
                next = child.next_sibling();
                if !child.get_visible() {
                    continue;
                }
                let child_width = child
                    .measure(gtk4::Orientation::Horizontal, -1)
                    .1
                    .max(width);
                let child_height = child.measure(gtk4::Orientation::Vertical, child_width).1;
                child.allocate(child_width, child_height, -1, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct Viewport(ObjectSubclass<imp::Viewport>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Viewport {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn show_child(&self, shown: &impl IsA<gtk4::Widget>) {
        if shown.as_ref().parent().as_ref() != Some(self.upcast_ref()) {
            shown.set_parent(self);
        }
        let mut next = self.first_child();
        while let Some(child) = next {
            next = child.next_sibling();
            child.set_visible(&child == shown.as_ref());
        }
    }

    pub fn set_size(&self, width: i32, height: i32) {
        self.imp().size.set((width, height));
        self.queue_resize();
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

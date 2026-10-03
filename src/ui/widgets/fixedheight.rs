use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FixedHeight {
        pub height: Cell<i32>,
        pub cap: Cell<Option<i32>>,
        pub following: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FixedHeight {
        const NAME: &'static str = "ProscenioFixedHeight";
        type Type = super::FixedHeight;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for FixedHeight {
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

    impl WidgetImpl for FixedHeight {
        fn request_mode(&self) -> gtk4::SizeRequestMode {
            gtk4::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk4::Orientation::Vertical {
                let height = match self.obj().first_child() {
                    Some(child) if self.following.get() => {
                        let natural = child.measure(orientation, for_size).1;
                        self.cap.get().map_or(natural, |cap| natural.min(cap))
                    }
                    _ => self.height.get(),
                };
                return (height, height, -1, -1);
            }
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let (minimum, natural, _, _) = child.measure(orientation, -1);
            (minimum, natural, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                let natural = child.measure(gtk4::Orientation::Vertical, width).1;
                child.allocate(width, natural, -1, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct FixedHeight(ObjectSubclass<imp::FixedHeight>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl FixedHeight {
    pub fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let fixed: FixedHeight = glib::Object::new();
        child.set_parent(&fixed);
        fixed
    }

    pub fn set_height(&self, height: i32) {
        let following = self.imp().following.replace(false);
        if self.imp().height.replace(height) != height || following {
            self.queue_resize();
        }
    }

    pub fn follow(&self, cap: Option<i32>) {
        self.imp().cap.set(cap);
        self.imp().following.set(true);
        self.queue_resize();
    }
}

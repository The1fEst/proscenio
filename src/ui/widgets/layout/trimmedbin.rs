use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TrimmedBin {
        pub trim: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrimmedBin {
        const NAME: &'static str = "ProscenioTrimmedBin";
        type Type = super::TrimmedBin;
        type ParentType = gtk4::LayoutManager;
    }

    impl ObjectImpl for TrimmedBin {}

    impl LayoutManagerImpl for TrimmedBin {
        fn measure(
            &self,
            widget: &gtk4::Widget,
            orientation: gtk4::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            let (mut minimum, mut natural) = (0, 0);
            let mut child = widget.first_child();
            while let Some(current) = child {
                if current.should_layout() {
                    let (child_minimum, child_natural, _, _) =
                        current.measure(orientation, for_size);
                    minimum = minimum.max(child_minimum);
                    natural = natural.max(child_natural);
                }
                child = current.next_sibling();
            }
            if orientation == gtk4::Orientation::Horizontal {
                natural = (natural - self.trim.get()).max(minimum);
            }
            (minimum, natural, -1, -1)
        }

        fn allocate(&self, widget: &gtk4::Widget, width: i32, height: i32, baseline: i32) {
            let mut child = widget.first_child();
            while let Some(current) = child {
                if current.should_layout() {
                    current.allocate(width, height, baseline, None);
                }
                child = current.next_sibling();
            }
        }
    }
}

glib::wrapper! {
    pub struct TrimmedBin(ObjectSubclass<imp::TrimmedBin>)
        @extends gtk4::LayoutManager;
}

impl TrimmedBin {
    pub fn new(trim: i32) -> Self {
        let layout: TrimmedBin = glib::Object::new();
        layout.imp().trim.set(trim);
        layout
    }
}

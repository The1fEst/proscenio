use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Strip {
        pub start: RefCell<Option<gtk4::Widget>>,
        pub center: RefCell<Option<gtk4::Widget>>,
        pub end: RefCell<Option<gtk4::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Strip {
        const NAME: &'static str = "ProscenioStrip";
        type Type = super::Strip;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Strip {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Strip {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let size = |slot: &RefCell<Option<gtk4::Widget>>| {
                slot.borrow()
                    .as_ref()
                    .map_or(0, |child| child.measure(orientation, -1).1)
            };
            let size = match orientation {
                gtk4::Orientation::Horizontal => size(&self.center),
                _ => size(&self.start)
                    .max(size(&self.center))
                    .max(size(&self.end)),
            };
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let center_width = self.center.borrow().as_ref().map_or(0, |child| {
                child.measure(gtk4::Orientation::Horizontal, -1).1
            });
            let minimum = |slot: &RefCell<Option<gtk4::Widget>>| {
                slot.borrow().as_ref().map_or(0, |child| {
                    child.measure(gtk4::Orientation::Horizontal, height).0
                })
            };
            let centred = ((width - center_width) as f32 / 2.0 + 0.5).floor() as i32;
            let center_x = centred
                .min(width - minimum(&self.end) - center_width)
                .max(minimum(&self.start));
            let place = |child: &gtk4::Widget, x: i32, span: i32| {
                let span = span.max(child.measure(gtk4::Orientation::Horizontal, height).0);
                child.allocate(
                    span,
                    height,
                    -1,
                    Some(
                        gtk4::gsk::Transform::new()
                            .translate(&gtk4::graphene::Point::new(x as f32, 0.0)),
                    ),
                );
            };
            if let Some(center) = self.center.borrow().as_ref() {
                place(center, center_x, center_width);
            }
            if let Some(start) = self.start.borrow().as_ref() {
                place(start, 0, center_x);
            }
            if let Some(end) = self.end.borrow().as_ref() {
                let span = width - center_x - center_width;
                let minimum = end.measure(gtk4::Orientation::Horizontal, height).0;
                place(end, width - span.max(minimum), span);
            }
        }
    }
}

glib::wrapper! {
    pub struct Strip(ObjectSubclass<imp::Strip>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Strip {
    pub fn new(
        start: &impl IsA<gtk4::Widget>,
        center: &impl IsA<gtk4::Widget>,
        end: &impl IsA<gtk4::Widget>,
    ) -> Self {
        let strip: Strip = glib::Object::new();
        for (slot, child) in [
            (&strip.imp().start, start.as_ref()),
            (&strip.imp().center, center.as_ref()),
            (&strip.imp().end, end.as_ref()),
        ] {
            child.set_parent(&strip);
            slot.replace(Some(child.clone()));
        }
        strip
    }
}

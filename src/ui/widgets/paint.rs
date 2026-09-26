use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

type Draw = Box<dyn Fn(&gtk4::Snapshot, f32, f32)>;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct Paint {
        pub draw: RefCell<Option<Draw>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Paint {
        const NAME: &'static str = "ProscenioPaint";
        type Type = super::Paint;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Paint {}

    impl WidgetImpl for Paint {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            if let Some(draw) = self.draw.borrow().as_ref() {
                draw(snapshot, obj.width() as f32, obj.height() as f32);
            }
        }
    }
}

glib::wrapper! {
    pub struct Paint(ObjectSubclass<imp::Paint>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Paint {
    pub fn new(draw: impl Fn(&gtk4::Snapshot, f32, f32) + 'static) -> Self {
        let paint: Self = glib::Object::new();
        paint.imp().draw.replace(Some(Box::new(draw)));
        paint
    }

    pub fn set_draw(&self, draw: impl Fn(&gtk4::Snapshot, f32, f32) + 'static) {
        self.imp().draw.replace(Some(Box::new(draw)));
        self.queue_draw();
    }
}

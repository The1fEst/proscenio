use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Column {
        pub spacing: Cell<i32>,
        pub centred: Cell<bool>,
        pub extra: Cell<i32>,
        pub fill_width: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Column {
        const NAME: &'static str = "ProscenioColumn";
        type Type = super::Column;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Column {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Column {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let children = self.laid_out();
            let naturals = children
                .iter()
                .map(|child| child.measure(orientation, -1).1);
            let size = match orientation {
                gtk4::Orientation::Vertical => (naturals.sum::<i32>()
                    + self.spacing.get() * (children.len() as i32 - 1).max(0))
                .max(0),
                _ => naturals.max().unwrap_or(0),
            } + self.extra.get();
            if self.fill_width.get() && orientation == gtk4::Orientation::Horizontal {
                let minimum = children
                    .iter()
                    .map(|child| child.measure(orientation, -1).0)
                    .max()
                    .unwrap_or(0);
                return (minimum, size, -1, -1);
            }
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            crate::ui::widgets::row::present_popovers(&*self.obj());
            let children = self.laid_out();
            let heights: Vec<i32> = children
                .iter()
                .map(|child| child.measure(gtk4::Orientation::Vertical, -1).1)
                .collect();
            let content = heights.iter().sum::<i32>()
                + self.spacing.get() * (children.len() as i32 - 1).max(0);
            let share = if self.extra.get() > 0 && !children.is_empty() {
                (height - content).max(0) as f32 / children.len() as f32
            } else {
                0.0
            };
            let mut top = 0.0f32;
            for (child, child_height) in children.iter().zip(heights) {
                let child_width = if self.fill_width.get() {
                    width
                } else {
                    child.measure(gtk4::Orientation::Horizontal, -1).1
                };
                let cell = child_height as f32 + share;
                let x = if self.centred.get() {
                    ((width - child_width) as f32 / 2.0 + 0.5).floor()
                } else {
                    0.0
                };
                let y = (top + (cell - child_height as f32) / 2.0 + 0.5).floor();
                let place =
                    gtk4::gsk::Transform::new().translate(&gtk4::graphene::Point::new(x, y));
                child.allocate(child_width, child_height, -1, Some(place));
                top += cell + self.spacing.get() as f32;
            }
        }
    }

    impl Column {
        fn laid_out(&self) -> Vec<gtk4::Widget> {
            let mut children = Vec::new();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                if widget.get_visible() && !widget.is::<gtk4::Popover>() {
                    children.push(widget.clone());
                }
                child = widget.next_sibling();
            }
            children
        }
    }
}

glib::wrapper! {
    pub struct Column(ObjectSubclass<imp::Column>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Column {
    pub fn new(spacing: i32, centred: bool) -> Self {
        let column: Column = glib::Object::new();
        column.imp().spacing.set(spacing);
        column.imp().centred.set(centred);
        column
    }

    pub fn with_extra(spacing: i32, extra: i32) -> Self {
        let column = Self::new(spacing, true);
        column.imp().extra.set(extra);
        column
    }

    pub fn filling_width(spacing: i32) -> Self {
        let column = Self::new(spacing, false);
        column.imp().fill_width.set(true);
        column
    }

    pub fn append(&self, child: &impl IsA<gtk4::Widget>) {
        child.set_parent(self);
    }

    pub fn remove(&self, child: &impl IsA<gtk4::Widget>) {
        child.unparent();
    }

    pub fn set_spacing(&self, spacing: i32) {
        if self.imp().spacing.replace(spacing) != spacing {
            self.queue_resize();
        }
    }
}

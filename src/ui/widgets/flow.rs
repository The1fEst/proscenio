use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};

type LinesChanged = Box<dyn Fn(&[usize])>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Flow {
        pub spacing: Cell<i32>,
        pub lines: RefCell<Vec<usize>>,
        pub lines_changed: RefCell<Option<LinesChanged>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Flow {
        const NAME: &'static str = "ProscenioFlow";
        type Type = super::Flow;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Flow {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Flow {
        fn request_mode(&self) -> gtk4::SizeRequestMode {
            gtk4::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let children = self.visible_children();
            if orientation == gtk4::Orientation::Horizontal {
                let widths = children
                    .iter()
                    .map(|child| child.measure(orientation, -1).1);
                let widest = widths.clone().max().unwrap_or(0);
                let spacing = self.spacing.get() * (children.len() as i32 - 1).max(0);
                return (widest, widths.sum::<i32>() + spacing, -1, -1);
            }
            let width = if for_size < 0 { i32::MAX } else { for_size };
            let height = self
                .place(&children, width)
                .iter()
                .map(|place| place.y + place.height)
                .max()
                .unwrap_or(0);
            (height, height, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let children = self.visible_children();
            let places = self.place(&children, width);
            for (child, place) in children.iter().zip(&places) {
                let point = graphene::Point::new(place.x as f32, place.y as f32);
                let transform = gtk4::gsk::Transform::new().translate(&point);
                child.allocate(place.width, place.height, -1, Some(transform));
            }
            let lines: Vec<usize> = places.iter().map(|place| place.line).collect();
            if *self.lines.borrow() == lines {
                return;
            }
            self.lines.replace(lines);
            let flow = self.obj().downgrade();
            glib::idle_add_local_once(move || {
                let Some(flow) = flow.upgrade() else {
                    return;
                };
                let lines = flow.imp().lines.borrow().clone();
                if let Some(changed) = flow.imp().lines_changed.borrow().as_ref() {
                    changed(&lines);
                }
            });
        }
    }

    pub struct Place {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
        pub line: usize,
    }

    impl Flow {
        fn visible_children(&self) -> Vec<gtk4::Widget> {
            let mut children = Vec::new();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                if widget.is_visible() {
                    children.push(widget.clone());
                }
                child = widget.next_sibling();
            }
            children
        }

        fn place(&self, children: &[gtk4::Widget], width: i32) -> Vec<Place> {
            let spacing = self.spacing.get();
            let (mut x, mut y, mut line, mut line_height) = (0, 0, 0, 0);
            let mut places = Vec::with_capacity(children.len());
            for child in children {
                let child_width = child.measure(gtk4::Orientation::Horizontal, -1).1;
                let child_height = child.measure(gtk4::Orientation::Vertical, child_width).1;
                if x > 0 && x + child_width > width {
                    x = 0;
                    y += line_height + spacing;
                    line += 1;
                    line_height = 0;
                }
                places.push(Place {
                    x,
                    y,
                    width: child_width,
                    height: child_height,
                    line,
                });
                x += child_width + spacing;
                line_height = line_height.max(child_height);
            }
            places
        }
    }
}

glib::wrapper! {
    pub struct Flow(ObjectSubclass<imp::Flow>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Flow {
    pub fn new(spacing: i32) -> Self {
        let flow: Flow = glib::Object::new();
        flow.imp().spacing.set(spacing);
        flow
    }

    pub fn append(&self, child: &impl IsA<gtk4::Widget>) {
        child.set_parent(self);
    }

    pub fn connect_lines_changed(&self, action: impl Fn(&[usize]) + 'static) {
        self.imp().lines_changed.replace(Some(Box::new(action)));
    }
}

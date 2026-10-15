use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};

use super::{GroupButton, index_in_parent};
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::input::ripple::Token;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ButtonGroup {
        pub theme: RefCell<Option<SharedTheme>>,
        pub is_vertical: Cell<bool>,
        pub spacing: Cell<f64>,
        pub padding: Cell<f64>,
        pub color: Cell<Option<Token>>,
        pub click_index: Cell<i32>,
        pub phantoms: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ButtonGroup {
        const NAME: &'static str = "ProscenioButtonGroup";
        type Type = super::ButtonGroup;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for ButtonGroup {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ButtonGroup {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let padding = self.padding.get() * 2.0;
            let size = if orientation == self.along() {
                self.content_length() + padding
            } else {
                self.visible_children()
                    .iter()
                    .map(|child| child.measure(orientation, -1).1)
                    .max()
                    .unwrap_or(0) as f64
                    + padding
            }
            .round() as i32;
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let padding = self.padding.get();
            let spacing = self.spacing.get();
            let visible_children = self.visible_children();
            if visible_children.is_empty() {
                return;
            }
            let is_vertical = self.is_vertical.get();
            let (length, across) = if is_vertical {
                (height, width)
            } else {
                (width, height)
            };
            let inner_across = across as f64 - padding * 2.0;
            let available =
                length as f64 - padding * 2.0 - spacing * (visible_children.len() as f64 - 1.0);

            let hints: Vec<(f64, f64, f64)> = visible_children
                .iter()
                .map(|child| {
                    let main_size = match child.downcast_ref::<GroupButton>() {
                        Some(button) if is_vertical => button.imp().current_height(),
                        Some(button) => button.imp().current_width(),
                        None => child.measure(self.along(), -1).1 as f64,
                    };
                    if self.fills(child) {
                        (0.0, main_size, f64::INFINITY)
                    } else {
                        (main_size, main_size, main_size)
                    }
                })
                .collect();
            let sizes = distribute(&hints, available);

            let cross_orientation = if is_vertical {
                gtk4::Orientation::Horizontal
            } else {
                gtk4::Orientation::Vertical
            };
            let mut start = padding;
            for (child, size) in visible_children.iter().zip(sizes) {
                let natural = child.measure(cross_orientation, -1).1 as f64;
                let child_across = if self.fills(child) {
                    inner_across
                } else {
                    natural.min(inner_across)
                };
                let offset = padding + ((inner_across - child_across) / 2.0 + 0.5).floor();
                let near = start.round();
                let far = (start + size).round();
                let extent = (far - near).max(0.0) as i32;
                let (child_width, child_height, x, y) = if is_vertical {
                    (child_across.round() as i32, extent, offset, near)
                } else {
                    (extent, child_across.round() as i32, near, offset)
                };
                child.allocate(
                    child_width,
                    child_height,
                    -1,
                    Some(
                        gtk4::gsk::Transform::new()
                            .translate(&graphene::Point::new(x as f32, y as f32)),
                    ),
                );
                start += size + spacing;
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            if let Some(token) = self.color.get()
                && let Some(shared) = self.theme.borrow().clone()
            {
                let group = self.obj();
                let (width, height) = (group.width() as f32, group.height() as f32);
                let padding = self.padding.get();
                let limit = width.min(height) / 2.0;
                let corner = |child: Option<gtk4::Widget>| {
                    let radius = child
                        .and_downcast::<GroupButton>()
                        .map(|button| button.imp().effective_radius() + padding)
                        .unwrap_or(rounding::SMALL as f64);
                    (radius as f32).min(limit)
                };
                let first = corner(group.first_child());
                let last = corner(group.last_child());
                let (top_left, top_right, bottom_right, bottom_left) = if self.is_vertical.get() {
                    (first, first, last, last)
                } else {
                    (first, last, last, first)
                };
                let bounds = graphene::Rect::new(0.0, 0.0, width, height);
                let shape = gsk::RoundedRect::new(
                    bounds,
                    graphene::Size::new(top_left, top_left),
                    graphene::Size::new(top_right, top_right),
                    graphene::Size::new(bottom_right, bottom_right),
                    graphene::Size::new(bottom_left, bottom_left),
                );
                snapshot.push_rounded_clip(&shape);
                snapshot.append_color(&token(&shared.borrow()), &bounds);
                snapshot.pop();
            }
            self.parent_snapshot(snapshot);
        }
    }

    impl ButtonGroup {
        pub fn visible_children(&self) -> Vec<gtk4::Widget> {
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

        fn along(&self) -> gtk4::Orientation {
            if self.is_vertical.get() {
                gtk4::Orientation::Vertical
            } else {
                gtk4::Orientation::Horizontal
            }
        }

        fn content_length(&self) -> f64 {
            let mut total = 0.0;
            let mut count = self.phantoms.get();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                count += 1;
                if widget.get_visible() {
                    total += match widget.downcast_ref::<GroupButton>() {
                        Some(button) if self.is_vertical.get() => button.imp().base_height.get(),
                        Some(button) => {
                            let imp = button.imp();
                            imp.base.get().value(imp.now())
                        }
                        None => widget.measure(self.along(), -1).1 as f64,
                    };
                }
                child = widget.next_sibling();
            }
            total + self.spacing.get() * (count as f64 - 1.0).max(0.0)
        }

        fn fills(&self, child: &gtk4::Widget) -> bool {
            let Some(button) = child.downcast_ref::<GroupButton>() else {
                return false;
            };
            if let Some(fills) = button.imp().fill_override.get() {
                return fills;
            }
            let index = index_in_parent(child);
            let click = self.click_index.get();
            click - 1 <= index && index <= click + 1
        }
    }

    fn distribute(hints: &[(f64, f64, f64)], target: f64) -> Vec<f64> {
        let minimum: f64 = hints.iter().map(|hint| hint.0).sum();
        let natural: f64 = hints.iter().map(|hint| hint.1).sum();
        if target <= minimum {
            return hints.iter().map(|hint| hint.0).collect();
        }
        if target < natural {
            let available = target - minimum;
            let total_slack = natural - minimum;
            let factors: Vec<f64> = hints
                .iter()
                .map(|hint| {
                    let slack = hint.1 - hint.0;
                    if slack <= 0.0 {
                        return 0.0;
                    }
                    slack * (available / total_slack).powf(slack / total_slack)
                })
                .collect();
            let sum: f64 = factors.iter().sum();
            return hints
                .iter()
                .zip(factors)
                .map(|(hint, factor)| {
                    if sum <= 0.0 {
                        hint.0
                    } else {
                        hint.0 + available * factor / sum
                    }
                })
                .collect();
        }
        let growable_count = hints.iter().filter(|hint| hint.2 > hint.1).count();
        if growable_count == 0 {
            return hints.iter().map(|hint| hint.1).collect();
        }
        let extra = (target - natural) / growable_count as f64;
        hints
            .iter()
            .map(|hint| {
                if hint.2 > hint.1 {
                    (hint.1 + extra).min(hint.2)
                } else {
                    hint.1
                }
            })
            .collect()
    }
}

glib::wrapper! {
    pub struct ButtonGroup(ObjectSubclass<imp::ButtonGroup>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl ButtonGroup {
    pub fn new(theme: &SharedTheme) -> Self {
        let group: ButtonGroup = glib::Object::new();
        let imp = group.imp();
        imp.theme.replace(Some(theme.clone()));
        imp.spacing.set(5.0);
        imp.click_index.set(-1);
        group
    }

    pub fn set_vertical(&self, is_vertical: bool) {
        self.imp().is_vertical.set(is_vertical);
        self.queue_resize();
    }

    pub fn set_spacing(&self, spacing: f64) {
        self.imp().spacing.set(spacing);
        self.queue_resize();
    }

    pub fn set_phantom_children(&self, count: i32) {
        self.imp().phantoms.set(count);
        self.queue_resize();
    }

    pub fn set_padding(&self, padding: f64) {
        self.imp().padding.set(padding);
        self.queue_resize();
    }

    pub fn set_color(&self, color: Token) {
        self.imp().color.set(Some(color));
        self.queue_draw();
    }

    pub fn append(&self, child: &impl IsA<gtk4::Widget>) {
        child.set_parent(self);
        self.queue_resize();
    }

    pub(super) fn set_click_index(&self, index: i32) {
        self.imp().click_index.set(index);
        self.queue_allocate();
    }
}

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::ui::anim;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Clip {
        pub length: Cell<i32>,
        pub vertical: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Clip {
        const NAME: &'static str = "ProscenioRevealClip";
        type Type = super::Clip;
        type ParentType = gtk4::LayoutManager;
    }

    impl ObjectImpl for Clip {}

    impl LayoutManagerImpl for Clip {
        fn measure(
            &self,
            widget: &gtk4::Widget,
            orientation: gtk4::Orientation,
            _for_size: i32,
        ) -> (i32, i32, i32, i32) {
            if orientation == self.along() {
                let length = self.length.get();
                return (length, length, -1, -1);
            }
            let (mut minimum, mut natural) = (0, 0);
            let mut child = widget.first_child();
            while let Some(current) = child {
                if current.should_layout() {
                    let (child_minimum, child_natural, _, _) = current.measure(orientation, -1);
                    minimum = minimum.max(child_minimum);
                    natural = natural.max(child_natural);
                }
                child = current.next_sibling();
            }
            (minimum, natural, -1, -1)
        }

        fn allocate(&self, widget: &gtk4::Widget, width: i32, height: i32, baseline: i32) {
            let mut child = widget.first_child();
            while let Some(current) = child {
                if current.should_layout() {
                    let natural = current.measure(self.along(), -1).1;
                    if self.vertical.get() {
                        current.allocate(width, natural.max(height), -1, None);
                    } else {
                        current.allocate(natural.max(width), height, baseline, None);
                    }
                }
                child = current.next_sibling();
            }
        }
    }

    impl Clip {
        fn along(&self) -> gtk4::Orientation {
            if self.vertical.get() {
                gtk4::Orientation::Vertical
            } else {
                gtk4::Orientation::Horizontal
            }
        }
    }
}

glib::wrapper! {
    pub struct Clip(ObjectSubclass<imp::Clip>)
        @extends gtk4::LayoutManager;
}

impl Clip {
    fn new(orientation: gtk4::Orientation, length: i32) -> Self {
        let clip: Clip = glib::Object::new();
        clip.imp()
            .vertical
            .set(orientation == gtk4::Orientation::Vertical);
        clip.imp().length.set(length);
        clip
    }

    fn set_length(&self, length: i32) {
        if self.imp().length.replace(length) != length {
            self.layout_changed();
        }
    }
}

const SPACING: f64 = 15.0;
const LENGTH_MILLIS: f64 = 400.0;
const MARGIN_MILLIS: f64 = 200.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    Leading,
    Trailing,
    None,
}

pub fn wrap(child: &impl IsA<gtk4::Widget>, shown: bool) -> (gtk4::Box, Rc<dyn Fn(bool)>) {
    with_gap(child, shown, Gap::Trailing)
}

pub fn with_gap(
    child: &impl IsA<gtk4::Widget>,
    shown: bool,
    gap: Gap,
) -> (gtk4::Box, Rc<dyn Fn(bool)>) {
    reveal(child, shown, gap, gtk4::Orientation::Horizontal, SPACING)
}

pub fn vertical(
    child: &impl IsA<gtk4::Widget>,
    shown: bool,
    spacing: f64,
) -> (gtk4::Box, Rc<dyn Fn(bool)>) {
    reveal(
        child,
        shown,
        Gap::Trailing,
        gtk4::Orientation::Vertical,
        spacing,
    )
}

fn reveal(
    child: &impl IsA<gtk4::Widget>,
    shown: bool,
    gap: Gap,
    orientation: gtk4::Orientation,
    spacing: f64,
) -> (gtk4::Box, Rc<dyn Fn(bool)>) {
    let holder = gtk4::Box::new(orientation, 0);
    holder.set_overflow(gtk4::Overflow::Hidden);
    holder.append(child.as_ref());

    let natural = child.as_ref().measure(orientation, -1).1.max(1) as f64;

    let length = anim::Motion::new(
        &holder,
        if shown { natural } else { 0.0 },
        LENGTH_MILLIS,
        anim::EMPHASIZED_DECEL,
    );
    let margin = anim::Motion::new(
        &holder,
        if shown { spacing } else { 0.0 },
        MARGIN_MILLIS,
        anim::EXPRESSIVE_EFFECTS,
    );
    let clip = Clip::new(orientation, length.get() as i32);
    holder.set_layout_manager(Some(clip.clone()));
    fn place(
        holder: &gtk4::Box,
        clip: &Clip,
        gap: Gap,
        orientation: gtk4::Orientation,
        length: i32,
        margin: i32,
    ) {
        clip.set_length(length);
        match (orientation, gap) {
            (_, Gap::None) => {}
            (gtk4::Orientation::Vertical, Gap::Leading) => holder.set_margin_top(margin),
            (gtk4::Orientation::Vertical, Gap::Trailing) => holder.set_margin_bottom(margin),
            (_, Gap::Leading) => holder.set_margin_start(margin),
            (_, Gap::Trailing) => holder.set_margin_end(margin),
        }
    }
    place(
        &holder,
        &clip,
        gap,
        orientation,
        length.get() as i32,
        margin.get() as i32,
    );
    holder.set_visible(shown);

    let set = {
        let holder = holder.clone();
        let length = length.clone();
        let margin = margin.clone();
        move |reveal: bool| {
            if reveal {
                holder.set_visible(true);
            }
            length.to(if reveal { natural } else { 0.0 });
            margin.to(if reveal { spacing } else { 0.0 });

            let length = length.clone();
            let margin = margin.clone();
            let clip = clip.clone();
            holder.add_tick_callback(move |holder, _| {
                place(
                    holder,
                    &clip,
                    gap,
                    orientation,
                    length.get().round() as i32,
                    margin.get().round() as i32,
                );
                if length.running() || margin.running() {
                    return gtk4::glib::ControlFlow::Continue;
                }
                holder.set_visible(length.get() > 0.5);
                gtk4::glib::ControlFlow::Break
            });
        }
    };

    (holder, Rc::new(set))
}

use gtk4::prelude::*;
use std::rc::Rc;

use crate::ui::anim;

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
    fn place(
        holder: &gtk4::Box,
        gap: Gap,
        orientation: gtk4::Orientation,
        length: i32,
        margin: i32,
    ) {
        match orientation {
            gtk4::Orientation::Vertical => {
                holder.set_size_request(-1, length);
                match gap {
                    Gap::Leading => holder.set_margin_top(margin),
                    Gap::Trailing => holder.set_margin_bottom(margin),
                    Gap::None => {}
                }
            }
            _ => {
                holder.set_size_request(length, -1);
                match gap {
                    Gap::Leading => holder.set_margin_start(margin),
                    Gap::Trailing => holder.set_margin_end(margin),
                    Gap::None => {}
                }
            }
        }
    }
    place(
        &holder,
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
            holder.add_tick_callback(move |holder, _| {
                place(
                    holder,
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

mod button;
mod container;

use gtk4::prelude::*;

pub use button::{GroupButton, Look};
pub use container::ButtonGroup;

fn index_in_parent(widget: &impl IsA<gtk4::Widget>) -> i32 {
    let mut index = 0;
    let mut sibling = widget.as_ref().prev_sibling();
    while let Some(previous) = sibling {
        index += 1;
        sibling = previous.prev_sibling();
    }
    index
}

fn tree_cursor(widget: &gtk4::Widget, name: Option<&str>) {
    if !widget.is::<gtk4::Popover>() {
        widget.set_cursor_from_name(name);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        tree_cursor(&current, name);
        child = current.next_sibling();
    }
}

pub fn pointer_cursor(widget: &gtk4::Widget) {
    if widget.cursor().is_none() && !widget.is::<gtk4::Popover>() {
        widget.set_cursor_from_name(Some("pointer"));
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        pointer_cursor(&current);
        child = current.next_sibling();
    }
}

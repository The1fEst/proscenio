use gtk4::prelude::*;
use std::rc::Rc;

use super::buttons::AppButton;
use super::{APP_SPACING, Dock, refresh_docks};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Drag {
    pub(super) is_dragging: bool,
    pub(super) index: i64,
    pub(super) drop_position: i64,
    pub(super) x: f64,
}

pub(super) const IDLE_DRAG: Drag = Drag {
    is_dragging: false,
    index: -1,
    drop_position: -1,
    x: 0.0,
};

impl Dock {
    pub(super) fn order(&self) -> Vec<usize> {
        let count = self.buttons.borrow().len();
        let drag = self.drag.get();
        if drag.index < 0 || drag.drop_position < 0 {
            return (0..count).collect();
        }
        let mut order: Vec<usize> = (0..count)
            .filter(|index| *index as i64 != drag.index)
            .collect();
        order.insert(
            (drag.drop_position as usize).min(order.len()),
            drag.index as usize,
        );
        order
    }

    pub(super) fn drop_position(&self, x: f64) -> i64 {
        let buttons = self.buttons.borrow();
        let drag = self.drag.get();
        let mut position = 0;
        let mut reach = 0.0;
        for (index, entry) in buttons.iter().enumerate() {
            if index as i64 == drag.index {
                continue;
            }
            let slot = entry.width();
            if x < reach + slot / 2.0 {
                break;
            }
            reach += slot + APP_SPACING;
            position += 1;
        }
        position
    }

    pub(super) fn begin_drag(self: &Rc<Self>, entry: &Rc<AppButton>) {
        let index = self
            .buttons
            .borrow()
            .iter()
            .position(|button| Rc::ptr_eq(button, entry))
            .map(|index| index as i64)
            .unwrap_or(-1);
        self.drag.set(Drag {
            is_dragging: true,
            index,
            drop_position: -1,
            x: entry.x.get().target() + entry.width() / 2.0,
        });
        entry
            .button
            .insert_before(&self.apps, None::<&gtk4::Widget>);
    }

    pub(super) fn update_drag(self: &Rc<Self>, x: f64) {
        let mut drag = self.drag.get();
        drag.x = x;
        self.drag.set(drag);
        let drop_position = self.drop_position(x);
        let mut drag = self.drag.get();
        drag.drop_position = drop_position;
        self.drag.set(drag);
        self.layout(true);
    }

    fn cancel_drag(&self) {
        self.release_dragged();
        self.drag.set(IDLE_DRAG);
    }

    fn release_dragged(&self) {
        let drag = self.drag.get();
        if !drag.is_dragging || drag.index < 0 {
            return;
        }
        if let Some(entry) = self.buttons.borrow().get(drag.index as usize) {
            let mut x = entry.x.get();
            x.jump(drag.x - entry.width() / 2.0);
            entry.x.set(x);
        }
    }

    pub(super) fn commit_drag(self: &Rc<Self>) {
        let drag = self.drag.get();
        if drag.index < 0 || drag.drop_position < 0 {
            self.cancel_drag();
            self.layout(true);
            return;
        }
        let order = self.order();
        let buttons = self.buttons.borrow();
        let Some(separator_position) = order
            .iter()
            .position(|index| buttons[*index].is_separator())
        else {
            drop(buttons);
            self.cancel_drag();
            self.layout(true);
            return;
        };
        let pins: Vec<String> = order[..separator_position]
            .iter()
            .map(|index| {
                let entry = &buttons[*index];
                entry
                    .windows
                    .borrow()
                    .first()
                    .map(|window| window.class.clone())
                    .unwrap_or_else(|| entry.id.clone())
            })
            .collect();
        drop(buttons);
        if *self.config.dock_pinned.borrow() == pins {
            self.cancel_drag();
            self.layout(true);
            return;
        }
        self.release_dragged();
        let mut drag = self.drag.get();
        drag.is_dragging = false;
        self.drag.set(drag);
        self.config.set_dock_pins(pins);
        refresh_docks();
    }
}

pub(super) fn follow_drag(drag: Drag, dragged_id: Option<&str>, ids: &[&str]) -> Drag {
    if !drag.is_dragging {
        return IDLE_DRAG;
    }
    match dragged_id.and_then(|dragged_id| ids.iter().position(|id| *id == dragged_id)) {
        Some(index) => Drag {
            index: index as i64,
            ..drag
        },
        None => IDLE_DRAG,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::dock::taskbar::SEPARATOR;

    #[test]
    fn a_drag_follows_its_app_when_the_dock_changes() {
        let drag = Drag {
            is_dragging: true,
            index: 3,
            drop_position: 1,
            x: 40.0,
        };
        assert_eq!(
            follow_drag(drag, Some("kitty"), &["firefox", SEPARATOR, "kitty"]),
            Drag { index: 2, ..drag }
        );
        assert_eq!(
            follow_drag(drag, Some("kitty"), &["firefox", SEPARATOR]),
            IDLE_DRAG
        );
        assert_eq!(
            follow_drag(
                Drag {
                    is_dragging: false,
                    ..drag
                },
                Some("kitty"),
                &["firefox", SEPARATOR, "kitty"]
            ),
            IDLE_DRAG
        );
    }
}

use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

pub mod display;
pub mod input;
pub mod layout;
pub mod overlay;

pub fn coalesce(action: Rc<dyn Fn()>) -> Rc<dyn Fn()> {
    let is_pending = Rc::new(Cell::new(false));
    Rc::new(move || {
        if is_pending.replace(true) {
            return;
        }
        let is_pending = is_pending.clone();
        let action = action.clone();
        glib::idle_add_local_once(move || {
            is_pending.set(false);
            action();
        });
    })
}

use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

pub mod centred;
pub mod column;
pub mod controls;
pub mod customicon;
pub mod fixedheight;
pub mod fixedwidth;
pub mod flickable;
pub mod flow;
pub mod group;
pub mod loading;
pub mod materialshape;
pub mod paint;
pub mod popup;
pub mod progress;
pub mod reveal;
pub mod ring;
pub mod ripple;
pub mod row;
pub mod scrollhint;
pub mod secondarytabs;
pub mod selection;
pub mod slide;
pub mod slider;
pub mod spinbox;
pub mod swipe;
pub mod text;
pub mod textfield;
pub mod toolbar;
pub mod tooltip;
pub mod trimmedbin;
pub mod viewport;
pub mod windowdialog;

pub fn coalesce(action: Rc<dyn Fn()>) -> Rc<dyn Fn()> {
    let pending = Rc::new(Cell::new(false));
    Rc::new(move || {
        if pending.replace(true) {
            return;
        }
        let pending = pending.clone();
        let action = action.clone();
        glib::idle_add_local_once(move || {
            pending.set(false);
            action();
        });
    })
}

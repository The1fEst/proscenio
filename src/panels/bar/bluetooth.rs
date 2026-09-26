use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::services::bluez::Bluez;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::text;

pub fn build(bluez: &Bluez, scope: &Scope) -> gtk4::Widget {
    let label = text::symbol("", pixel_size::LARGER as f64);
    label.set_valign(gtk4::Align::Center);

    let show = {
        let label = label.clone();
        let bluez = bluez.clone();
        move || {
            label.set_visible(bluez.available.get());
            label.set_text(symbol(&bluez));
        }
    };
    show();
    scope.keep(bluez.subscribe(show));

    label.upcast()
}

pub fn symbol(bluez: &Bluez) -> &'static str {
    if bluez.connected_name.borrow().is_some() {
        "bluetooth_connected"
    } else if bluez.powered.get() {
        "bluetooth"
    } else {
        "bluetooth_disabled"
    }
}

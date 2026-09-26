use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::services::net::Net;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::text;

pub fn build(net: &Net, scope: &Scope) -> gtk4::Widget {
    let label = text::symbol("", pixel_size::LARGER as f64);
    label.set_valign(gtk4::Align::Center);

    let show = {
        let label = label.clone();
        let symbol = net.symbol.clone();
        move || label.set_text(&symbol.borrow())
    };
    show();
    scope.keep(net.subscribe(show));

    label.upcast()
}

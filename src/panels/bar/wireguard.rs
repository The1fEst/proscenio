use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::services::net::Net;
use crate::ui::widgets::{customicon, reveal};

const SIZE: i32 = 19;

pub fn build(net: &Net, scope: &Scope) -> gtk4::Widget {
    let icon = customicon::build("wireguard-symbolic", SIZE);
    icon.set_valign(gtk4::Align::Center);

    let (holder, set) = reveal::with_gap(&icon, net.wireguard.get(), reveal::Gap::Leading);

    let wireguard = net.wireguard.clone();
    scope.keep(net.subscribe(move || set(wireguard.get())));

    holder.upcast()
}

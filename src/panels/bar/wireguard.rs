use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::services::network::net::Net;
use crate::ui::widgets::{display::customicon, overlay::reveal};

const SIZE: i32 = 19;

pub fn build(net: &Net, scope: &Scope) -> gtk4::Widget {
    let icon = customicon::build("wireguard-symbolic", SIZE);
    icon.set_valign(gtk4::Align::Center);

    let (holder, set) = reveal::with_gap(&icon, net.is_wireguard_up.get(), reveal::Gap::Leading);

    let is_wireguard_up = net.is_wireguard_up.clone();
    scope.keep(net.subscribe(move || set(is_wireguard_up.get())));

    holder.upcast()
}

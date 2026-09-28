use gtk4::glib;
use gtk4::prelude::*;

use crate::core::process;
use crate::core::scope::Scope;
use crate::services::net::Net;
use crate::ui::widgets::{customicon, reveal};

const SIZE: i32 = 19;

pub fn build(net: &Net, scope: &Scope) -> gtk4::Widget {
    let icon = customicon::build("wireguard-symbolic", SIZE);
    icon.set_valign(gtk4::Align::Center);

    let (holder, set) = reveal::with_gap(&icon, false, reveal::Gap::Leading);

    let check = move || {
        let set = set.clone();
        glib::spawn_future_local(async move {
            set(active().await);
        });
    };
    check();
    scope.keep(net.subscribe(check));

    holder.upcast()
}

async fn active() -> bool {
    let command = process::command(&["nmcli", "connection", "show", "--active"]);
    process::capture_text(command)
        .await
        .is_some_and(|text| text.contains("WireGuard"))
}

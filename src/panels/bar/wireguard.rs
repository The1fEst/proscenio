use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::services::background::BackgroundTasks;
use crate::ui::widgets::{customicon, reveal};

const SIZE: i32 = 19;

pub fn build(background: &Rc<BackgroundTasks>, scope: &Scope) -> gtk4::Widget {
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
    scope.keep(background.add_scoped("wireguard", move || {
        check();
        Ok(())
    }));

    holder.upcast()
}

async fn active() -> bool {
    let Ok(process) = gio::Subprocess::newv(
        &[
            std::ffi::OsStr::new("nmcli"),
            std::ffi::OsStr::new("connection"),
            std::ffi::OsStr::new("show"),
            std::ffi::OsStr::new("--active"),
        ],
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    ) else {
        return false;
    };
    let Ok((stdout, _)) = process.communicate_utf8_future(None).await else {
        return false;
    };
    stdout.is_some_and(|text| text.contains("WireGuard"))
}

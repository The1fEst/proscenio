use gtk4::prelude::*;
use gtk4::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Context, Page};
use crate::platform::hyprland::inputdevices::{self, Device, Group, Kind};
use crate::services::hardware::deviceoptions::DeviceOptions;
use crate::ui::widgets::input::controls::ConfigSwitch;

const POLL: Duration = Duration::from_secs(2);
const NOTICE: &str = "A mouse or keyboard often shows up as several devices. Turning off an extra one keeps it from taking over the keyboard layout; consumer and system control devices carry the media and power keys.";

fn icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Keyboard => "keyboard",
        Kind::Mouse => "mouse",
        Kind::Tablet => "stylus",
        Kind::Touch => "touch_app",
        Kind::Switch => "toggle_on",
    }
}

fn caption(device: &Device) -> String {
    if device.is_main {
        trf("%1 · main keyboard, %2", &[&device.name, &device.keymap])
    } else {
        device.name.clone()
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let devices = DeviceOptions::new();

    let section = page.section("devices_other", &tr("Input devices"));
    page.notice(&section, "info", &tr(NOTICE));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    section.append(&list);

    let rows: Rc<RefCell<Vec<Rc<ConfigSwitch>>>> = Rc::default();
    let last_groups: Rc<RefCell<Vec<Group>>> = Rc::default();
    let rebuild = Rc::new({
        let page = Rc::downgrade(&page);
        let devices = Rc::downgrade(&devices);
        let rows = rows.clone();
        move |groups: Vec<Group>| {
            let (Some(page), Some(devices)) = (page.upgrade(), devices.upgrade()) else {
                return;
            };
            if *last_groups.borrow() == groups {
                return;
            }
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let mut switches = Vec::new();
            for group in &groups {
                let (content, _) = page.unkept_subsection(&list, &group.label, "");
                for device in &group.devices {
                    let name = device.name.clone();
                    let on_toggled = {
                        let devices = Rc::downgrade(&devices);
                        let name = name.clone();
                        move |is_on: bool| {
                            if let Some(devices) = devices.upgrade() {
                                devices.set(&name, "enabled", &is_on.to_string());
                            }
                        }
                    };
                    let switch = ConfigSwitch::new(
                        &page.theme,
                        icon(device.kind),
                        &caption(device),
                        on_toggled,
                    );
                    switch.button.set_hexpand(true);
                    switch.bind({
                        let devices = Rc::downgrade(&devices);
                        move || {
                            devices
                                .upgrade()
                                .is_none_or(|devices| devices.value_of(&name, "enabled") != "false")
                        }
                    });
                    content.append(&switch.button);
                    switches.push(switch);
                }
            }
            rows.replace(switches);
            last_groups.replace(groups);
        }
    });
    let is_reading = Rc::new(Cell::new(false));
    page.every(POLL, move || {
        if is_reading.replace(true) {
            return;
        }
        let rebuild = rebuild.clone();
        let is_reading = is_reading.clone();
        glib::spawn_future_local(async move {
            let groups = gio::spawn_blocking(inputdevices::read).await;
            is_reading.set(false);
            if let Ok(groups) = groups {
                rebuild(groups);
            }
        });
    });
    page.keep(devices.watch(move || {
        for row in rows.borrow().iter() {
            row.refresh();
        }
    }));
    page.keep(devices);
    page
}

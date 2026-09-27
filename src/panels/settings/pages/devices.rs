use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::panels::settings::content::{Context, Page};
use crate::platform::inputdevices::{self, Device, Group, Kind};
use crate::services::deviceoptions::DeviceOptions;
use crate::ui::widgets::controls::ConfigSwitch;

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
    if device.main {
        format!("{} · main keyboard, {}", device.name, device.keymap)
    } else {
        device.name.clone()
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let devices = DeviceOptions::new();

    let section = page.section("devices_other", "Input devices");
    page.notice(&section, "info", NOTICE);
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    section.append(&list);

    let rows: Rc<RefCell<Vec<Rc<ConfigSwitch>>>> = Rc::default();
    let shown: Rc<RefCell<Vec<Group>>> = Rc::default();
    let rebuild = {
        let page = Rc::downgrade(&page);
        let devices = Rc::downgrade(&devices);
        let rows = rows.clone();
        move || {
            let (Some(page), Some(devices)) = (page.upgrade(), devices.upgrade()) else {
                return;
            };
            let groups = inputdevices::read();
            if *shown.borrow() == groups {
                return;
            }
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let mut made = Vec::new();
            for group in &groups {
                let (content, _) = page.unkept_subsection(&list, &group.label, "");
                for device in &group.devices {
                    let name = device.name.clone();
                    let toggled = {
                        let devices = Rc::downgrade(&devices);
                        let name = name.clone();
                        move |on: bool| {
                            if let Some(devices) = devices.upgrade() {
                                devices.set(&name, "enabled", &on.to_string());
                            }
                        }
                    };
                    let switch = ConfigSwitch::new(
                        &page.theme,
                        icon(device.kind),
                        &caption(device),
                        toggled,
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
                    made.push(switch);
                }
            }
            rows.replace(made);
            shown.replace(groups);
        }
    };
    page.every(POLL, rebuild);
    page.keep(devices.watch(move || {
        for row in rows.borrow().iter() {
            row.refresh();
        }
    }));
    page.keep(devices);
    page
}

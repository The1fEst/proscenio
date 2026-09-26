use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::panels::notifications::list::Placeholder;
use crate::panels::settings::content::{Context, Page};
use crate::services::bluez::{Bluez, Device, Setup, friendly_order};
use crate::ui::shapes::Shape;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::icon_button;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;

const PLACEHOLDER_HEIGHT: i32 = 220;
const ROW_HEIGHT: i32 = 56;
const ROW_START: i32 = 12;
const ROW_END: i32 = 8;
const ROW_SPACING: i32 = 10;
const SEARCHING_MARGIN: i32 = 8;
const LOOK_AGAIN: Duration = Duration::from_secs(1);

struct Looking {
    bluez: Bluez,
    source: Option<glib::SourceId>,
}

impl Drop for Looking {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            source.remove();
        }
        self.bluez.set_discovering(false);
    }
}

type Shown = Option<(Vec<Device>, Option<String>, Option<String>, Vec<String>)>;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let bluez = context.services.bluez.clone();
    let setup = Setup::new(&bluez);

    let controls = page.section("", "");
    let switch = page.switch(&controls, "bluetooth", "Bluetooth", {
        let bluez = bluez.clone();
        move |wanted| {
            if bluez.powered.get() != wanted {
                bluez.set_powered(wanted);
            }
        }
    });
    switch.bind({
        let bluez = bluez.clone();
        move || bluez.powered.get()
    });

    let missing = placeholder(
        &page,
        "No Bluetooth Found",
        "Plug in a dongle to use Bluetooth",
    );
    let off = placeholder(
        &page,
        "Bluetooth Turned Off",
        "Turn on to connect devices and receive file transfers",
    );

    let (devices, busy) = page.busy_section("devices", "Devices");
    let searching = text::styled("Searching for devices…");
    text::set_color(&searching, "colSubtext");
    searching.set_xalign(0.0);
    searching.set_margin_start(SEARCHING_MARGIN);
    devices.append(&searching);
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    devices.append(&rows);

    let looking = Rc::new(RefCell::new(Looking {
        bluez: bluez.clone(),
        source: None,
    }));
    let shown: RefCell<Shown> = RefCell::new(None);
    let held: RefCell<Vec<Box<dyn Any>>> = RefCell::new(Vec::new());
    let follow = {
        let bluez = bluez.clone();
        let setup = Rc::downgrade(&setup);
        let page = Rc::downgrade(&page);
        let controls = controls.parent();
        let devices_section = devices.parent();
        let looking = Rc::downgrade(&looking);
        move || {
            let (Some(setup), Some(page), Some(looking)) =
                (setup.upgrade(), page.upgrade(), looking.upgrade())
            else {
                return;
            };
            let (available, enabled) = (bluez.available.get(), bluez.powered.get());
            if let Some(controls) = &controls {
                controls.set_visible(available);
            }
            missing.set_visible(!available);
            off.set_visible(available && !enabled);
            if let Some(section) = &devices_section {
                section.set_visible(available && enabled);
            }
            let discovering = bluez.discovering.get();
            busy.area.set_visible(discovering);
            busy.set_loading(discovering);
            switch.refresh();

            let wanted = available && enabled && !setup.busy();
            let mut looking = looking.borrow_mut();
            match (wanted, looking.source.is_some()) {
                (true, false) => {
                    let bluez = bluez.clone();
                    let look = move || {
                        if !bluez.discovering.get() {
                            bluez.set_discovering(true);
                        }
                    };
                    look();
                    looking.source = Some(glib::timeout_add_local(LOOK_AGAIN, move || {
                        look();
                        glib::ControlFlow::Continue
                    }));
                }
                (false, true) => {
                    if let Some(source) = looking.source.take() {
                        source.remove();
                    }
                    bluez.set_discovering(false);
                }
                _ => {}
            }
            drop(looking);

            let list = friendly_order(&bluez.devices.borrow());
            let current = (
                list,
                setup.pairing.borrow().clone(),
                setup.connecting.borrow().clone(),
                setup.refused.borrow().clone(),
            );
            if shown.borrow().as_ref() == Some(&current) {
                return;
            }
            searching.set_visible(current.0.is_empty());
            while let Some(child) = rows.first_child() {
                rows.remove(&child);
            }
            let mut kept = held.borrow_mut();
            kept.clear();
            for device in &current.0 {
                kept.push(row(&page, &setup, &bluez, &rows, device));
            }
            shown.replace(Some(current));
        }
    };
    let follow = Rc::new(follow);
    follow();
    page.keep(bluez.subscribe({
        let follow = Rc::downgrade(&follow);
        move || {
            if let Some(follow) = follow.upgrade() {
                follow();
            }
        }
    }));
    setup.connect_changed({
        let follow = Rc::downgrade(&follow);
        move || {
            if let Some(follow) = follow.upgrade() {
                follow();
            }
        }
    });
    page.keep(follow);
    page.keep(looking);
    page.keep(setup);
    page
}

fn placeholder(page: &Page, title: &str, description: &str) -> gtk4::CenterBox {
    let placeholder = Placeholder::build(
        &page.theme,
        "bluetooth_disabled",
        Some(title),
        Some(description),
        Shape::Clover4Leaf,
    );
    let holder = gtk4::CenterBox::new();
    holder.set_orientation(gtk4::Orientation::Vertical);
    holder.set_size_request(-1, PLACEHOLDER_HEIGHT);
    holder.set_center_widget(Some(&placeholder.widget));
    holder.set_visible(false);
    page.append(&holder);
    page.keep(placeholder);
    holder
}

fn row(
    page: &Page,
    setup: &Rc<Setup>,
    bluez: &Bluez,
    parent: &gtk4::Box,
    device: &Device,
) -> Box<dyn Any> {
    let pairing = setup.pairing.borrow().as_deref() == Some(device.path.as_str());
    let connecting = setup.connecting.borrow().as_deref() == Some(device.path.as_str());
    let refused = setup.refused.borrow().contains(&device.path);

    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, ROW_HEIGHT);
    let inside = Row::new(ROW_SPACING);
    inside.set_margin_start(ROW_START);
    inside.set_margin_end(ROW_END);
    inside.set_hexpand(true);

    let symbol = text::symbol(
        if device.icon.is_empty() {
            "bluetooth"
        } else {
            "bluetooth_connected"
        },
        pixel_size::LARGER as f64,
    );
    text::set_color(&symbol, "colOnLayer2");
    inside.append(&Centred::integral(&symbol));

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let name = text::styled(if device.name.is_empty() {
        &device.address
    } else {
        &device.name
    });
    text::set_color(&name, "colOnLayer2");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centred::filling_width(&name));
    let status = text::styled_sized(
        if pairing {
            "Pairing…"
        } else if connecting {
            "Connecting…"
        } else if device.connected {
            "Connected"
        } else if refused {
            "Could not connect. Wake the device and try again"
        } else if device.paired {
            "Paired"
        } else {
            "Not set up"
        },
        pixel_size::SMALLER,
    );
    text::set_color(&status, if refused { "colError" } else { "colSubtext" });
    status.set_xalign(0.0);
    status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centred::filling_width(&status));
    inside.append(&lines);

    if device.paired {
        let (forget, _) = icon_button(&page.theme, "delete", true, "Forget");
        forget.connect_clicked({
            let bluez = bluez.clone();
            let path = device.path.clone();
            move |_| bluez.forget(&path)
        });
        inside.append(&forget);
    }

    let (icon, label) = match (device.paired, device.connected) {
        (false, _) => ("link", "Pair"),
        (true, true) => ("bluetooth_disabled", "Disconnect"),
        (true, false) => ("bluetooth", "Connect"),
    };
    let (action, _) = icon_button(&page.theme, icon, true, label);
    action.set_sensitive(!pairing && !connecting);
    action.connect_clicked({
        let setup = Rc::downgrade(setup);
        let bluez = bluez.clone();
        let (path, paired, connected) = (device.path.clone(), device.paired, device.connected);
        move |_| {
            let Some(setup) = setup.upgrade() else {
                return;
            };
            match (paired, connected) {
                (false, _) => setup.pair(&path),
                (true, true) => bluez.connect_device(&path, false),
                (true, false) => setup.connect(&path),
            }
        }
    });
    inside.append(&action);
    card.append(&inside);
    parent.append(&card);
    Box::new(card)
}

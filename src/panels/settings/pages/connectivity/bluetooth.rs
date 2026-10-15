use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::tr;
use crate::core::tools;
use crate::panels::notifications::list::Placeholder;
use crate::panels::settings::content::{Context, Page};
use crate::services::network::bluez::{Bluez, Device, Setup, friendly_order};
use crate::ui::shapes::Shape;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::icon_button;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

const PLACEHOLDER_HEIGHT: i32 = 220;
const ROW_HEIGHT: i32 = 56;
const ROW_START: i32 = 12;
const ROW_END: i32 = 8;
const ROW_SPACING: i32 = 10;
const SEARCHING_MARGIN: i32 = 8;
const DISCOVERY_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const BLUEZ: &str = "org.bluez";
const NO_BLUEZ: &str = "BlueZ is not on the system bus, so there is no Bluetooth. It comes with the bluez package and runs as bluetooth.service.";

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
    let controls = page.section("", "");
    if !tools::has_system_service(BLUEZ) {
        page.notice(&controls, "info", &tr(NO_BLUEZ));
        return page;
    }
    page.tools_notice(
        &controls,
        &[&tools::BLUETOOTHCTL],
        &tr("pairing a device that asks for confirmation fails"),
    );
    let bluez = context.services.bluez.clone();
    let setup = Setup::new(&bluez);

    let bluetooth_switch = page.switch(&controls, "bluetooth", &tr("Bluetooth"), {
        let bluez = bluez.clone();
        move |is_on| {
            if bluez.is_powered.get() != is_on {
                bluez.set_powered(is_on);
            }
        }
    });
    bluetooth_switch.bind({
        let bluez = bluez.clone();
        move || bluez.is_powered.get()
    });

    let missing_placeholder = placeholder(
        &page,
        &tr("No Bluetooth Found"),
        &tr("Plug in a dongle to use Bluetooth"),
    );
    let off_placeholder = placeholder(
        &page,
        &tr("Bluetooth Turned Off"),
        &tr("Turn on to connect devices and receive file transfers"),
    );

    let (devices_section, loading_indicator) = page.busy_section("devices", &tr("Devices"));
    let searching_label = text::styled(&tr("Searching for devices…"));
    text::set_color(&searching_label, "colSubtext");
    searching_label.set_xalign(0.0);
    searching_label.set_margin_start(SEARCHING_MARGIN);
    devices_section.append(&searching_label);
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    devices_section.append(&rows);

    let discovery = Rc::new(RefCell::new(Looking {
        bluez: bluez.clone(),
        source: None,
    }));
    let last_state: RefCell<Shown> = RefCell::new(None);
    let owned_items: RefCell<Vec<Box<dyn Any>>> = RefCell::new(Vec::new());
    let show_state = {
        let bluez = bluez.clone();
        let setup = Rc::downgrade(&setup);
        let page = Rc::downgrade(&page);
        let controls_card = controls.parent();
        let devices_card = devices_section.parent();
        let discovery = Rc::downgrade(&discovery);
        move || {
            let (Some(setup), Some(page), Some(discovery)) =
                (setup.upgrade(), page.upgrade(), discovery.upgrade())
            else {
                return;
            };
            let (is_available, is_enabled) = (bluez.is_available.get(), bluez.is_powered.get());
            if let Some(controls_card) = &controls_card {
                controls_card.set_visible(is_available);
            }
            missing_placeholder.set_visible(!is_available);
            off_placeholder.set_visible(is_available && !is_enabled);
            if let Some(devices_card) = &devices_card {
                devices_card.set_visible(is_available && is_enabled);
            }
            let is_discovering = bluez.is_discovering.get();
            loading_indicator.area.set_visible(is_discovering);
            loading_indicator.set_loading(is_discovering);
            bluetooth_switch.refresh();

            let should_discover = is_available && is_enabled && !setup.is_busy();
            let mut discovery = discovery.borrow_mut();
            match (should_discover, discovery.source.is_some()) {
                (true, false) => {
                    let bluez = bluez.clone();
                    let start_discovery = move || {
                        if !bluez.is_discovering.get() {
                            bluez.set_discovering(true);
                        }
                    };
                    start_discovery();
                    discovery.source = Some(glib::timeout_add_local(
                        DISCOVERY_RETRY_INTERVAL,
                        move || {
                            start_discovery();
                            glib::ControlFlow::Continue
                        },
                    ));
                }
                (false, true) => {
                    if let Some(source) = discovery.source.take() {
                        source.remove();
                    }
                    bluez.set_discovering(false);
                }
                _ => {}
            }
            drop(discovery);

            let list = friendly_order(&bluez.devices.borrow());
            let current_state = (
                list,
                setup.pairing_path.borrow().clone(),
                setup.connecting_path.borrow().clone(),
                setup.refused_paths.borrow().clone(),
            );
            if last_state.borrow().as_ref() == Some(&current_state) {
                return;
            }
            searching_label.set_visible(current_state.0.is_empty());
            while let Some(child) = rows.first_child() {
                rows.remove(&child);
            }
            let mut items = owned_items.borrow_mut();
            items.clear();
            for device in &current_state.0 {
                items.push(row(&page, &setup, &bluez, &rows, device));
            }
            last_state.replace(Some(current_state));
        }
    };
    let show_state = Rc::new(show_state);
    show_state();
    page.keep(bluez.subscribe({
        let show_state = Rc::downgrade(&show_state);
        move || {
            if let Some(show_state) = show_state.upgrade() {
                show_state();
            }
        }
    }));
    setup.connect_changed({
        let show_state = Rc::downgrade(&show_state);
        move || {
            if let Some(show_state) = show_state.upgrade() {
                show_state();
            }
        }
    });
    page.keep(show_state);
    page.keep(discovery);
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
    let is_pairing = setup.pairing_path.borrow().as_deref() == Some(device.path.as_str());
    let is_connecting = setup.connecting_path.borrow().as_deref() == Some(device.path.as_str());
    let is_refused = setup.refused_paths.borrow().contains(&device.path);

    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, ROW_HEIGHT);
    let content = Row::new(ROW_SPACING);
    content.set_margin_start(ROW_START);
    content.set_margin_end(ROW_END);
    content.set_hexpand(true);

    let symbol = text::symbol(
        if device.icon.is_empty() {
            "bluetooth"
        } else {
            "bluetooth_connected"
        },
        pixel_size::LARGER as f64,
    );
    text::set_color(&symbol, "colOnLayer2");
    content.append(&Centered::integral(&symbol));

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
    lines.append(&Centered::filling_width(&name));
    let status = text::styled_sized(
        &if is_pairing {
            tr("Pairing…")
        } else if is_connecting {
            tr("Connecting…")
        } else if device.is_connected {
            tr("Connected")
        } else if is_refused {
            tr("Could not connect. Wake the device and try again")
        } else if device.is_paired {
            tr("Paired")
        } else {
            tr("Not set up")
        },
        pixel_size::SMALLER,
    );
    text::set_color(&status, if is_refused { "colError" } else { "colSubtext" });
    status.set_xalign(0.0);
    status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centered::filling_width(&status));
    content.append(&lines);

    if device.is_paired {
        let (forget_button, _) = icon_button(&page.theme, "delete", true, &tr("Forget"));
        forget_button.connect_clicked({
            let bluez = bluez.clone();
            let path = device.path.clone();
            move |_| bluez.forget(&path)
        });
        content.append(&forget_button);
    }

    let (icon, label) = match (device.is_paired, device.is_connected) {
        (false, _) => ("link", tr("Pair")),
        (true, true) => ("bluetooth_disabled", tr("Disconnect")),
        (true, false) => ("bluetooth", tr("Connect")),
    };
    let (action_button, _) = icon_button(&page.theme, icon, true, &label);
    action_button.set_sensitive(!is_pairing && !is_connecting);
    action_button.connect_clicked({
        let setup = Rc::downgrade(setup);
        let bluez = bluez.clone();
        let (path, is_paired, is_connected) =
            (device.path.clone(), device.is_paired, device.is_connected);
        move |_| {
            let Some(setup) = setup.upgrade() else {
                return;
            };
            match (is_paired, is_connected) {
                (false, _) => setup.pair(&path),
                (true, true) => bluez.set_device_connected(&path, false),
                (true, false) => setup.connect(&path),
            }
        }
    });
    content.append(&action_button);
    card.append(&content);
    parent.append(&card);
    Box::new(card)
}

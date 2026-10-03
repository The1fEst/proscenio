use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, slider_row};
use crate::panels::settings::pages::mouse::{OPTIONS, SPEED, show_speed};
use crate::platform::hyprconfig::Area;
use crate::services::deviceoptions::DeviceOptions;
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;

const EMPTY_START: i32 = 8;
const DEVICE_KEYS: [&str; 3] = ["sensitivity", "accel_profile", "natural_scroll"];

fn device_number(devices: &DeviceOptions, device: &str, key: &str, fallback: f64) -> f64 {
    let value = devices.value_of(device, key);
    if value.is_empty() {
        fallback
    } else {
        value.trim().parse().unwrap_or(f64::NAN)
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Mouse, &OPTIONS);
    let devices = DeviceOptions::new();

    let only = page.section("", "");
    let empty = text::styled(&tr("No pointing device is connected"));
    text::set_color(&empty, "colSubtext");
    let empty = Centred::new(&empty);
    empty.set_halign(gtk4::Align::Start);
    empty.set_margin_start(EMPTY_START);
    only.append(&empty);
    let device_group = page.subsection(
        &only,
        &tr("Device"),
        &tr("A device with nothing set here follows the general settings"),
    );
    let chosen = Rc::new(Cell::new(0usize));
    let device = {
        let devices = Rc::downgrade(&devices);
        let chosen = chosen.clone();
        move || {
            devices
                .upgrade()
                .and_then(|devices| devices.mice().get(chosen.get()).cloned())
                .unwrap_or_default()
        }
    };
    let device = Rc::new(device);
    let picker = page.combo(&device_group, "mouse");
    let (device_speed, _) = slider_row(&page.theme, &device_group, "speed", &tr("Speed"), SPEED);
    device_speed.on_moved({
        let devices = Rc::downgrade(&devices);
        let device = device.clone();
        let tip = Rc::downgrade(&device_speed);
        move |value| {
            if let Some(slider) = tip.upgrade() {
                slider.set_tooltip(&format!("{}", value.round()));
            }
            if let Some(devices) = devices.upgrade() {
                devices.set(&device(), "sensitivity", &format!("{}", value / 100.0));
            }
        }
    });
    page.keep(device_speed.clone());
    let device_switch = |icon: &str, label: &str, key: &'static str| {
        let current = {
            let devices = Rc::downgrade(&devices);
            let options = Rc::downgrade(&options);
            let device = device.clone();
            move || {
                let (Some(devices), Some(options)) = (devices.upgrade(), options.upgrade()) else {
                    return false;
                };
                let own = devices.value_of(&device(), key);
                match key {
                    "enabled" => own != "false",
                    "accel_profile" => {
                        let profile = if own.is_empty() {
                            options.text("input:accel_profile")
                        } else {
                            own
                        };
                        profile != "flat"
                    }
                    _ => {
                        let natural = if own.is_empty() {
                            options.flag("input:natural_scroll").to_string()
                        } else {
                            own
                        };
                        natural == "true"
                    }
                }
            }
        };
        let current = Rc::new(current);
        let switch = page.switch(&device_group, icon, label, {
            let devices = Rc::downgrade(&devices);
            let device = device.clone();
            let current = current.clone();
            move |wanted| {
                if current() == wanted {
                    return;
                }
                let value = match key {
                    "accel_profile" if wanted => "adaptive".to_owned(),
                    "accel_profile" => "flat".to_owned(),
                    _ => wanted.to_string(),
                };
                if let Some(devices) = devices.upgrade() {
                    devices.set(&device(), key, &value);
                }
            }
        });
        switch.bind(move || current());
        switch
    };
    let device_enabled = device_switch("power_settings_new", &tr("Enabled"), "enabled");
    page.tip(
        &device_enabled.button,
        &tr("A disabled device stops moving the pointer until it is turned back on here."),
    );
    let device_acceleration =
        device_switch("trending_up", &tr("Mouse acceleration"), "accel_profile");
    let device_natural = device_switch("swap_vert", &tr("Natural scrolling"), "natural_scroll");
    let (reset, _) = page.icon_button(
        "settings_backup_restore",
        true,
        &tr("Follow the general settings"),
        {
            let devices = Rc::downgrade(&devices);
            let device = device.clone();
            move || {
                if let Some(devices) = devices.upgrade() {
                    let device = device();
                    for key in DEVICE_KEYS {
                        devices.unset(&device, key);
                    }
                }
            }
        },
    );
    device_group.append(&reset);
    let show_device = Rc::new({
        let devices = Rc::downgrade(&devices);
        let options = Rc::downgrade(&options);
        let picker = Rc::downgrade(&picker);
        let device_speed = Rc::downgrade(&device_speed);
        let chosen = chosen.clone();
        let device = device.clone();
        let device_group = Page::subsection_root(&device_group);
        move || {
            let (Some(devices), Some(options), Some(picker), Some(slider)) = (
                devices.upgrade(),
                options.upgrade(),
                picker.upgrade(),
                device_speed.upgrade(),
            ) else {
                return;
            };
            let mice = devices.mice();
            empty.set_visible(mice.is_empty());
            device_group.set_visible(!mice.is_empty());
            picker.set_items(&mice, chosen.get() as i32);
            let device = device();
            let fallback = options.number("input:sensitivity");
            show_speed(
                &slider,
                (device_number(&devices, &device, "sensitivity", fallback) * 100.0).round(),
            );
            device_enabled.refresh();
            device_acceleration.refresh();
            device_natural.refresh();
            reset.set_sensitive(devices.overrides(&device) > 0);
        }
    });
    show_device();
    page.keep(devices.watch({
        let show_device = show_device.clone();
        move || show_device()
    }));
    options.connect_changed({
        let show_device = show_device.clone();
        move || show_device()
    });
    picker.connect_activated(move |index| {
        chosen.set(index);
        show_device();
    });

    page.keep(options);
    page.keep(devices);
    page
}

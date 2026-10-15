use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::inputs::slider_row;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::input::mouse::{OPTIONS, SPEED, show_speed};
use crate::platform::hyprland::hyprconfig::Area;
use crate::services::hardware::deviceoptions::DeviceOptions;
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;

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

fn selected_or_first_mouse(mice: &[String], selected_name: &str) -> String {
    if mice.iter().any(|mouse| mouse == selected_name) {
        return selected_name.to_owned();
    }
    mice.first().cloned().unwrap_or_default()
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Mouse, &OPTIONS);
    let devices = DeviceOptions::with_mice();

    let device_section = page.section("", "");
    let empty_note = text::styled(&tr("No pointing device is connected"));
    text::set_color(&empty_note, "colSubtext");
    let empty_note = Centered::new(&empty_note);
    empty_note.set_halign(gtk4::Align::Start);
    empty_note.set_margin_start(EMPTY_START);
    device_section.append(&empty_note);
    let device_group = page.subsection(
        &device_section,
        &tr("Device"),
        &tr("A device with nothing set here follows the general settings"),
    );
    let selected_mouse = Rc::new(RefCell::new(String::new()));
    let device = {
        let selected_mouse = selected_mouse.clone();
        move || selected_mouse.borrow().clone()
    };
    let device = Rc::new(device);
    let picker = page.combo(&device_group, "mouse");
    let (device_speed_slider, _) =
        slider_row(&page.theme, &device_group, "speed", &tr("Speed"), SPEED);
    device_speed_slider.on_moved({
        let devices = Rc::downgrade(&devices);
        let device = device.clone();
        let slider = Rc::downgrade(&device_speed_slider);
        move |value| {
            if let Some(slider) = slider.upgrade() {
                slider.set_tooltip(&format!("{}", value.round()));
            }
            if let Some(devices) = devices.upgrade() {
                devices.set(&device(), "sensitivity", &format!("{}", value / 100.0));
            }
        }
    });
    page.keep(device_speed_slider.clone());
    let device_switch = |icon: &str, label: &str, key: &'static str| {
        let is_device_option_on = {
            let devices = Rc::downgrade(&devices);
            let options = Rc::downgrade(&options);
            let device = device.clone();
            move || {
                let (Some(devices), Some(options)) = (devices.upgrade(), options.upgrade()) else {
                    return false;
                };
                let own_value = devices.value_of(&device(), key);
                match key {
                    "enabled" => own_value != "false",
                    "accel_profile" => {
                        let profile = if own_value.is_empty() {
                            options.text("input:accel_profile")
                        } else {
                            own_value
                        };
                        profile != "flat"
                    }
                    _ => {
                        let natural_scroll = if own_value.is_empty() {
                            options.flag("input:natural_scroll").to_string()
                        } else {
                            own_value
                        };
                        natural_scroll == "true"
                    }
                }
            }
        };
        let is_device_option_on = Rc::new(is_device_option_on);
        let switch = page.switch(&device_group, icon, label, {
            let devices = Rc::downgrade(&devices);
            let device = device.clone();
            let is_device_option_on = is_device_option_on.clone();
            move |is_on| {
                if is_device_option_on() == is_on {
                    return;
                }
                let value = match key {
                    "accel_profile" if is_on => "adaptive".to_owned(),
                    "accel_profile" => "flat".to_owned(),
                    _ => is_on.to_string(),
                };
                if let Some(devices) = devices.upgrade() {
                    devices.set(&device(), key, &value);
                }
            }
        });
        switch.bind(move || is_device_option_on());
        switch
    };
    let enabled_switch = device_switch("power_settings_new", &tr("Enabled"), "enabled");
    page.tip(
        &enabled_switch.button,
        &tr("A disabled device stops moving the pointer until it is turned back on here."),
    );
    let acceleration_switch =
        device_switch("trending_up", &tr("Mouse acceleration"), "accel_profile");
    let natural_switch = device_switch("swap_vert", &tr("Natural scrolling"), "natural_scroll");
    let (reset_button, _) = page.icon_button(
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
    device_group.append(&reset_button);
    let show_device = Rc::new({
        let devices = Rc::downgrade(&devices);
        let options = Rc::downgrade(&options);
        let picker = Rc::downgrade(&picker);
        let device_speed_slider = Rc::downgrade(&device_speed_slider);
        let selected_mouse = selected_mouse.clone();
        let device_group = Page::subsection_root(&device_group);
        move || {
            let (Some(devices), Some(options), Some(picker), Some(slider)) = (
                devices.upgrade(),
                options.upgrade(),
                picker.upgrade(),
                device_speed_slider.upgrade(),
            ) else {
                return;
            };
            let mice = devices.mice();
            empty_note.set_visible(devices.has_list() && mice.is_empty());
            device_group.set_visible(!mice.is_empty());
            let device = selected_or_first_mouse(&mice, &selected_mouse.borrow());
            selected_mouse.replace(device.clone());
            picker.set_items_showing(&mice, &device);
            let fallback = options.number("input:sensitivity");
            show_speed(
                &slider,
                (device_number(&devices, &device, "sensitivity", fallback) * 100.0).round(),
            );
            enabled_switch.refresh();
            acceleration_switch.refresh();
            natural_switch.refresh();
            reset_button.set_sensitive(devices.override_count(&device, &DEVICE_KEYS) > 0);
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
    picker.connect_activated({
        let picker = Rc::downgrade(&picker);
        move |index| {
            if let Some(mouse) = picker.upgrade().and_then(|picker| picker.item(index)) {
                selected_mouse.replace(mouse);
            }
            show_device();
        }
    });

    page.keep(options);
    page.keep(devices);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selected_mouse_is_found_by_name_after_the_list_changes() {
        let mice =
            |names: &[&str]| -> Vec<String> { names.iter().map(|name| name.to_string()).collect() };
        assert_eq!(selected_or_first_mouse(&mice(&["a", "c", "b"]), "b"), "b");
        assert_eq!(selected_or_first_mouse(&mice(&["b"]), "b"), "b");
        assert_eq!(selected_or_first_mouse(&mice(&["a", "c"]), "b"), "a");
        assert_eq!(selected_or_first_mouse(&mice(&["a", "c"]), ""), "a");
        assert_eq!(selected_or_first_mouse(&[], "b"), "");
    }
}

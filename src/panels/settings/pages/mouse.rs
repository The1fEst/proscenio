use gtk4::prelude::*;
use serde_json::Value;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, slider_row};
use crate::panels::settings::gestures;
use crate::panels::settings::hyprrows::{self, Spin};
use crate::services::deviceoptions::DeviceOptions;
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::slider::Slider;
use crate::ui::widgets::text;

const OPTIONS: [&str; 18] = [
    "input:left_handed",
    "input:sensitivity",
    "input:accel_profile",
    "input:natural_scroll",
    "input:scroll_method",
    "input:scroll_factor",
    "cursor:inactive_timeout",
    "cursor:hide_on_key_press",
    "cursor:no_hardware_cursors",
    "cursor:enable_hyprcursor",
    "input:touchpad:disable_while_typing",
    "input:touchpad:tap_to_click",
    "input:touchpad:tap_and_drag",
    "input:touchpad:middle_button_emulation",
    "input:touchpad:clickfinger_behavior",
    "input:touchpad:tap_button_map",
    "input:touchpad:natural_scroll",
    "input:touchpad:scroll_factor",
];
const SCROLL_METHODS: [(&str, &str); 4] = [
    ("Two fingers or wheel", "2fg"),
    ("Along the edge", "edge"),
    ("While a button is held", "on_button_down"),
    ("No scrolling", "no_scroll"),
];
const HARDWARE_CURSORS: [(&str, &str); 3] = [
    ("The screen draws the pointer, except while tearing", "2"),
    ("The screen always draws the pointer", "0"),
    ("The pointer is drawn with the rest of the screen", "1"),
];
const SPEED: (f64, f64) = (-100.0, 100.0);
const EMPTY_START: i32 = 8;
const DEVICE_KEYS: [&str; 3] = ["sensitivity", "accel_profile", "natural_scroll"];

fn choice(label: &str, icon: &'static str, value: Value) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value,
    }
}

fn spin(
    icon: &'static str,
    label: &'static str,
    option: &'static str,
    factor: f64,
    range: (i64, i64),
    step: i64,
) -> Spin {
    Spin {
        icon,
        label,
        option,
        factor,
        range,
        step,
        decimals: 0,
    }
}

fn show_speed(slider: &Slider, value: f64) {
    slider.set(value);
    slider.set_tooltip(&format!("{}", value.round()));
}

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
    let options = HyprOptions::new(&OPTIONS);
    let devices = DeviceOptions::new();

    let general = page.section("mouse", &tr("General"));
    let primary = page.subsection(
        &general,
        &tr("Primary button"),
        &tr("Order of the physical buttons on mice and touchpads"),
    );
    hyprrows::selection(
        &page,
        &primary,
        &options,
        vec![
            choice(&tr("Left"), "arrow_back", Value::Bool(false)),
            choice(&tr("Right"), "arrow_forward", Value::Bool(true)),
        ],
        |options| Value::Bool(options.flag("input:left_handed")),
        |options, value| options.set("input:left_handed", &value.to_string()),
    );

    let mouse = page.section("mouse", &tr("Mouse"));
    let speed_group = page.subsection(&mouse, &tr("Pointer speed"), "");
    let (speed, _) = slider_row(&page.theme, &speed_group, "speed", &tr("Speed"), SPEED);
    let show_general_speed = {
        let speed = Rc::downgrade(&speed);
        let options = Rc::downgrade(&options);
        move || {
            if let (Some(speed), Some(options)) = (speed.upgrade(), options.upgrade()) {
                show_speed(
                    &speed,
                    (options.number("input:sensitivity") * 100.0).round(),
                );
            }
        }
    };
    show_general_speed();
    options.connect_changed(show_general_speed);
    speed.on_moved({
        let options = Rc::downgrade(&options);
        let tip = Rc::downgrade(&speed);
        move |value| {
            if let Some(slider) = tip.upgrade() {
                slider.set_tooltip(&format!("{}", value.round()));
            }
            if let Some(options) = options.upgrade() {
                options.set("input:sensitivity", &format!("{}", value / 100.0));
            }
        }
    });
    page.keep(speed);
    let acceleration = hyprrows::option_switch(
        &page,
        &mouse,
        &options,
        ("trending_up", tr("Mouse acceleration")),
        |options| options.text("input:accel_profile") != "flat",
        |options, accelerated| {
            options.set(
                "input:accel_profile",
                if accelerated { "adaptive" } else { "flat" },
            )
        },
    );
    page.tip(
        &acceleration.button,
        &tr("Off moves the pointer exactly as far as the mouse moved, which games and drawing want.\nOn speeds the pointer up as the mouse moves faster."),
    );
    let scrolling = page.subsection(&mouse, &tr("Scrolling"), "");
    let natural = hyprrows::switch(
        &page,
        &scrolling,
        &options,
        "swap_vert",
        &tr("Natural scrolling"),
        "input:natural_scroll",
    );
    page.tip(
        &natural.button,
        &tr("Scrolling moves the content rather than the view."),
    );
    hyprrows::combo(
        &page,
        &scrolling,
        &options,
        "mouse",
        ("input:scroll_method", "2fg"),
        &SCROLL_METHODS,
    );
    hyprrows::spin(
        &page,
        &scrolling,
        &options,
        &spin(
            "height",
            "Scroll amount (%)",
            "input:scroll_factor",
            100.0,
            (10, 500),
            10,
        ),
    );

    let only = page.section("usb", &tr("This mouse only"));
    let empty = text::styled(&tr("No pointing device is connected"));
    text::set_color(&empty, "colSubtext");
    let empty = Centred::new(&empty);
    empty.set_halign(gtk4::Align::Start);
    empty.set_margin_start(EMPTY_START);
    only.append(&empty);
    let device_group = page.subsection(
        &only,
        &tr("Device"),
        &tr("A device with nothing set here follows the general settings above"),
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

    let pointer = page.section("highlight_mouse_cursor", &tr("Pointer"));
    let hiding = page.subsection(&pointer, &tr("Hiding"), "");
    let (still, _) = hyprrows::spin(
        &page,
        &hiding,
        &options,
        &spin(
            "timer",
            "Hide when still for (s)",
            "cursor:inactive_timeout",
            1.0,
            (0, 120),
            1,
        ),
    );
    page.tip(
        &still,
        &tr("Zero keeps the pointer on screen no matter how long it sits still."),
    );
    hyprrows::switch(
        &page,
        &hiding,
        &options,
        "keyboard_hide",
        &tr("Hide while typing"),
        "cursor:hide_on_key_press",
    );
    let drawing = page.subsection(&pointer, &tr("Drawing"), "");
    let hardware = hyprrows::combo(
        &page,
        &drawing,
        &options,
        "memory",
        ("cursor:no_hardware_cursors", "2"),
        &HARDWARE_CURSORS,
    );
    page.tip(
        &hardware.button,
        &tr("A pointer the screen draws itself stays smooth whatever the rest of the screen is doing.\nPick the last choice if the pointer disappears or is drawn in the wrong place."),
    );
    hyprrows::switch(
        &page,
        &drawing,
        &options,
        "animated_images",
        &tr("Use hyprcursor themes"),
        "cursor:enable_hyprcursor",
    );

    let touchpad = page.section("touch_app", &tr("Touchpad"));
    hyprrows::switch(
        &page,
        &touchpad,
        &options,
        "keyboard",
        &tr("Disable while typing"),
        "input:touchpad:disable_while_typing",
    );
    let clicking = page.subsection(&touchpad, &tr("Clicking"), "");
    let tap = hyprrows::switch(
        &page,
        &clicking,
        &options,
        "touch_app",
        &tr("Tap to click"),
        "input:touchpad:tap_to_click",
    );
    page.tip(&tap.button, &tr("Quickly touch the touchpad to click."));
    hyprrows::switch(
        &page,
        &clicking,
        &options,
        "drag_pan",
        &tr("Tap and drag"),
        "input:touchpad:tap_and_drag",
    );
    hyprrows::switch(
        &page,
        &clicking,
        &options,
        "pan_tool",
        &tr("Middle click with three fingers"),
        "input:touchpad:middle_button_emulation",
    );
    let secondary = page.subsection(&clicking, &tr("Secondary click"), "");
    hyprrows::selection(
        &page,
        &secondary,
        &options,
        vec![
            choice(&tr("Corner push"), "south_west", Value::Bool(false)),
            choice(&tr("Two finger push"), "touch_app", Value::Bool(true)),
        ],
        |options| Value::Bool(options.flag("input:touchpad:clickfinger_behavior")),
        |options, value| options.set("input:touchpad:clickfinger_behavior", &value.to_string()),
    );
    let taps = page.subsection(&clicking, &tr("Tap with two or three fingers"), "");
    hyprrows::selection(
        &page,
        &taps,
        &options,
        vec![
            choice(
                &tr("Right, then middle"),
                "arrow_forward",
                Value::from("lrm"),
            ),
            choice(
                &tr("Middle, then right"),
                "arrow_upward",
                Value::from("lmr"),
            ),
        ],
        |options| {
            let map = options.text("input:touchpad:tap_button_map");
            Value::from(if map.is_empty() {
                "lrm".to_owned()
            } else {
                map
            })
        },
        |options, value| {
            if let Some(map) = value.as_str() {
                options.set("input:touchpad:tap_button_map", map);
            }
        },
    );
    let touch_scrolling = page.subsection(&touchpad, &tr("Scrolling"), "");
    hyprrows::switch(
        &page,
        &touch_scrolling,
        &options,
        "swap_vert",
        &tr("Natural scrolling"),
        "input:touchpad:natural_scroll",
    );
    hyprrows::spin(
        &page,
        &touch_scrolling,
        &options,
        &spin(
            "height",
            "Scroll amount (%)",
            "input:touchpad:scroll_factor",
            100.0,
            (10, 500),
            10,
        ),
    );
    gestures::section(&page, &touchpad);

    page.keep(options);
    page.keep(devices);
    page
}

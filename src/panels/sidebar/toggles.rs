use gtk4::glib;
use std::rc::Rc;
use std::time::Duration;

use crate::core::actions;
use crate::core::i18n::{tr, trf};
use crate::core::process;
use crate::core::scope::Scope;
use crate::panels::bar::bluetooth;
use crate::panels::sidebar::quicktoggle::Look;
use crate::services::Services;

const ACTION_DELAY: Duration = Duration::from_millis(300);

pub const ALL_KINDS: [&str; 16] = [
    "network",
    "ethernet",
    "bluetooth",
    "idleInhibitor",
    "easyEffects",
    "nightLight",
    "darkMode",
    "cloudflareWarp",
    "wireGuard",
    "screenSnip",
    "colorPicker",
    "onScreenKeyboard",
    "mic",
    "audio",
    "notifications",
    "powerProfile",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    Wifi,
    Bluetooth,
    NightLight,
    WireGuard,
}

pub fn kind(name: &str) -> Option<&'static str> {
    ALL_KINDS.iter().find(|kind| **kind == name).copied()
}

pub fn menu(kind: &str) -> Option<Menu> {
    match kind {
        "network" => Some(Menu::Wifi),
        "bluetooth" => Some(Menu::Bluetooth),
        "nightLight" => Some(Menu::NightLight),
        "wireGuard" => Some(Menu::WireGuard),
        _ => None,
    }
}

pub fn look(kind: &str, services: &Rc<Services>) -> Look {
    match kind {
        "network" => {
            let is_enabled = *services.net.wifi_status.borrow() != "disabled";
            let ssid = services.net.ssid.borrow().clone();
            let status = match (is_enabled, ssid.is_empty()) {
                (false, _) => tr("Off"),
                (true, true) => tr("Disconnected"),
                (true, false) => ssid,
            };
            Look {
                name: "Wi-Fi",
                status: status.clone(),
                has_status: true,
                icon: services.net.wifi_symbol.borrow().clone(),
                tooltip: trf("%1 | Right-click to configure", &[&status]),
                is_toggled: is_enabled,
                is_available: true,
            }
        }
        "ethernet" => {
            let ethernet = services.net.wired.borrow().clone();
            let status = if ethernet.is_connected {
                ethernet.connection
            } else {
                tr("Disconnected")
            };
            Look {
                name: "Ethernet",
                status: status.clone(),
                has_status: true,
                icon: "lan".to_owned(),
                tooltip: trf("%1 | Right-click to configure", &[&status]),
                is_toggled: ethernet.is_connected,
                is_available: !ethernet.device.is_empty(),
            }
        }
        "bluetooth" => {
            let device_name = services.bluez.connected_name.borrow().clone();
            Look {
                name: "Bluetooth",
                status: device_name.clone().unwrap_or_else(|| tr("Not connected")),
                has_status: true,
                icon: bluetooth::symbol(&services.bluez).to_owned(),
                tooltip: trf(
                    "%1 | Right-click to configure",
                    &[&device_name.unwrap_or_else(|| tr("Bluetooth"))],
                ),
                is_toggled: services.bluez.is_powered.get(),
                is_available: services.bluez.is_available.get(),
            }
        }
        "wireGuard" => Look {
            name: "WireGuard",
            status: String::new(),
            has_status: true,
            icon: "vpn_key".to_owned(),
            tooltip: tr("WireGuard"),
            is_toggled: services.net.is_wireguard_up.get(),
            is_available: true,
        },
        "audio" => {
            let is_muted = services
                .audio
                .as_ref()
                .is_some_and(|audio| audio.is_sink_muted.get());
            Look {
                name: "Audio output",
                status: tr(if is_muted { "Muted" } else { "Unmuted" }),
                has_status: true,
                icon: if is_muted { "volume_off" } else { "volume_up" }.to_owned(),
                tooltip: tr("Audio output | Right-click for volume mixer & device selector"),
                is_toggled: !is_muted,
                is_available: true,
            }
        }
        "mic" => {
            let is_muted = services
                .audio
                .as_ref()
                .is_some_and(|audio| audio.is_source_muted.get());
            Look {
                name: "Audio input",
                status: tr(if is_muted { "Muted" } else { "Enabled" }),
                has_status: true,
                icon: if is_muted { "mic_off" } else { "mic" }.to_owned(),
                tooltip: tr("Audio input | Right-click for volume mixer & device selector"),
                is_toggled: !is_muted,
                is_available: true,
            }
        }
        "notifications" => {
            let is_shown = !services.notifications.is_silent.get();
            Look {
                name: "Notifications",
                status: tr(if is_shown { "Show" } else { "Silent" }),
                has_status: true,
                icon: if is_shown {
                    "notifications_active"
                } else {
                    "notifications_paused"
                }
                .to_owned(),
                tooltip: tr("Show notifications"),
                is_toggled: is_shown,
                is_available: true,
            }
        }
        "screenSnip" => Look {
            name: "Screen snip",
            status: String::new(),
            has_status: false,
            icon: "screenshot_region".to_owned(),
            tooltip: tr("Screen snip"),
            is_toggled: false,
            is_available: true,
        },
        "colorPicker" => Look {
            name: "Color picker",
            status: String::new(),
            has_status: false,
            icon: "colorize".to_owned(),
            tooltip: tr("Color picker"),
            is_toggled: false,
            is_available: true,
        },
        "powerProfile" => {
            let profile = services.power.profile.borrow().clone();
            Look {
                name: "Power Profile",
                status: tr(match profile.as_str() {
                    "power-saver" => "Power Saver",
                    "performance" => "Performance",
                    _ => "Balanced",
                }),
                has_status: true,
                icon: match profile.as_str() {
                    "power-saver" => "energy_savings_leaf",
                    "performance" => "local_fire_department",
                    _ => "airwave",
                }
                .to_owned(),
                tooltip: tr("Click to cycle through power profiles"),
                is_toggled: profile != "balanced",
                is_available: true,
            }
        }
        "easyEffects" => Look {
            name: "EasyEffects",
            status: String::new(),
            has_status: true,
            icon: "graphic_eq".to_owned(),
            tooltip: tr("EasyEffects | Right-click to configure"),
            is_toggled: services.easyeffects.is_active.get(),
            is_available: services.easyeffects.is_available.get(),
        },
        "cloudflareWarp" => Look {
            name: "Cloudflare WARP",
            status: String::new(),
            has_status: true,
            icon: "cloud_lock".to_owned(),
            tooltip: tr("Cloudflare WARP (1.1.1.1)"),
            is_toggled: services.warp.is_connected.get(),
            is_available: services.warp.is_available.get(),
        },
        "onScreenKeyboard" => Look {
            name: "Virtual Keyboard",
            status: String::new(),
            has_status: true,
            icon: if services.states.is_osk_open.get() {
                "keyboard_hide"
            } else {
                "keyboard"
            }
            .to_owned(),
            tooltip: tr("On-screen keyboard"),
            is_toggled: services.states.is_osk_open.get(),
            is_available: true,
        },
        "idleInhibitor" => Look {
            name: "Keep awake",
            status: String::new(),
            has_status: true,
            icon: "coffee".to_owned(),
            tooltip: tr("Keep system awake"),
            is_toggled: services.session.keeps_awake.get(),
            is_available: true,
        },
        "nightLight" => {
            let is_automatic = services.session.is_night_light_automatic.get();
            let is_on = services.session.is_night_light_on.get();
            Look {
                name: "Night Light",
                status: format!(
                    "{}{}",
                    if is_automatic {
                        tr("Auto, ")
                    } else {
                        String::new()
                    },
                    tr(if is_on { "Active" } else { "Inactive" })
                ),
                has_status: true,
                icon: if is_automatic {
                    "night_sight_auto"
                } else {
                    "bedtime"
                }
                .to_owned(),
                tooltip: tr("Night Light | Right-click to configure"),
                is_toggled: is_on,
                is_available: true,
            }
        }
        _ => Look {
            name: "Dark Mode",
            status: tr(if services.session.is_dark_mode.get() {
                "Dark"
            } else {
                "Light"
            }),
            has_status: true,
            icon: "contrast".to_owned(),
            tooltip: tr("Dark Mode"),
            is_toggled: services.session.is_dark_mode.get(),
            is_available: true,
        },
    }
}

pub fn act(kind: &str, services: &Rc<Services>, close: &Rc<dyn Fn()>) {
    match kind {
        "network" => services.net.toggle_wifi(),
        "ethernet" => services.net.toggle_wired(),
        "bluetooth" => services.bluez.toggle(),
        "wireGuard" => services.net.toggle_wireguard(),
        "audio" => {
            if let Some(audio) = &services.audio {
                audio.toggle_sink_mute();
            }
        }
        "mic" => {
            if let Some(audio) = &services.audio {
                audio.toggle_source_mute();
            }
        }
        "notifications" => {
            let is_silent = services.notifications.is_silent.get();
            services.notifications.set_silent(!is_silent);
        }
        "powerProfile" => services.power.cycle(),
        "easyEffects" => services.easyeffects.toggle(),
        "cloudflareWarp" => services.warp.toggle(),
        "idleInhibitor" => services.session.toggle_keep_awake(),
        "nightLight" => services.session.toggle_night_light(),
        "darkMode" => services.session.toggle_dark_mode(),
        "onScreenKeyboard" => actions::run("oskToggle"),
        "screenSnip" => {
            close();
            glib::timeout_add_local_once(ACTION_DELAY, || actions::run("regionScreenshot"));
        }
        "colorPicker" => {
            close();
            glib::timeout_add_local_once(ACTION_DELAY, || {
                process::start(process::quiet(&["hyprpicker", "-a"]));
            });
        }
        _ => {}
    }
}

pub fn subscribe(services: &Rc<Services>, scope: &Scope, redraw: impl Fn() + Clone + 'static) {
    scope.keep(services.net.subscribe(redraw.clone()));
    scope.keep(services.bluez.subscribe(redraw.clone()));
    scope.keep(services.power.subscribe(redraw.clone()));
    scope.keep(services.easyeffects.subscribe(redraw.clone()));
    scope.keep(services.warp.subscribe(redraw.clone()));
    scope.keep(services.session.subscribe(redraw.clone()));
    scope.keep(services.notifications.subscribe(redraw.clone()));
    scope.keep(services.states.subscribe(redraw.clone()));
    if let Some(audio) = &services.audio {
        scope.keep(audio.subscribe(redraw));
    }
}

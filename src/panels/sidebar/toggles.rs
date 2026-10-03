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

const DELAYED: Duration = Duration::from_millis(300);

pub const AVAILABLE: [&str; 16] = [
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
    AVAILABLE.iter().find(|kind| **kind == name).copied()
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
            let enabled = *services.net.wifi_status.borrow() != "disabled";
            let ssid = services.net.ssid.borrow().clone();
            let status = match (enabled, ssid.is_empty()) {
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
                toggled: enabled,
                available: true,
            }
        }
        "ethernet" => {
            let wired = services.net.wired.borrow().clone();
            let status = if wired.connected {
                wired.connection
            } else {
                tr("Disconnected")
            };
            Look {
                name: "Ethernet",
                status: status.clone(),
                has_status: true,
                icon: "lan".to_owned(),
                tooltip: trf("%1 | Right-click to configure", &[&status]),
                toggled: wired.connected,
                available: !wired.device.is_empty(),
            }
        }
        "bluetooth" => {
            let connected = services.bluez.connected_name.borrow().clone();
            Look {
                name: "Bluetooth",
                status: connected.clone().unwrap_or_else(|| tr("Not connected")),
                has_status: true,
                icon: bluetooth::symbol(&services.bluez).to_owned(),
                tooltip: trf(
                    "%1 | Right-click to configure",
                    &[&connected.unwrap_or_else(|| tr("Bluetooth"))],
                ),
                toggled: services.bluez.powered.get(),
                available: services.bluez.available.get(),
            }
        }
        "wireGuard" => Look {
            name: "WireGuard",
            status: String::new(),
            has_status: true,
            icon: "vpn_key".to_owned(),
            tooltip: tr("WireGuard"),
            toggled: services.net.wireguard.get(),
            available: true,
        },
        "audio" => {
            let muted = services
                .audio
                .as_ref()
                .is_some_and(|audio| audio.sink_muted.get());
            Look {
                name: "Audio output",
                status: tr(if muted { "Muted" } else { "Unmuted" }),
                has_status: true,
                icon: if muted { "volume_off" } else { "volume_up" }.to_owned(),
                tooltip: tr("Audio output | Right-click for volume mixer & device selector"),
                toggled: !muted,
                available: true,
            }
        }
        "mic" => {
            let muted = services
                .audio
                .as_ref()
                .is_some_and(|audio| audio.source_muted.get());
            Look {
                name: "Audio input",
                status: tr(if muted { "Muted" } else { "Enabled" }),
                has_status: true,
                icon: if muted { "mic_off" } else { "mic" }.to_owned(),
                tooltip: tr("Audio input | Right-click for volume mixer & device selector"),
                toggled: !muted,
                available: true,
            }
        }
        "notifications" => {
            let shown = !services.notifications.silent.get();
            Look {
                name: "Notifications",
                status: tr(if shown { "Show" } else { "Silent" }),
                has_status: true,
                icon: if shown {
                    "notifications_active"
                } else {
                    "notifications_paused"
                }
                .to_owned(),
                tooltip: tr("Show notifications"),
                toggled: shown,
                available: true,
            }
        }
        "screenSnip" => Look {
            name: "Screen snip",
            status: String::new(),
            has_status: false,
            icon: "screenshot_region".to_owned(),
            tooltip: tr("Screen snip"),
            toggled: false,
            available: true,
        },
        "colorPicker" => Look {
            name: "Color picker",
            status: String::new(),
            has_status: false,
            icon: "colorize".to_owned(),
            tooltip: tr("Color picker"),
            toggled: false,
            available: true,
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
                toggled: profile != "balanced",
                available: true,
            }
        }
        "easyEffects" => Look {
            name: "EasyEffects",
            status: String::new(),
            has_status: true,
            icon: "graphic_eq".to_owned(),
            tooltip: tr("EasyEffects | Right-click to configure"),
            toggled: services.easyeffects.active.get(),
            available: services.easyeffects.available.get(),
        },
        "cloudflareWarp" => Look {
            name: "Cloudflare WARP",
            status: String::new(),
            has_status: true,
            icon: "cloud_lock".to_owned(),
            tooltip: tr("Cloudflare WARP (1.1.1.1)"),
            toggled: services.warp.connected.get(),
            available: services.warp.available.get(),
        },
        "onScreenKeyboard" => Look {
            name: "Virtual Keyboard",
            status: String::new(),
            has_status: true,
            icon: if services.states.osk_open.get() {
                "keyboard_hide"
            } else {
                "keyboard"
            }
            .to_owned(),
            tooltip: tr("On-screen keyboard"),
            toggled: services.states.osk_open.get(),
            available: true,
        },
        "idleInhibitor" => Look {
            name: "Keep awake",
            status: String::new(),
            has_status: true,
            icon: "coffee".to_owned(),
            tooltip: tr("Keep system awake"),
            toggled: services.session.awake.get(),
            available: true,
        },
        "nightLight" => {
            let auto = services.session.automatic.get();
            let on = services.session.night.get();
            Look {
                name: "Night Light",
                status: format!(
                    "{}{}",
                    if auto { tr("Auto, ") } else { String::new() },
                    tr(if on { "Active" } else { "Inactive" })
                ),
                has_status: true,
                icon: if auto { "night_sight_auto" } else { "bedtime" }.to_owned(),
                tooltip: tr("Night Light | Right-click to configure"),
                toggled: on,
                available: true,
            }
        }
        _ => Look {
            name: "Dark Mode",
            status: tr(if services.session.dark.get() {
                "Dark"
            } else {
                "Light"
            }),
            has_status: true,
            icon: "contrast".to_owned(),
            tooltip: tr("Dark Mode"),
            toggled: services.session.dark.get(),
            available: true,
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
            let silent = services.notifications.silent.get();
            services.notifications.set_silent(!silent);
        }
        "powerProfile" => services.power.cycle(),
        "easyEffects" => services.easyeffects.toggle(),
        "cloudflareWarp" => services.warp.toggle(),
        "idleInhibitor" => services.session.toggle_awake(),
        "nightLight" => services.session.toggle_night(),
        "darkMode" => services.session.toggle_dark(),
        "onScreenKeyboard" => actions::run("oskToggle"),
        "screenSnip" => {
            close();
            glib::timeout_add_local_once(DELAYED, || actions::run("regionScreenshot"));
        }
        "colorPicker" => {
            close();
            glib::timeout_add_local_once(DELAYED, || {
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

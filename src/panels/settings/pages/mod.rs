pub mod about;
pub mod accessibility;
pub mod advanced;
pub mod appearance;
pub mod apps;
pub mod autostart;
pub mod background;
pub mod bar;
pub mod bluetooth;
pub mod capture;
pub mod connection;
pub mod datetime;
pub mod devices;
pub mod displays;
pub mod filetypes;
pub mod firewall;
pub mod hiddennetwork;
pub mod hotspot;
pub mod keyboard;
pub mod lock;
pub mod mouse;
pub mod multitasking;
pub mod network;
pub mod notifications;
pub mod panels;
pub mod power;
pub mod privacy;
pub mod proxy;
pub mod quick;
pub mod region;
pub mod savednetworks;
pub mod search;
pub mod services;
pub mod shortcuts;
pub mod sound;
pub mod system;
pub mod users;
pub mod wifi;

use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Context, Page};
use crate::platform::hyprconfig::{self, Area};
use crate::ui::widgets::text;

pub struct Entry {
    pub name: &'static str,
    pub icon: &'static str,
    pub id: &'static str,
    pub starts_group: bool,
    pub keywords: &'static [&'static str],
}

pub const PAGES: &[Entry] = &[
    Entry {
        name: "Quick",
        icon: "instant_mix",
        id: "quick",
        starts_group: false,
        keywords: &["quick", "common", "frequent"],
    },
    Entry {
        name: "Wi-Fi",
        icon: "wifi",
        id: "wifi",
        starts_group: true,
        keywords: &[
            "wifi",
            "wireless",
            "network",
            "internet",
            "connection",
            "password",
        ],
    },
    Entry {
        name: "Network",
        icon: "lan",
        id: "network",
        starts_group: false,
        keywords: &[
            "network",
            "ethernet",
            "wired",
            "vpn",
            "wireguard",
            "connection",
            "proxy",
        ],
    },
    Entry {
        name: "Bluetooth",
        icon: "bluetooth",
        id: "bluetooth",
        starts_group: false,
        keywords: &[
            "bluetooth",
            "pair",
            "device",
            "headset",
            "mouse",
            "keyboard",
            "wireless",
        ],
    },
    Entry {
        name: "Displays",
        icon: "monitor",
        id: "displays",
        starts_group: true,
        keywords: &[
            "screen",
            "resolution",
            "refresh",
            "monitor",
            "night",
            "light",
            "hdr",
            "scale",
            "arrange",
            "vrr",
        ],
    },
    Entry {
        name: "Sound",
        icon: "volume_up",
        id: "sound",
        starts_group: false,
        keywords: &[
            "card",
            "microphone",
            "volume",
            "balance",
            "headset",
            "audio",
            "output",
            "input",
        ],
    },
    Entry {
        name: "Power",
        icon: "battery_android_full",
        id: "power",
        starts_group: false,
        keywords: &[
            "power", "sleep", "suspend", "battery", "blank", "idle", "dpms", "energy", "lock",
        ],
    },
    Entry {
        name: "Multitasking",
        icon: "select_window_2",
        id: "multitasking",
        starts_group: false,
        keywords: &[
            "workspace",
            "tiling",
            "layout",
            "gaps",
            "snap",
            "gesture",
            "swipe",
            "overview",
            "window",
        ],
    },
    Entry {
        name: "Appearance",
        icon: "palette",
        id: "appearance",
        starts_group: false,
        keywords: &[
            "style",
            "light",
            "dark",
            "theme",
            "colour",
            "color",
            "font",
            "rounding",
            "blur",
            "opacity",
            "animation",
            "wallpaper",
            "background",
            "bar",
            "panel",
            "dock",
            "sidebar",
        ],
    },
    Entry {
        name: "Apps",
        icon: "apps",
        id: "apps",
        starts_group: true,
        keywords: &[
            "application",
            "default",
            "preferred",
            "terminal",
            "browser",
            "open",
            "with",
            "handler",
        ],
    },
    Entry {
        name: "Notifications",
        icon: "notifications",
        id: "notifications",
        starts_group: false,
        keywords: &[
            "notification",
            "banner",
            "message",
            "popup",
            "disturb",
            "osd",
        ],
    },
    Entry {
        name: "Search",
        icon: "search",
        id: "search",
        starts_group: false,
        keywords: &["search", "find", "launcher", "prefix", "results", "fuzzy"],
    },
    Entry {
        name: "Mouse & Touchpad",
        icon: "mouse",
        id: "mouse",
        starts_group: true,
        keywords: &[
            "trackpad",
            "touchpad",
            "pointer",
            "click",
            "tap",
            "button",
            "scroll",
            "cursor",
            "sensitivity",
        ],
    },
    Entry {
        name: "Keyboard",
        icon: "keyboard",
        id: "keyboard",
        starts_group: false,
        keywords: &[
            "layout",
            "input",
            "source",
            "xkb",
            "shortcut",
            "hotkey",
            "compose",
            "character",
            "repeat",
        ],
    },
    Entry {
        name: "Devices",
        icon: "devices_other",
        id: "devices",
        starts_group: false,
        keywords: &[
            "device",
            "input",
            "keyboard",
            "mouse",
            "touchpad",
            "tablet",
            "touchscreen",
            "disable",
            "main",
        ],
    },
    Entry {
        name: "Accessibility",
        icon: "accessibility_new",
        id: "accessibility",
        starts_group: true,
        keywords: &[
            "accessibility",
            "a11y",
            "zoom",
            "magnifier",
            "cursor",
            "size",
            "animation",
            "motion",
            "repeat",
            "contrast",
        ],
    },
    Entry {
        name: "Privacy & Security",
        icon: "lock",
        id: "privacy",
        starts_group: false,
        keywords: &[
            "privacy",
            "security",
            "lock",
            "screen",
            "screenshot",
            "recording",
            "camera",
            "microphone",
            "clipboard",
            "safety",
        ],
    },
    Entry {
        name: "System",
        icon: "settings_applications",
        id: "system",
        starts_group: false,
        keywords: &[
            "system",
            "about",
            "device",
            "information",
            "hostname",
            "memory",
            "processor",
            "version",
            "os",
            "language",
            "region",
            "time",
            "date",
            "user",
            "account",
            "services",
            "advanced",
        ],
    },
];

pub fn index_of(id: &str) -> Option<usize> {
    PAGES.iter().position(|page| page.id == id)
}

pub struct Subpage {
    pub id: &'static str,
    pub title: &'static str,
    pub parent: &'static str,
}

pub const SUBPAGES: [Subpage; 20] = [
    Subpage {
        id: "filetypes",
        title: "File types",
        parent: "apps",
    },
    Subpage {
        id: "shortcuts",
        title: "Keyboard Shortcuts",
        parent: "keyboard",
    },
    Subpage {
        id: "savednetworks",
        title: "Saved Networks",
        parent: "wifi",
    },
    Subpage {
        id: "connection",
        title: "Connection",
        parent: "network",
    },
    Subpage {
        id: "proxy",
        title: "Proxy",
        parent: "network",
    },
    Subpage {
        id: "firewall",
        title: "Firewall",
        parent: "network",
    },
    Subpage {
        id: "hiddennetwork",
        title: "Connect to Hidden Network…",
        parent: "wifi",
    },
    Subpage {
        id: "hotspot",
        title: "Hotspot",
        parent: "wifi",
    },
    Subpage {
        id: "lock",
        title: "Screen Lock",
        parent: "privacy",
    },
    Subpage {
        id: "capture",
        title: "Screenshots & Recording",
        parent: "privacy",
    },
    Subpage {
        id: "region",
        title: "Region & Language",
        parent: "system",
    },
    Subpage {
        id: "datetime",
        title: "Date & Time",
        parent: "system",
    },
    Subpage {
        id: "users",
        title: "Users",
        parent: "system",
    },
    Subpage {
        id: "autostart",
        title: "Autostart",
        parent: "system",
    },
    Subpage {
        id: "about",
        title: "About",
        parent: "system",
    },
    Subpage {
        id: "services",
        title: "Services",
        parent: "system",
    },
    Subpage {
        id: "advanced",
        title: "Advanced",
        parent: "system",
    },
    Subpage {
        id: "background",
        title: "Background",
        parent: "appearance",
    },
    Subpage {
        id: "bar",
        title: "Bar",
        parent: "appearance",
    },
    Subpage {
        id: "panels",
        title: "Panels",
        parent: "appearance",
    },
];

pub fn split_legacy_settings() -> std::io::Result<()> {
    let owners: [(Area, &[&str]); 6] = [
        (Area::Appearance, &appearance::OPTIONS),
        (Area::Displays, &displays::OPTIONS),
        (Area::Multitasking, &multitasking::OPTIONS),
        (Area::Keyboard, &keyboard::OPTIONS),
        (Area::Accessibility, &accessibility::OPTIONS),
        (Area::Mouse, &mouse::OPTIONS),
    ];
    hyprconfig::split_legacy(|option| {
        owners
            .iter()
            .find(|(_, names)| names.iter().any(|name| name.replace('-', "_") == option))
            .map(|(area, _)| *area)
    })
}

pub fn subpage(id: &str) -> Option<&'static Subpage> {
    SUBPAGES.iter().find(|subpage| subpage.id == id)
}

pub fn build(id: &str, subpage: Option<&str>, context: &Context) -> Rc<Page> {
    match id {
        "quick" => quick::build(context),
        "wifi" => wifi::build(context),
        "network" => network::build(context),
        "connection" => connection::build(context),
        "proxy" => proxy::build(context),
        "firewall" => firewall::build(context),
        "bluetooth" => bluetooth::build(context),
        "displays" => displays::build(context),
        "sound" => sound::build(context),
        "power" => power::build(context),
        "lock" => lock::build(context),
        "multitasking" => multitasking::build(context),
        "appearance" => appearance::build(context),
        "background" => background::build(context),
        "bar" => bar::build(context),
        "panels" => panels::build(context),
        "apps" => apps::build(context),
        "mouse" => mouse::build(context),
        "keyboard" => keyboard::build(context),
        "devices" => devices::build(context),
        "accessibility" => accessibility::build(context),
        "savednetworks" => savednetworks::build(context),
        "hiddennetwork" => hiddennetwork::build(context),
        "hotspot" => hotspot::build(context),
        "notifications" => notifications::build(context),
        "search" => search::build(context),
        "privacy" => privacy::build(context),
        "capture" => capture::build(context),
        "system" => system::build(context),
        "region" => region::build(context),
        "datetime" => datetime::build(context),
        "users" => users::build(context),
        "autostart" => autostart::build(context),
        "filetypes" => filetypes::build(context),
        "shortcuts" => shortcuts::build(context),
        "about" => about::build(context),
        "services" => services::build(context),
        "advanced" => advanced::build(context),
        _ => {
            let name = subpage
                .or_else(|| index_of(id).map(|index| PAGES[index].name))
                .unwrap_or(id);
            placeholder(name, context)
        }
    }
}

fn placeholder(name: &str, context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let section = page.section("", "");
    let label = text::styled(&trf("%1 is not ported yet", &[&tr(name)]));
    text::set_color(&label, "colSubtext");
    label.set_halign(gtk4::Align::Center);
    section.append(&label);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subpage_hangs_off_a_rail_page_and_is_not_one_itself() {
        for subpage in &SUBPAGES {
            assert!(index_of(subpage.parent).is_some(), "{}", subpage.id);
            assert!(index_of(subpage.id).is_none(), "{}", subpage.id);
        }
        assert_eq!(
            subpage("capture").map(|found| found.parent),
            Some("privacy")
        );
    }
}

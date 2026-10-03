pub mod about;
pub mod accessibility;
pub mod advanced;
pub mod appearance;
pub mod apps;
pub mod autostart;
pub mod background;
pub mod bar;
pub mod barworkspaces;
pub mod bluetooth;
pub mod capture;
pub mod cheatsheet;
pub mod colors;
pub mod connection;
pub mod datetime;
pub mod devices;
pub mod displaycolor;
pub mod displays;
pub mod dock;
pub mod eap;
pub mod filetypes;
pub mod firewall;
pub mod fonts;
pub mod hiddennetwork;
pub mod hotspot;
pub mod ipv4;
pub mod ipv6;
pub mod keyboard;
pub mod keyoptions;
pub mod lock;
pub mod mouse;
pub mod mousedevice;
pub mod multitasking;
pub mod network;
pub mod nightlight;
pub mod notifications;
pub mod overview;
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
pub mod sidebars;
pub mod sound;
pub mod swiping;
pub mod system;
pub mod touchpad;
pub mod users;
pub mod utilitybuttons;
pub mod wifi;
pub mod windowrules;
pub mod windows;

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

pub const SUBPAGES: [Subpage; 39] = [
    Subpage {
        id: "filetypes",
        title: "File types",
        parent: "apps",
    },
    Subpage {
        id: "windowrules",
        title: "Window rules",
        parent: "apps",
    },
    Subpage {
        id: "shortcuts",
        title: "Keyboard Shortcuts",
        parent: "keyboard",
    },
    Subpage {
        id: "keyoptions",
        title: "Keyboard options",
        parent: "keyboard",
    },
    Subpage {
        id: "displaycolor",
        title: "Color",
        parent: "displays",
    },
    Subpage {
        id: "nightlight",
        title: "Night light",
        parent: "displays",
    },
    Subpage {
        id: "overview",
        title: "Overview",
        parent: "multitasking",
    },
    Subpage {
        id: "swiping",
        title: "Swiping between workspaces",
        parent: "multitasking",
    },
    Subpage {
        id: "mousedevice",
        title: "This mouse only",
        parent: "mouse",
    },
    Subpage {
        id: "touchpad",
        title: "Touchpad",
        parent: "mouse",
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
        id: "ipv4",
        title: "IPv4",
        parent: "connection",
    },
    Subpage {
        id: "ipv6",
        title: "IPv6",
        parent: "connection",
    },
    Subpage {
        id: "eap",
        title: "Authentication",
        parent: "connection",
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
        id: "colors",
        title: "Colors",
        parent: "appearance",
    },
    Subpage {
        id: "fonts",
        title: "Fonts",
        parent: "appearance",
    },
    Subpage {
        id: "windows",
        title: "Windows",
        parent: "appearance",
    },
    Subpage {
        id: "bar",
        title: "Bar",
        parent: "appearance",
    },
    Subpage {
        id: "utilitybuttons",
        title: "Utility buttons",
        parent: "bar",
    },
    Subpage {
        id: "barworkspaces",
        title: "Workspaces",
        parent: "bar",
    },
    Subpage {
        id: "panels",
        title: "Panels",
        parent: "appearance",
    },
    Subpage {
        id: "dock",
        title: "Dock",
        parent: "panels",
    },
    Subpage {
        id: "sidebars",
        title: "Sidebars",
        parent: "panels",
    },
    Subpage {
        id: "cheatsheet",
        title: "Cheat sheet",
        parent: "panels",
    },
];

fn option_area(option: &str) -> Option<Area> {
    let owners: [(Area, &[&str]); 7] = [
        (Area::Appearance, &appearance::OPTIONS),
        (Area::Appearance, &windows::OPTIONS),
        (Area::Displays, &displays::OPTIONS),
        (Area::Multitasking, &multitasking::OPTIONS),
        (Area::Keyboard, &keyboard::OPTIONS),
        (Area::Accessibility, &accessibility::OPTIONS),
        (Area::Mouse, &mouse::OPTIONS),
    ];
    owners
        .iter()
        .find(|(_, names)| names.iter().any(|name| name.replace('-', "_") == option))
        .map(|(area, _)| *area)
}

pub fn migrate_settings_files() -> std::io::Result<()> {
    hyprconfig::split_legacy(option_area)?;
    hyprconfig::move_misplaced(option_area)
}

pub fn ancestors(id: &str) -> Vec<&'static str> {
    let mut chain = Vec::new();
    let mut current = id;
    while let Some(found) = subpage(current) {
        chain.insert(0, found.parent);
        current = found.parent;
    }
    chain
}

pub fn within(id: &str, ancestor: &str) -> bool {
    id == ancestor || ancestors(id).contains(&ancestor)
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
        "ipv4" => ipv4::build(context),
        "ipv6" => ipv6::build(context),
        "eap" => eap::build(context),
        "proxy" => proxy::build(context),
        "firewall" => firewall::build(context),
        "bluetooth" => bluetooth::build(context),
        "displays" => displays::build(context),
        "displaycolor" => displaycolor::build(context),
        "nightlight" => nightlight::build(context),
        "sound" => sound::build(context),
        "power" => power::build(context),
        "lock" => lock::build(context),
        "multitasking" => multitasking::build(context),
        "overview" => overview::build(context),
        "swiping" => swiping::build(context),
        "appearance" => appearance::build(context),
        "colors" => colors::build(context),
        "fonts" => fonts::build(context),
        "windows" => windows::build(context),
        "background" => background::build(context),
        "bar" => bar::build(context),
        "utilitybuttons" => utilitybuttons::build(context),
        "barworkspaces" => barworkspaces::build(context),
        "panels" => panels::build(context),
        "dock" => dock::build(context),
        "sidebars" => sidebars::build(context),
        "cheatsheet" => cheatsheet::build(context),
        "apps" => apps::build(context),
        "mouse" => mouse::build(context),
        "mousedevice" => mousedevice::build(context),
        "touchpad" => touchpad::build(context),
        "keyboard" => keyboard::build(context),
        "keyoptions" => keyoptions::build(context),
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
        "windowrules" => windowrules::build(context),
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
    fn every_subpage_leads_up_to_a_rail_page_and_is_not_one_itself() {
        for subpage in &SUBPAGES {
            let chain = ancestors(subpage.id);
            assert!(
                chain.first().is_some_and(|root| index_of(root).is_some()),
                "{}",
                subpage.id
            );
            assert!(index_of(subpage.id).is_none(), "{}", subpage.id);
        }
        assert_eq!(ancestors("capture"), ["privacy"]);
        assert_eq!(ancestors("ipv4"), ["network", "connection"]);
        assert!(within("ipv4", "connection"));
        assert!(within("connection", "connection"));
        assert!(!within("network", "connection"));
        assert!(!within("proxy", "connection"));
    }

    #[test]
    fn options_are_stored_with_the_page_that_shows_them() {
        assert_eq!(option_area("general:border_size"), Some(Area::Appearance));
        assert_eq!(option_area("general:gaps_in"), Some(Area::Multitasking));
        assert_eq!(option_area("general:allow_tearing"), Some(Area::Displays));
        assert_eq!(option_area("cursor:enable_hyprcursor"), Some(Area::Appearance));
        for option in [
            "misc:animate_manual_resizes",
            "misc:animate_mouse_windowdragging",
        ] {
            assert_eq!(option_area(option), Some(Area::Multitasking), "{option}");
        }
    }
}

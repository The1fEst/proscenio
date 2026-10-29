pub mod administration;
pub mod connectivity;
pub mod desktop;
pub mod input;
pub mod quick;
pub mod software;

use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Context, Page};
use crate::platform::hyprland::hyprconfig::{self, Area};
use crate::ui::widgets::display::text;

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
            "warp",
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

pub const SUBPAGES: [Subpage; 44] = [
    Subpage {
        id: "volumelevels",
        title: "Volume Levels",
        parent: "sound",
    },
    Subpage {
        id: "soundcards",
        title: "Sound cards",
        parent: "sound",
    },
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
        id: "updates",
        title: "Updates",
        parent: "system",
    },
    Subpage {
        id: "updatepackages",
        title: "Packages",
        parent: "updates",
    },
    Subpage {
        id: "shellcommits",
        title: "Commits",
        parent: "updates",
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

fn area_of(option: &str) -> Option<Area> {
    let owners: [(Area, &[&str]); 7] = [
        (Area::Appearance, &desktop::appearance::OPTIONS),
        (Area::Appearance, &desktop::appearance::windows::OPTIONS),
        (Area::Displays, &desktop::displays::OPTIONS),
        (Area::Multitasking, &desktop::multitasking::OPTIONS),
        (Area::Keyboard, &input::keyboard::OPTIONS),
        (Area::Accessibility, &administration::accessibility::OPTIONS),
        (Area::Mouse, &input::mouse::OPTIONS),
    ];
    owners
        .iter()
        .find(|(_, names)| names.iter().any(|name| name.replace('-', "_") == option))
        .map(|(area, _)| *area)
}

pub fn migrate_settings_files() -> std::io::Result<()> {
    hyprconfig::split_legacy(area_of)?;
    hyprconfig::move_misplaced(area_of)
}

pub fn ancestors(id: &str) -> Vec<&'static str> {
    let mut chain = Vec::new();
    let mut current = id;
    while let Some(entry) = subpage(current) {
        chain.insert(0, entry.parent);
        current = entry.parent;
    }
    chain
}

pub fn is_within(id: &str, ancestor: &str) -> bool {
    id == ancestor || ancestors(id).contains(&ancestor)
}

pub fn subpage(id: &str) -> Option<&'static Subpage> {
    SUBPAGES.iter().find(|subpage| subpage.id == id)
}

pub fn build(id: &str, subpage: Option<&str>, context: &Context) -> Rc<Page> {
    match id {
        "quick" => quick::build(context),
        "wifi" => connectivity::wifi::build(context),
        "network" => connectivity::network::build(context),
        "connection" => connectivity::network::connection::build(context),
        "ipv4" => connectivity::network::connection::ipv4::build(context),
        "ipv6" => connectivity::network::connection::ipv6::build(context),
        "eap" => connectivity::network::connection::eap::build(context),
        "proxy" => connectivity::network::proxy::build(context),
        "firewall" => connectivity::network::firewall::build(context),
        "bluetooth" => connectivity::bluetooth::build(context),
        "displays" => desktop::displays::builder::build(context),
        "displaycolor" => desktop::displays::displaycolor::build(context),
        "nightlight" => desktop::displays::nightlight::build(context),
        "sound" => desktop::sound::build(context),
        "volumelevels" => desktop::sound::volumelevels::build(context),
        "soundcards" => desktop::sound::soundcards::build(context),
        "power" => desktop::power::build(context),
        "lock" => administration::privacy::lock::build(context),
        "multitasking" => desktop::multitasking::build(context),
        "overview" => desktop::multitasking::overview::build(context),
        "swiping" => desktop::multitasking::swiping::build(context),
        "appearance" => desktop::appearance::build(context),
        "colors" => desktop::appearance::colors::build(context),
        "fonts" => desktop::appearance::fonts::build(context),
        "windows" => desktop::appearance::windows::build(context),
        "background" => desktop::appearance::background::build(context),
        "bar" => desktop::appearance::bar::build(context),
        "utilitybuttons" => desktop::appearance::bar::utilitybuttons::build(context),
        "barworkspaces" => desktop::appearance::bar::barworkspaces::build(context),
        "panels" => desktop::appearance::panels::build(context),
        "dock" => desktop::appearance::panels::dock::build(context),
        "sidebars" => desktop::appearance::panels::sidebars::build(context),
        "cheatsheet" => desktop::appearance::panels::cheatsheet::build(context),
        "apps" => software::apps::build(context),
        "mouse" => input::mouse::build(context),
        "mousedevice" => input::mouse::mousedevice::build(context),
        "touchpad" => input::mouse::touchpad::build(context),
        "keyboard" => input::keyboard::build(context),
        "keyoptions" => input::keyboard::keyoptions::build(context),
        "devices" => input::devices::build(context),
        "accessibility" => administration::accessibility::build(context),
        "savednetworks" => connectivity::wifi::savednetworks::build(context),
        "hiddennetwork" => connectivity::wifi::hiddennetwork::build(context),
        "hotspot" => connectivity::wifi::hotspot::build(context),
        "notifications" => software::notifications::build(context),
        "search" => software::search::build(context),
        "privacy" => administration::privacy::build(context),
        "capture" => administration::privacy::capture::build(context),
        "system" => administration::system::build(context),
        "region" => administration::system::region::build(context),
        "datetime" => administration::system::datetime::build(context),
        "users" => administration::system::users::build(context),
        "autostart" => administration::system::autostart::build(context),
        "filetypes" => software::apps::filetypes::build(context),
        "windowrules" => software::apps::windowrules::build(context),
        "shortcuts" => input::keyboard::shortcuts::build(context),
        "updates" => administration::system::updates::build(context),
        "updatepackages" => administration::system::updates::updatepackages::build(context),
        "shellcommits" => administration::system::updates::shellcommits::build(context),
        "about" => administration::system::about::build(context),
        "services" => administration::system::services::build(context),
        "advanced" => administration::system::advanced::build(context),
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
        assert!(is_within("ipv4", "connection"));
        assert!(is_within("connection", "connection"));
        assert!(!is_within("network", "connection"));
        assert!(!is_within("proxy", "connection"));
    }

    #[test]
    fn options_are_stored_with_the_page_that_shows_them() {
        assert_eq!(area_of("general:border_size"), Some(Area::Appearance));
        assert_eq!(area_of("general:gaps_in"), Some(Area::Multitasking));
        assert_eq!(area_of("general:allow_tearing"), Some(Area::Displays));
        assert_eq!(area_of("cursor:enable_hyprcursor"), Some(Area::Appearance));
        assert_eq!(area_of("cursor:warp_on_monitor_change"), Some(Area::Mouse));
        for option in [
            "misc:animate_manual_resizes",
            "misc:animate_mouse_windowdragging",
        ] {
            assert_eq!(area_of(option), Some(Area::Multitasking), "{option}");
        }
    }
}

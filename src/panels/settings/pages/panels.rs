use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::{config, tools};
use crate::panels::osk::layouts::LAYOUTS;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::text;

const HOVER_TO_REVEAL: &str = "/dock/hoverToReveal";
const TOGGLES_STYLE: &str = "/sidebar/quickToggles/style";
const SLIDERS: &str = "/sidebar/quickSliders/enable";
const CORNER_OPEN: &str = "/sidebar/cornerOpen/enable";
const CLICKLESS: &str = "/sidebar/cornerOpen/clickless";
const OSK_LAYOUT: &str = "/osk/layout";
const SUPER_KEY: &str = "/cheatsheet/superKey";
const SUPER_KEYS: [&str; 19] = [
    "\u{f05b3}",
    "\u{e8e5}",
    "\u{f0a21}",
    "\u{ebc6}",
    "\u{f033d}",
    "\u{f08c7}",
    "\u{f322}",
    "\u{f312}",
    "\u{e7e6}",
    "\u{e77d}",
    "\u{ef72}",
    "\u{f111b}",
    "\u{e7d9}",
    "\u{f1b6}",
    "\u{e711}",
    "\u{2318}",
    "\u{f0032}",
    "\u{f07cd}",
    "\u{f268}",
];

fn choice(label: &str, icon: &'static str, value: &str) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value: Value::from(value),
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let dock = page.section("call_to_action", "Dock");
    page.config_switch(&dock, "check", "Enable", "/dock/enable", false);
    let reveal = page.subsection(&dock, "Reveal", "");
    let reveal_row = page.uniform_row(&reveal);
    page.config_switch(
        &reveal_row,
        "highlight_mouse_cursor",
        "Hover to reveal",
        HOVER_TO_REVEAL,
        true,
    );
    page.config_switch(
        &reveal_row,
        "keep",
        "Pinned on startup",
        "/dock/pinnedOnStartup",
        false,
    );
    let hover_region = page.config_spin(
        &reveal,
        "highlight_mouse_cursor",
        "Hover region height (px)",
        "/dock/hoverRegionHeight",
        2,
        (1, 50),
        1,
    );
    let looks = page.subsection(&dock, "Looks", "");
    page.config_switch(
        &looks,
        "colors",
        "Tint app icons",
        "/dock/monochromeIcons",
        true,
    );
    page.config_spin(
        &looks,
        "height",
        "Height (px)",
        "/dock/height",
        60,
        (30, 150),
        5,
    );
    let pinned = page.subsection(
        &dock,
        "Pinned apps",
        "Comma-separated desktop entry IDs, in the order they should appear",
    );
    page.config_list(
        &pinned,
        "e.g. org.kde.dolphin, kitty",
        "/dock/pinnedApps",
        &["org.kde.dolphin", "kitty"],
    );
    let ignored = page.subsection(
        &dock,
        "Ignored apps",
        "Comma-separated regexes. Matching windows won't get a dock entry.",
    );
    page.config_list(
        &ignored,
        "e.g. ^steam_app_.*",
        "/dock/ignoredAppRegexes",
        &[],
    );

    let sidebars = page.section("side_navigation", "Sidebars");
    let toggles = page.subsection(
        &sidebars,
        "Quick toggles",
        "Which toggles are shown, their size and their order are edited in the sidebar itself, with its edit mode",
    );
    let styles = page.selection(
        &toggles,
        vec![
            choice("Classic", "password_2", "classic"),
            choice("Android", "action_key", "android"),
        ],
        TOGGLES_STYLE,
        Value::from("android"),
        |value| config::store_value(TOGGLES_STYLE, value),
    );
    styles.root.set_hexpand(false);
    let columns = page.config_spin(
        &toggles,
        "splitscreen_left",
        "Columns",
        "/sidebar/quickToggles/android/columns",
        5,
        (1, 8),
        1,
    );
    let sliders = page.subsection(&sidebars, "Sliders", "");
    page.config_switch(&sliders, "check", "Enable", SLIDERS, false);
    let shown_sliders: Vec<_> = [
        (
            "brightness_6",
            "Brightness",
            "/sidebar/quickSliders/showBrightness",
            true,
        ),
        (
            "volume_up",
            "Volume",
            "/sidebar/quickSliders/showVolume",
            true,
        ),
        ("mic", "Microphone", "/sidebar/quickSliders/showMic", false),
    ]
    .into_iter()
    .map(|(icon, label, pointer, default)| {
        page.config_switch(&sliders, icon, label, pointer, default)
    })
    .collect();

    let corners = page.subsection(
        &sidebars,
        "Corner open",
        "Allows you to open sidebars by clicking or hovering screen corners regardless of bar position",
    );
    let enable_row = page.uniform_row(&corners);
    page.config_switch(&enable_row, "check", "Enable", CORNER_OPEN, true);
    let clickless = page.config_switch(
        &corners,
        "highlight_mouse_cursor",
        "Hover to trigger",
        CLICKLESS,
        false,
    );
    page.tip(&clickless.button, "When this is off you'll have to click");
    let corner_end_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    corner_end_row.set_halign(gtk4::Align::Start);
    corners.append(&corner_end_row);
    let corner_end = page.config_switch(
        &corner_end_row,
        "",
        "Force hover open at absolute corner",
        "/sidebar/cornerOpen/clicklessCornerEnd",
        true,
    );
    corner_end.button.set_hexpand(false);
    page.tip(
        &corner_end.button,
        "When the previous option is off and this is on,\nyou can still hover the corner's end to open sidebar,\nand the remaining area can be used for volume/brightness scroll",
    );
    let offset = page.config_spin(
        &corner_end_row,
        "arrow_cool_down",
        "with vertical offset",
        "/sidebar/cornerOpen/clicklessCornerVerticalOffset",
        1,
        (0, 20),
        1,
    );
    offset.0.set_margin_start(0);
    offset.0.set_margin_end(0);
    offset.0.set_hexpand(false);
    page.tip(
        &offset.0,
        "Why this is cool:\nFor non-0 values, it won't trigger when you reach the\nscreen corner along the horizontal edge, but it will when\nyou do along the vertical edge",
    );
    let placing = page.uniform_row(&corners);
    let bottom = page.config_switch(
        &placing,
        "vertical_align_bottom",
        "Place at bottom",
        "/sidebar/cornerOpen/bottom",
        false,
    );
    page.tip(&bottom.button, "Place the corners to trigger at the bottom");
    let value_scroll = page.config_switch(
        &placing,
        "unfold_more_double",
        "Value scroll",
        "/sidebar/cornerOpen/valueScroll",
        true,
    );
    page.tip(&value_scroll.button, "Brightness and volume");
    let visualize = page.config_switch(
        &corners,
        "visibility",
        "Visualize region",
        "/sidebar/cornerOpen/visualize",
        false,
    );
    let region = page.row(&corners);
    let region_width = page.config_spin(
        &region,
        "arrow_range",
        "Region width",
        "/sidebar/cornerOpen/cornerRegionWidth",
        250,
        (1, 300),
        1,
    );
    let region_height = page.config_spin(
        &region,
        "height",
        "Region height",
        "/sidebar/cornerOpen/cornerRegionHeight",
        5,
        (1, 300),
        1,
    );

    let selector = page.section("wallpaper_slideshow", "Wallpaper selector");
    page.tools_notice(
        &selector,
        &[&tools::KDIALOG],
        "the system file picker does not open",
    );
    page.config_switch(
        &selector,
        "ad",
        "Use system file picker",
        "/wallpaperSelector/useSystemFileDialog",
        false,
    );

    let osk = page.section("keyboard", "On-screen keyboard");
    page.tools_notice(
        &osk,
        &[&tools::YDOTOOL],
        "the on-screen keyboard types nothing",
    );
    page.config_switch(
        &osk,
        "keep",
        "Pinned on startup",
        "/osk/pinnedOnStartup",
        false,
    );
    let layout = page.subsection(&osk, "Layout", "");
    let layouts = page.combo(&layout, "keyboard_alt");
    let names: Vec<String> = LAYOUTS
        .iter()
        .map(|layout| layout.name.to_owned())
        .collect();
    let show_layout = {
        let layouts = Rc::downgrade(&layouts);
        move || {
            if let Some(layouts) = layouts.upgrade() {
                let current = config::value_str(OSK_LAYOUT).unwrap_or_default();
                layouts.set_items_showing(&names, &current);
            }
        }
    };
    show_layout();
    page.watch(OSK_LAYOUT, show_layout);
    layouts.connect_activated(|index| {
        config::store_value(OSK_LAYOUT, Value::from(LAYOUTS[index].name));
    });

    let cheatsheet = page.section("keyboard_keys", "Cheat sheet");
    let super_key = page.subsection(
        &cheatsheet,
        "Super key symbol",
        "You can also manually edit cheatsheet.superKey",
    );
    let keys = Selection::with_family(
        &page.theme,
        SUPER_KEYS.iter().map(|key| choice(key, "", key)).collect(),
        text::Family::Nerd,
        |value| config::store_value(SUPER_KEY, value),
    );
    let show_key = {
        let keys = Rc::downgrade(&keys);
        move || {
            if let Some(keys) = keys.upgrade() {
                keys.set_current(&config::value(SUPER_KEY).unwrap_or(Value::from("")));
            }
        }
    };
    show_key();
    page.watch(SUPER_KEY, show_key);
    super_key.append(&keys.root);
    page.keep(keys);

    let symbols = page.subsection(&cheatsheet, "Symbols", "");
    for (icon, label, pointer, tip) in [
        (
            "󰘵",
            "Use macOS-like symbols for mods keys",
            "/cheatsheet/useMacSymbol",
            "e.g. 󰘴  for Ctrl, 󰘵  for Alt, 󰘶  for Shift, etc",
        ),
        (
            "󱊶",
            "Use symbols for function keys",
            "/cheatsheet/useFnSymbol",
            "e.g. 󱊫 for F1, 󱊶  for F12",
        ),
        (
            "󰍽",
            "Use symbols for mouse",
            "/cheatsheet/useMouseSymbol",
            "Replace 󱕐   for \"Scroll ↓\", 󱕑   \"Scroll ↑\", L󰍽   \"LMB\", R󰍽   \"RMB\", 󱕒   \"Scroll ↑/↓\" and ⇞/⇟ for \"Page_↑/↓\"",
        ),
    ] {
        let switch = page.config_switch(&symbols, icon, label, pointer, false);
        page.tip(&switch.button, tip);
    }
    let keycaps = page.subsection(&cheatsheet, "Keycaps", "");
    let split = page.config_switch(
        &keycaps,
        "highlight_keyboard_focus",
        "Split buttons",
        "/cheatsheet/splitButtons",
        false,
    );
    page.tip(
        &split.button,
        "Display modifiers and keys in multiple keycap (e.g., \"Ctrl + A\" instead of \"Ctrl A\" or \"󰘴 + A\" instead of \"󰘴 A\")",
    );
    let sizes = page.uniform_row(&keycaps);
    page.config_spin(
        &sizes,
        "",
        "Keybind font size",
        "/cheatsheet/fontSize/key",
        12,
        (8, 30),
        1,
    );
    page.config_spin(
        &sizes,
        "",
        "Description font size",
        "/cheatsheet/fontSize/comment",
        12,
        (8, 30),
        1,
    );

    let follow_dock = move || {
        Page::set_spin_row_enabled(
            &hover_region.0,
            &hover_region.1,
            config::value_bool(HOVER_TO_REVEAL, true),
        );
    };
    follow_dock();
    page.watch("/dock", follow_dock);
    let follow_sidebar = move || {
        let style = config::value_str(TOGGLES_STYLE).unwrap_or_else(|| "android".to_owned());
        Page::set_spin_row_enabled(&columns.0, &columns.1, style == "android");
        let sliders_on = config::value_bool(SLIDERS, false);
        for switch in &shown_sliders {
            switch.set_enabled(sliders_on);
        }
        let corners_on = config::value_bool(CORNER_OPEN, true);
        let clicking = corners_on && !config::value_bool(CLICKLESS, false);
        clickless.set_enabled(corners_on);
        corner_end.set_enabled(clicking);
        Page::set_spin_row_enabled(&offset.0, &offset.1, clicking);
        bottom.set_enabled(corners_on);
        value_scroll.set_enabled(corners_on);
        visualize.set_enabled(corners_on);
        Page::set_spin_row_enabled(&region_width.0, &region_width.1, corners_on);
        Page::set_spin_row_enabled(&region_height.0, &region_height.1, corners_on);
    };
    follow_sidebar();
    page.watch("/sidebar", follow_sidebar);
    page
}

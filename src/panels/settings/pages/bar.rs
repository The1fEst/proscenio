use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::{config, tools};
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::pages::notifications::monitors;
use crate::panels::settings::pages::quick::{bar_position, corner_style};

const AUTO_HIDE: &str = "/bar/autoHide/enable";
const REVEAL_ON_SUPER: &str = "/bar/autoHide/showWhenPressingSuper/enable";
const SHOW_BACKGROUND: &str = "/bar/showBackground";
const CORNER_STYLE: &str = "/bar/cornerStyle";
const SCREEN_LIST: &str = "/bar/screenList";
const INVERT_PINNED: &str = "/tray/invertPinnedItems";
const POWER_PROFILES: &str = "net.hadess.PowerProfiles";
const NO_POWER_PROFILES: &str = "power-profiles-daemon is not on the system bus, so the Performance Profile button does nothing. It comes with the power-profiles-daemon package.";
const PINNED_ITEMS: &str = "/tray/pinnedItems";
const NUMBER_MAP: &str = "/bar/workspaces/numberMap";
const UPDATES: &str = "/bar/utilButtons/showUpdates";
const HAN: [&str; 20] = [
    "一", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四",
    "十五", "十六", "十七", "十八", "十九", "二十",
];
const ROMAN: [&str; 20] = [
    "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV",
    "XVI", "XVII", "XVIII", "XIX", "XX",
];

fn choice(label: &str, icon: &'static str, value: Value) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value,
    }
}

fn strings(pointer: &str) -> Vec<String> {
    config::value(pointer)
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn pinned_title() -> &'static str {
    if config::value_bool(INVERT_PINNED, true) {
        "Unpinned items"
    } else {
        "Pinned items"
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let notifications = page.section("notifications", "Notifications");
    page.config_switch(
        &notifications,
        "counter_2",
        "Unread indicator: show count",
        "/bar/indicators/notifications/showUnreadCount",
        false,
    );

    let positioning = page.section("spoke", "Positioning");
    let placement = page.row(&positioning);
    let position = page.subsection(&placement, "Bar position", "");
    bar_position(&page, &position);
    let hiding = page.subsection(&placement, "Automatically hide", "");
    page.selection(
        &hiding,
        vec![
            choice("No", "close", Value::Bool(false)),
            choice("Yes", "check", Value::Bool(true)),
        ],
        AUTO_HIDE,
        Value::Bool(false),
        |value| config::store_value(AUTO_HIDE, value),
    );
    Page::subsection_root(&hiding).set_hexpand(false);

    let holding = page.subsection(
        &positioning,
        "Holding Super",
        "Applies whether or not the bar auto-hides: this is also what makes workspace numbers appear while Super is held",
    );
    let reveal = page.config_switch(
        &holding,
        "keyboard_command_key",
        "Reveal bar and workspace numbers",
        REVEAL_ON_SUPER,
        true,
    );
    page.tip(
        &reveal.button,
        "Turn off to never show workspace numbers, unless \"Always show numbers\" is on",
    );
    let delay = page.config_spin(
        &holding,
        "touch_long",
        "Hold delay (ms)",
        "/bar/autoHide/showWhenPressingSuper/delay",
        140,
        (0, 1000),
        20,
    );
    let push = page.config_switch(
        &positioning,
        "move_down",
        "Push windows away",
        "/bar/autoHide/pushWindows",
        false,
    );
    page.tip(
        &push.button,
        "Reserve space for the bar even when it's hidden",
    );
    let (hover, _) = page.config_spin(
        &positioning,
        "width",
        "Hover region thickness (px)",
        "/bar/autoHide/hoverRegionWidth",
        2,
        (1, 50),
        1,
    );
    page.tip(
        &hover,
        "How far past the bar's edge the cursor still counts as touching it. Also what reveals an auto-hidden bar.",
    );
    let styles = page.row(&positioning);
    let corners = page.subsection(&styles, "Corner style", "");
    corner_style(&page, &corners);
    let groups = page.subsection(&styles, "Group style", "");
    page.selection(
        &groups,
        vec![
            choice("Pills", "location_chip", Value::Bool(false)),
            choice("Line-separated", "split_scene", Value::Bool(true)),
        ],
        "/bar/borderless",
        Value::Bool(false),
        |value| config::store_value("/bar/borderless", value),
    );
    Page::subsection_root(&groups).set_hexpand(false);

    let looks = page.section("format_paint", "Appearance");
    page.config_switch(
        &looks,
        "rectangle",
        "Show background",
        SHOW_BACKGROUND,
        true,
    );
    let shadow = page.config_switch(
        &looks,
        "ev_shadow",
        "Shadow when floating",
        "/bar/floatStyleShadow",
        true,
    );
    page.tip(&shadow.button, "Only applies with the Float corner style");
    let verbose = page.config_switch(&looks, "notes", "Verbose", "/bar/verbose", true);
    page.tip(
        &verbose.button,
        "Shows the date next to the clock and the utility buttons",
    );
    let screens = page.subsection(
        &looks,
        "Monitors",
        "Which monitors get a bar. At least one always has it.",
    );
    let all: Rc<Vec<String>> = Rc::new(monitors().into_iter().map(|(_, name)| name).collect());
    for (label, name) in monitors() {
        let shown_on = {
            let all = all.clone();
            move || {
                let picked: Vec<String> = strings(SCREEN_LIST)
                    .into_iter()
                    .filter(|name| all.contains(name))
                    .collect();
                if picked.is_empty() {
                    all.to_vec()
                } else {
                    picked
                }
            }
        };
        let switch = page.switch(&screens, "monitor", &label, {
            let shown_on = shown_on.clone();
            let all = all.clone();
            let name = name.clone();
            move |on| {
                let mut wanted = shown_on();
                if wanted.contains(&name) == on {
                    return;
                }
                if on {
                    wanted.push(name.clone());
                } else {
                    wanted.retain(|shown| *shown != name);
                }
                if wanted.is_empty() {
                    return;
                }
                if all.iter().all(|name| wanted.contains(name)) {
                    wanted.clear();
                }
                config::store_value(SCREEN_LIST, Value::from(wanted));
            }
        });
        switch.bind(move || shown_on().contains(&name));
        page.refresh_on(SCREEN_LIST, &switch);
        switch.button.connect_clicked({
            let switch = Rc::downgrade(&switch);
            move |_| {
                if let Some(switch) = switch.upgrade() {
                    switch.refresh();
                }
            }
        });
    }

    let resources = page.section("memory", "Resources");
    let thresholds = page.subsection(&resources, "Warning thresholds (%)", "");
    for (icon, label, pointer, default) in [
        (
            "memory",
            "Memory",
            "/bar/resources/memoryWarningThreshold",
            95,
        ),
        (
            "memory_alt",
            "Swap",
            "/bar/resources/swapWarningThreshold",
            85,
        ),
        ("speed", "CPU", "/bar/resources/cpuWarningThreshold", 90),
    ] {
        page.config_spin(&thresholds, icon, label, pointer, default, (0, 100), 5);
    }

    let tray = page.section("shelf_auto_hide", "Tray");
    page.config_switch(
        &tray,
        "keep",
        "Make icons pinned by default",
        INVERT_PINNED,
        true,
    );
    page.config_switch(&tray, "colors", "Tint icons", "/tray/monochromeIcons", true);
    let passive = page.config_switch(
        &tray,
        "visibility_off",
        "Hide passive items",
        "/tray/filterPassive",
        true,
    );
    page.tip(
        &passive.button,
        "Hides tray items that report themselves as passive",
    );
    let ids = page.config_switch(
        &tray,
        "id_card",
        "Show item IDs in tooltips",
        "/tray/showItemId",
        false,
    );
    page.tip(&ids.button, "Useful for finding out what to type below");
    let pinned = page.subsection(&tray, pinned_title(), "Comma-separated tray item IDs");
    page.config_list(&pinned, "e.g. Fcitx, steam", PINNED_ITEMS, &["Fcitx"]);
    page.watch(INVERT_PINNED, move || {
        Page::set_subsection_title(&pinned, pinned_title())
    });

    let buttons = page.section("widgets", "Utility buttons");
    page.tools_notice(
        &buttons,
        &[
            &tools::GRIM,
            &tools::MAGICK,
            &tools::WL_COPY,
            &tools::HYPRPICKER,
            &tools::YDOTOOL,
            &tools::WPCTL,
            &tools::WF_RECORDER,
            &tools::SLURP,
        ],
        "the buttons that run them do nothing",
    );
    if !tools::system_service(POWER_PROFILES) {
        page.notice(&buttons, "info", NO_POWER_PROFILES);
    }
    for pair in [
        [
            (
                "content_cut",
                "Screen snip",
                "/bar/utilButtons/showScreenSnip",
                true,
            ),
            (
                "colorize",
                "Color picker",
                "/bar/utilButtons/showColorPicker",
                false,
            ),
        ],
        [
            (
                "keyboard",
                "Keyboard toggle",
                "/bar/utilButtons/showKeyboardToggle",
                true,
            ),
            ("mic", "Mic toggle", "/bar/utilButtons/showMicToggle", false),
        ],
        [
            (
                "dark_mode",
                "Dark/Light toggle",
                "/bar/utilButtons/showDarkModeToggle",
                true,
            ),
            (
                "speed",
                "Performance Profile toggle",
                "/bar/utilButtons/showPerformanceProfileToggle",
                false,
            ),
        ],
        [
            (
                "videocam",
                "Record",
                "/bar/utilButtons/showScreenRecord",
                false,
            ),
            ("deployed_code_update", "System updates", UPDATES, true),
        ],
    ] {
        let row = page.uniform_row(&buttons);
        for (icon, label, pointer, default) in pair {
            let switch = page.config_switch(&row, icon, label, pointer, default);
            if pointer == UPDATES {
                page.tip(
                    &switch.button,
                    "Appears once enough packages are out of date. The threshold is under Services.",
                );
            }
        }
    }

    let weather = page.section("cloud", "Weather");
    page.config_switch(&weather, "check", "Enable", "/bar/weather/enable", false);

    let workspaces = page.section("workspaces", "Workspaces");
    page.config_switch(
        &workspaces,
        "counter_1",
        "Always show numbers",
        "/bar/workspaces/alwaysShowNumbers",
        false,
    );
    page.config_switch(
        &workspaces,
        "award_star",
        "Show app icons",
        "/bar/workspaces/showAppIcons",
        true,
    );
    page.config_switch(
        &workspaces,
        "colors",
        "Tint app icons",
        "/bar/workspaces/monochromeIcons",
        true,
    );
    let nerd = page.config_switch(
        &workspaces,
        "font_download",
        "Nerd Font for workspace numbers",
        "/bar/workspaces/useNerdFont",
        false,
    );
    page.tip(
        &nerd.button,
        "Renders workspace numbers with your Nerd Font instead of the main one",
    );
    page.config_spin(
        &workspaces,
        "view_column",
        "Workspaces shown",
        "/bar/workspaces/shown",
        10,
        (1, 30),
        1,
    );
    let numbers = page.subsection(&workspaces, "Number style", "");
    page.selection_of(
        &numbers,
        vec![
            choice("Normal", "timer_10", Value::from(Vec::<String>::new())),
            choice("Han chars", "square_dot", Value::from(HAN.to_vec())),
            choice("Roman", "account_balance", Value::from(ROMAN.to_vec())),
        ],
        &[NUMBER_MAP],
        || config::value(NUMBER_MAP).unwrap_or_else(|| Value::from(vec!["1", "2"])),
        |value| config::store_value(NUMBER_MAP, value),
    );

    let follow = move || {
        Page::set_spin_row_enabled(
            &delay.0,
            &delay.1,
            config::value_bool(REVEAL_ON_SUPER, true),
        );
        push.set_enabled(config::value_bool(AUTO_HIDE, false));
        shadow.set_enabled(
            config::value_bool(SHOW_BACKGROUND, true) && config::value_i64(CORNER_STYLE, 0) == 1,
        );
    };
    follow();
    page.watch("/bar", follow);
    page
}

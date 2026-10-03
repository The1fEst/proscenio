use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::pages::notifications::monitors;
use crate::panels::settings::pages::quick::{bar_position, corner_style, screen_rounding};

const AUTO_HIDE: &str = "/bar/autoHide/enable";
const REVEAL_ON_SUPER: &str = "/bar/autoHide/showWhenPressingSuper/enable";
const SHOW_BACKGROUND: &str = "/bar/showBackground";
const CORNER_STYLE: &str = "/bar/cornerStyle";
const SCREEN_LIST: &str = "/bar/screenList";
const INVERT_PINNED: &str = "/tray/invertPinnedItems";
const PINNED_ITEMS: &str = "/tray/pinnedItems";

pub(super) fn choice(label: &str, icon: &'static str, value: Value) -> Choice {
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

    let notifications = page.section("notifications", &tr("Notifications"));
    page.config_switch(
        &notifications,
        "counter_2",
        &tr("Unread indicator: show count"),
        "/bar/indicators/notifications/showUnreadCount",
        false,
    );

    let positioning = page.section("spoke", &tr("Positioning"));
    let placement = page.row(&positioning);
    let position = page.subsection(&placement, &tr("Bar position"), "");
    bar_position(&page, &position);
    let hiding = page.subsection(&placement, &tr("Automatically hide"), "");
    page.selection(
        &hiding,
        vec![
            choice(&tr("No"), "close", Value::Bool(false)),
            choice(&tr("Yes"), "check", Value::Bool(true)),
        ],
        AUTO_HIDE,
        Value::Bool(false),
        |value| config::store_value(AUTO_HIDE, value),
    );
    Page::subsection_root(&hiding).set_hexpand(false);

    let holding = page.subsection(
        &positioning,
        &tr("Holding Super"),
        &tr(
            "Applies whether or not the bar auto-hides: this is also what makes workspace numbers appear while Super is held",
        ),
    );
    let reveal = page.config_switch(
        &holding,
        "keyboard_command_key",
        &tr("Reveal bar and workspace numbers"),
        REVEAL_ON_SUPER,
        true,
    );
    page.tip(
        &reveal.button,
        &tr("Turn off to never show workspace numbers, unless \"Always show numbers\" is on"),
    );
    let delay = page.config_spin(
        &holding,
        "touch_long",
        &tr("Hold delay (ms)"),
        "/bar/autoHide/showWhenPressingSuper/delay",
        140,
        (0, 1000),
        20,
    );
    let push = page.config_switch(
        &positioning,
        "move_down",
        &tr("Push windows away"),
        "/bar/autoHide/pushWindows",
        false,
    );
    page.tip(
        &push.button,
        &tr("Reserve space for the bar even when it's hidden"),
    );
    let (hover, _) = page.config_spin(
        &positioning,
        "width",
        &tr("Hover region thickness (px)"),
        "/bar/autoHide/hoverRegionWidth",
        2,
        (1, 50),
        1,
    );
    page.tip(
        &hover,
        &tr(
            "How far past the bar's edge the cursor still counts as touching it. Also what reveals an auto-hidden bar.",
        ),
    );
    let styles = page.row(&positioning);
    let corners = page.subsection(&styles, &tr("Bar style"), "");
    corner_style(&page, &corners);
    let groups = page.subsection(&styles, &tr("Group style"), "");
    page.selection(
        &groups,
        vec![
            choice(&tr("Pills"), "location_chip", Value::Bool(false)),
            choice(&tr("Line-separated"), "split_scene", Value::Bool(true)),
        ],
        "/bar/borderless",
        Value::Bool(false),
        |value| config::store_value("/bar/borderless", value),
    );
    Page::subsection_root(&groups).set_hexpand(false);
    let screen = page.row(&positioning);
    let rounding = page.subsection(&screen, &tr("Screen round corner"), "");
    screen_rounding(&page, &rounding);

    let looks = page.section("format_paint", &tr("Appearance"));
    page.config_switch(
        &looks,
        "rectangle",
        &tr("Show background"),
        SHOW_BACKGROUND,
        true,
    );
    let shadow = page.config_switch(
        &looks,
        "ev_shadow",
        &tr("Shadow when floating"),
        "/bar/floatStyleShadow",
        true,
    );
    page.tip(
        &shadow.button,
        &tr("Only applies with the Float corner style"),
    );
    let verbose = page.config_switch(&looks, "notes", &tr("Verbose"), "/bar/verbose", true);
    page.tip(
        &verbose.button,
        &tr("Shows the date next to the clock and the utility buttons"),
    );
    let screens = page.subsection(
        &looks,
        &tr("Monitors"),
        &tr("Which monitors get a bar. At least one always has it."),
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

    let resources = page.section("memory", &tr("Resources"));
    let thresholds = page.subsection(&resources, &tr("Warning thresholds (%)"), "");
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
        page.config_spin(&thresholds, icon, &tr(label), pointer, default, (0, 100), 5);
    }

    let tray = page.section("shelf_auto_hide", &tr("Tray"));
    page.config_switch(
        &tray,
        "keep",
        &tr("Make icons pinned by default"),
        INVERT_PINNED,
        true,
    );
    page.config_switch(
        &tray,
        "colors",
        &tr("Tint icons"),
        "/tray/monochromeIcons",
        true,
    );
    let passive = page.config_switch(
        &tray,
        "visibility_off",
        &tr("Hide passive items"),
        "/tray/filterPassive",
        true,
    );
    page.tip(
        &passive.button,
        &tr("Hides tray items that report themselves as passive"),
    );
    let ids = page.config_switch(
        &tray,
        "id_card",
        &tr("Show item IDs in tooltips"),
        "/tray/showItemId",
        false,
    );
    page.tip(
        &ids.button,
        &tr("Useful for finding out what to type below"),
    );
    let pinned = page.subsection(
        &tray,
        &tr(pinned_title()),
        &tr("Comma-separated tray item IDs"),
    );
    page.config_list(&pinned, &tr("e.g. Fcitx, steam"), PINNED_ITEMS, &["Fcitx"]);
    page.watch(INVERT_PINNED, move || {
        Page::set_subsection_title(&pinned, &tr(pinned_title()))
    });

    let weather = page.section("cloud", &tr("Weather"));
    page.config_switch(
        &weather,
        "check",
        &tr("Enable"),
        "/bar/weather/enable",
        false,
    );

    let more = page.section("", "");
    for (icon, title, subtitle, id) in [
        (
            "widgets",
            "Utility buttons",
            "Screen snip, color picker, toggles and recording",
            "utilitybuttons",
        ),
        (
            "workspaces",
            "Workspaces",
            "Numbers, app icons and how many are shown",
            "barworkspaces",
        ),
    ] {
        page.link_row(
            &more,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

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

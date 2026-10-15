pub mod barworkspaces;
pub mod utilitybuttons;

use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::pages::quick::{add_screen_rounding, bar_position, corner_style};
use crate::panels::settings::pages::software::notifications::monitors;

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

fn read_strings(pointer: &str) -> Vec<String> {
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

    let notifications_section = page.section("notifications", &tr("Notifications"));
    page.config_switch(
        &notifications_section,
        "counter_2",
        &tr("Unread indicator: show count"),
        "/bar/indicators/notifications/showUnreadCount",
        false,
    );

    let positioning_section = page.section("spoke", &tr("Positioning"));
    let placement_row = page.row(&positioning_section);
    let position_section = page.subsection(&placement_row, &tr("Bar position"), "");
    bar_position(&page, &position_section);
    let auto_hide_section = page.subsection(&placement_row, &tr("Automatically hide"), "");
    page.selection(
        &auto_hide_section,
        vec![
            choice(&tr("No"), "close", Value::Bool(false)),
            choice(&tr("Yes"), "check", Value::Bool(true)),
        ],
        AUTO_HIDE,
        Value::Bool(false),
        |value| config::store_value(AUTO_HIDE, value),
    );
    Page::subsection_root(&auto_hide_section).set_hexpand(false);

    let super_hold_section = page.subsection(
        &positioning_section,
        &tr("Holding Super"),
        &tr(
            "Applies whether or not the bar auto-hides: this is also what makes workspace numbers appear while Super is held",
        ),
    );
    let reveal_switch = page.config_switch(
        &super_hold_section,
        "keyboard_command_key",
        &tr("Reveal bar and workspace numbers"),
        REVEAL_ON_SUPER,
        true,
    );
    page.tip(
        &reveal_switch.button,
        &tr("Turn off to never show workspace numbers, unless \"Always show numbers\" is on"),
    );
    let (delay_row, delay_spin) = page.config_spin(
        &super_hold_section,
        "touch_long",
        &tr("Hold delay (ms)"),
        "/bar/autoHide/showWhenPressingSuper/delay",
        140,
        (0, 1000),
        20,
    );
    let push_switch = page.config_switch(
        &positioning_section,
        "move_down",
        &tr("Push windows away"),
        "/bar/autoHide/pushWindows",
        false,
    );
    page.tip(
        &push_switch.button,
        &tr("While the hidden bar shows, windows make room for it instead of staying under it"),
    );
    let (hover_row, _) = page.config_spin(
        &positioning_section,
        "width",
        &tr("Hover region thickness (px)"),
        "/bar/autoHide/hoverRegionWidth",
        2,
        (1, 50),
        1,
    );
    page.tip(
        &hover_row,
        &tr(
            "How far past the bar's edge the cursor still counts as touching it. Also what reveals an auto-hidden bar.",
        ),
    );
    let styles_row = page.row(&positioning_section);
    let corners_section = page.subsection(&styles_row, &tr("Bar style"), "");
    corner_style(&page, &corners_section);
    let group_style_section = page.subsection(&styles_row, &tr("Group style"), "");
    page.selection(
        &group_style_section,
        vec![
            choice(&tr("Pills"), "location_chip", Value::Bool(false)),
            choice(&tr("Line-separated"), "split_scene", Value::Bool(true)),
        ],
        "/bar/borderless",
        Value::Bool(false),
        |value| config::store_value("/bar/borderless", value),
    );
    Page::subsection_root(&group_style_section).set_hexpand(false);
    let screen_row = page.row(&positioning_section);
    let rounding_section = page.subsection(&screen_row, &tr("Screen round corner"), "");
    add_screen_rounding(&page, &rounding_section);

    let looks_section = page.section("format_paint", &tr("Appearance"));
    page.config_switch(
        &looks_section,
        "rectangle",
        &tr("Show background"),
        SHOW_BACKGROUND,
        true,
    );
    let shadow_switch = page.config_switch(
        &looks_section,
        "ev_shadow",
        &tr("Shadow when floating"),
        "/bar/floatStyleShadow",
        true,
    );
    page.tip(
        &shadow_switch.button,
        &tr("Only applies with the Float corner style"),
    );
    let verbose_switch = page.config_switch(
        &looks_section,
        "notes",
        &tr("Verbose"),
        "/bar/verbose",
        true,
    );
    page.tip(
        &verbose_switch.button,
        &tr("Shows the date next to the clock and the utility buttons"),
    );
    let monitors_section = page.subsection(
        &looks_section,
        &tr("Monitors"),
        &tr("Which monitors get a bar. At least one always has it."),
    );
    let all_monitors: Rc<Vec<String>> =
        Rc::new(monitors().into_iter().map(|(_, name)| name).collect());
    for (label, name) in monitors() {
        let read_shown_monitors = {
            let all_monitors = all_monitors.clone();
            move || {
                let monitors_in_list: Vec<String> = read_strings(SCREEN_LIST)
                    .into_iter()
                    .filter(|monitor| all_monitors.contains(monitor))
                    .collect();
                if monitors_in_list.is_empty() {
                    all_monitors.to_vec()
                } else {
                    monitors_in_list
                }
            }
        };
        let switch = page.switch(&monitors_section, "monitor", &label, {
            let read_shown_monitors = read_shown_monitors.clone();
            let all_monitors = all_monitors.clone();
            let name = name.clone();
            move |is_on| {
                let mut target_monitors = read_shown_monitors();
                if target_monitors.contains(&name) == is_on {
                    return;
                }
                if is_on {
                    target_monitors.push(name.clone());
                } else {
                    target_monitors.retain(|monitor| *monitor != name);
                }
                if target_monitors.is_empty() {
                    return;
                }
                if all_monitors
                    .iter()
                    .all(|monitor| target_monitors.contains(monitor))
                {
                    target_monitors.clear();
                }
                config::store_value(SCREEN_LIST, Value::from(target_monitors));
            }
        });
        switch.bind(move || read_shown_monitors().contains(&name));
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

    let resources_section = page.section("memory", &tr("Resources"));
    let thresholds_section = page.subsection(&resources_section, &tr("Warning thresholds (%)"), "");
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
        page.config_spin(
            &thresholds_section,
            icon,
            &tr(label),
            pointer,
            default,
            (0, 100),
            5,
        );
    }

    let tray_section = page.section("shelf_auto_hide", &tr("Tray"));
    page.config_switch(
        &tray_section,
        "keep",
        &tr("Make icons pinned by default"),
        INVERT_PINNED,
        true,
    );
    page.config_switch(
        &tray_section,
        "colors",
        &tr("Tint icons"),
        "/tray/monochromeIcons",
        true,
    );
    let passive_switch = page.config_switch(
        &tray_section,
        "visibility_off",
        &tr("Hide passive items"),
        "/tray/filterPassive",
        true,
    );
    page.tip(
        &passive_switch.button,
        &tr("Hides tray items that report themselves as passive"),
    );
    let ids_switch = page.config_switch(
        &tray_section,
        "id_card",
        &tr("Show item IDs in tooltips"),
        "/tray/showItemId",
        false,
    );
    page.tip(
        &ids_switch.button,
        &tr("Useful for finding out what to type below"),
    );
    let pinned_section = page.subsection(
        &tray_section,
        &tr(pinned_title()),
        &tr("Comma-separated tray item IDs"),
    );
    page.config_list(&pinned_section, &tr("e.g. Fcitx, steam"), PINNED_ITEMS, &[]);
    page.watch(INVERT_PINNED, move || {
        Page::set_subsection_title(&pinned_section, &tr(pinned_title()))
    });

    let weather_section = page.section("cloud", &tr("Weather"));
    page.config_switch(
        &weather_section,
        "check",
        &tr("Enable"),
        "/bar/weather/enable",
        false,
    );

    let more_section = page.section("", "");
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
            &more_section,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let update_dependent_controls = move || {
        Page::set_spin_row_enabled(
            &delay_row,
            &delay_spin,
            config::value_bool(REVEAL_ON_SUPER, true),
        );
        push_switch.set_enabled(config::value_bool(AUTO_HIDE, false));
        shadow_switch.set_enabled(
            config::value_bool(SHOW_BACKGROUND, true) && config::value_i64(CORNER_STYLE, 0) == 1,
        );
    };
    update_dependent_controls();
    page.watch("/bar", update_dependent_controls);
    page
}

use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::desktop::appearance::bar::choice;

const NUMBER_MAP: &str = "/bar/workspaces/numberMap";
const HAN: [&str; 20] = [
    "一", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四",
    "十五", "十六", "十七", "十八", "十九", "二十",
];
const ROMAN: [&str; 20] = [
    "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV",
    "XVI", "XVII", "XVIII", "XIX", "XX",
];

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let workspaces_section = page.section("", "");
    page.config_switch(
        &workspaces_section,
        "counter_1",
        &tr("Always show numbers"),
        "/bar/workspaces/alwaysShowNumbers",
        false,
    );
    page.config_switch(
        &workspaces_section,
        "award_star",
        &tr("Show app icons"),
        "/bar/workspaces/showAppIcons",
        true,
    );
    page.config_switch(
        &workspaces_section,
        "colors",
        &tr("Tint app icons"),
        "/bar/workspaces/monochromeIcons",
        true,
    );
    let nerd_font_switch = page.config_switch(
        &workspaces_section,
        "font_download",
        &tr("Nerd Font for workspace numbers"),
        "/bar/workspaces/useNerdFont",
        false,
    );
    page.tip(
        &nerd_font_switch.button,
        &tr("Renders workspace numbers with your Nerd Font instead of the main one"),
    );
    let (shown_row, _) = page.config_spin(
        &workspaces_section,
        "view_column",
        &tr("Workspaces shown"),
        "/bar/workspaces/shown",
        10,
        (1, 30),
        1,
    );
    page.tip(
        &shown_row,
        &tr("Each display gets this many workspaces of its own"),
    );
    let numbers_section = page.subsection(&workspaces_section, &tr("Number style"), "");
    page.selection_of(
        &numbers_section,
        vec![
            choice(&tr("Normal"), "timer_10", Value::from(Vec::<String>::new())),
            choice(&tr("Han chars"), "square_dot", Value::from(HAN.to_vec())),
            choice(&tr("Roman"), "account_balance", Value::from(ROMAN.to_vec())),
        ],
        &[NUMBER_MAP],
        || config::value(NUMBER_MAP).unwrap_or_else(|| Value::from(Vec::<String>::new())),
        |value| config::store_value(NUMBER_MAP, value),
    );
    page
}

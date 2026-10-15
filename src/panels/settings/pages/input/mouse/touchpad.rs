use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::gestures;
use crate::panels::settings::hyprrows;
use crate::panels::settings::pages::input::mouse::{OPTIONS, choice, spin};
use crate::platform::hyprland::hyprconfig::Area;
use crate::services::hyprland::hyproptions::HyprOptions;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Mouse, &OPTIONS);

    let touchpad_section = page.section("", "");
    hyprrows::switch(
        &page,
        &touchpad_section,
        &options,
        "keyboard",
        &tr("Disable while typing"),
        "input:touchpad:disable_while_typing",
    );
    let clicking_section = page.subsection(&touchpad_section, &tr("Clicking"), "");
    let tap_switch = hyprrows::switch(
        &page,
        &clicking_section,
        &options,
        "touch_app",
        &tr("Tap to click"),
        "input:touchpad:tap_to_click",
    );
    page.tip(
        &tap_switch.button,
        &tr("Quickly touch the touchpad to click."),
    );
    hyprrows::switch(
        &page,
        &clicking_section,
        &options,
        "drag_pan",
        &tr("Tap and drag"),
        "input:touchpad:tap_and_drag",
    );
    hyprrows::switch(
        &page,
        &clicking_section,
        &options,
        "pan_tool",
        &tr("Middle click with three fingers"),
        "input:touchpad:middle_button_emulation",
    );
    let secondary_section = page.subsection(&clicking_section, &tr("Secondary click"), "");
    hyprrows::selection(
        &page,
        &secondary_section,
        &options,
        vec![
            choice(&tr("Corner push"), "south_west", Value::Bool(false)),
            choice(&tr("Two finger push"), "touch_app", Value::Bool(true)),
        ],
        |options| Value::Bool(options.flag("input:touchpad:clickfinger_behavior")),
        |options, value| options.set("input:touchpad:clickfinger_behavior", &value.to_string()),
    );
    let taps_section = page.subsection(&clicking_section, &tr("Tap with two or three fingers"), "");
    hyprrows::selection(
        &page,
        &taps_section,
        &options,
        vec![
            choice(
                &tr("Right, then middle"),
                "arrow_forward",
                Value::from("lrm"),
            ),
            choice(
                &tr("Middle, then right"),
                "arrow_upward",
                Value::from("lmr"),
            ),
        ],
        |options| {
            let map = options.text("input:touchpad:tap_button_map");
            Value::from(if map.is_empty() {
                "lrm".to_owned()
            } else {
                map
            })
        },
        |options, value| {
            if let Some(map) = value.as_str() {
                options.set("input:touchpad:tap_button_map", map);
            }
        },
    );
    let scrolling_section = page.subsection(&touchpad_section, &tr("Scrolling"), "");
    hyprrows::switch(
        &page,
        &scrolling_section,
        &options,
        "swap_vert",
        &tr("Natural scrolling"),
        "input:touchpad:natural_scroll",
    );
    hyprrows::spin(
        &page,
        &scrolling_section,
        &options,
        &spin(
            "height",
            "Scroll amount (%)",
            "input:touchpad:scroll_factor",
            100.0,
            (10, 500),
            10,
        ),
    );
    gestures::section(&page, &touchpad_section);

    page.keep(options);
    page
}

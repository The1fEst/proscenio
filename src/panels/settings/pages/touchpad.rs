use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::gestures;
use crate::panels::settings::hyprrows;
use crate::panels::settings::pages::mouse::{OPTIONS, choice, spin};
use crate::platform::hyprconfig::Area;
use crate::services::hyproptions::HyprOptions;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Mouse, &OPTIONS);

    let touchpad = page.section("", "");
    hyprrows::switch(
        &page,
        &touchpad,
        &options,
        "keyboard",
        &tr("Disable while typing"),
        "input:touchpad:disable_while_typing",
    );
    let clicking = page.subsection(&touchpad, &tr("Clicking"), "");
    let tap = hyprrows::switch(
        &page,
        &clicking,
        &options,
        "touch_app",
        &tr("Tap to click"),
        "input:touchpad:tap_to_click",
    );
    page.tip(&tap.button, &tr("Quickly touch the touchpad to click."));
    hyprrows::switch(
        &page,
        &clicking,
        &options,
        "drag_pan",
        &tr("Tap and drag"),
        "input:touchpad:tap_and_drag",
    );
    hyprrows::switch(
        &page,
        &clicking,
        &options,
        "pan_tool",
        &tr("Middle click with three fingers"),
        "input:touchpad:middle_button_emulation",
    );
    let secondary = page.subsection(&clicking, &tr("Secondary click"), "");
    hyprrows::selection(
        &page,
        &secondary,
        &options,
        vec![
            choice(&tr("Corner push"), "south_west", Value::Bool(false)),
            choice(&tr("Two finger push"), "touch_app", Value::Bool(true)),
        ],
        |options| Value::Bool(options.flag("input:touchpad:clickfinger_behavior")),
        |options, value| options.set("input:touchpad:clickfinger_behavior", &value.to_string()),
    );
    let taps = page.subsection(&clicking, &tr("Tap with two or three fingers"), "");
    hyprrows::selection(
        &page,
        &taps,
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
    let touch_scrolling = page.subsection(&touchpad, &tr("Scrolling"), "");
    hyprrows::switch(
        &page,
        &touch_scrolling,
        &options,
        "swap_vert",
        &tr("Natural scrolling"),
        "input:touchpad:natural_scroll",
    );
    hyprrows::spin(
        &page,
        &touch_scrolling,
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
    gestures::section(&page, &touchpad);

    page.keep(options);
    page
}

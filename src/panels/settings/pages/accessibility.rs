use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::bellflash;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::platform::colorfilter;
use crate::platform::hyprconfig::Area;
use crate::services::hyproptions::HyprOptions;

pub const OPTIONS: [&str; 9] = [
    "animations:enabled",
    "misc:animate_manual_resizes",
    "misc:animate_mouse_windowdragging",
    "input:repeat_delay",
    "input:repeat_rate",
    "cursor:zoom_factor",
    "cursor:zoom_disable_aa",
    SCREEN_SHADER,
    BELL_SOUND,
];
const SCREEN_SHADER: &str = "decoration:screen_shader";
const BELL_SOUND: &str = "misc:bell_sound";
const FILTER_NAMES: [&str; 6] = [
    "None",
    "Grayscale",
    "Inverted colors",
    "Red–green (deuteranopia)",
    "Red–green (protanopia)",
    "Blue–yellow (tritanopia)",
];

fn spin(
    icon: &'static str,
    label: &'static str,
    option: &'static str,
    factor: f64,
    range: (i64, i64),
    step: i64,
) -> Spin {
    Spin {
        icon,
        label,
        option,
        factor,
        range,
        step,
        decimals: 0,
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Accessibility, &OPTIONS);

    let seeing = page.section("visibility", &tr("Seeing"));
    let reduced = hyprrows::option_switch(
        &page,
        &seeing,
        &options,
        ("animation", tr("Reduced motion")),
        |options| !options.flag("animations:enabled"),
        |options, reduced| options.set("animations:enabled", &(!reduced).to_string()),
    );
    page.tip(
        &reduced.button,
        &tr("Windows and workspaces appear at once instead of moving."),
    );
    hyprrows::switch(
        &page,
        &seeing,
        &options,
        "open_with",
        &tr("Animate manual resizes"),
        "misc:animate_manual_resizes",
    );
    hyprrows::switch(
        &page,
        &seeing,
        &options,
        "drag_pan",
        &tr("Animate windows being dragged"),
        "misc:animate_mouse_windowdragging",
    );

    let filters = page.subsection(
        &seeing,
        &tr("Color filter"),
        &tr("Applied to the whole screen. The color-blindness filters shift the colors that are hard to tell apart toward ones that are not"),
    );
    let mut choices: Vec<Choice> = std::iter::once("")
        .chain(colorfilter::FILTERS)
        .zip(FILTER_NAMES)
        .map(|(filter, name)| Choice {
            label: tr(name),
            icon: "",
            value: Value::from(filter),
        })
        .collect();
    if colorfilter::filter_of(&options.text(SCREEN_SHADER)) == "custom" {
        choices.push(Choice {
            label: tr("Custom shader"),
            icon: "",
            value: Value::from("custom"),
        });
    }
    hyprrows::selection(
        &page,
        &filters,
        &options,
        choices,
        |options| Value::from(colorfilter::filter_of(&options.text(SCREEN_SHADER))),
        |options, value| {
            let filter = value.as_str().unwrap_or_default();
            if filter == "custom" {
                return;
            }
            let shader = colorfilter::install(filter).unwrap_or_default();
            options.set(SCREEN_SHADER, &shader);
        },
    );

    let bell = page.section("notifications_active", &tr("Bell"));
    if options.has(BELL_SOUND) {
        hyprrows::option_switch(
            &page,
            &bell,
            &options,
            ("volume_up", tr("Play the bell sound")),
            |options| !matches!(options.text(BELL_SOUND).as_str(), "" | "none"),
            |options, on| options.set(BELL_SOUND, if on { "default" } else { "none" }),
        );
    }
    let flash = page.config_switch(
        &bell,
        "flash_on",
        &tr("Flash the screen"),
        bellflash::FLASH,
        false,
    );
    page.tip(
        &flash.button,
        &tr("The display with the focused window lights up briefly when an app rings the bell"),
    );

    let typing = page.section("keyboard", &tr("Typing"));
    let repeat = page.subsection(
        &typing,
        &tr("Repeat keys"),
        &tr("Key presses repeat when the key is held down"),
    );
    let repeat_row = page.uniform_row(&repeat);
    hyprrows::spin(
        &page,
        &repeat_row,
        &options,
        &spin(
            "timer",
            "Delay (ms)",
            "input:repeat_delay",
            1.0,
            (100, 2000),
            25,
        ),
    );
    hyprrows::spin(
        &page,
        &repeat_row,
        &options,
        &spin(
            "speed",
            "Rate (per second)",
            "input:repeat_rate",
            1.0,
            (1, 100),
            1,
        ),
    );

    let zoom = page.section("zoom_in", &tr("Zoom"));
    let magnifier = page.subsection(
        &zoom,
        &tr("Magnifier"),
        &tr("The whole screen, magnified around the pointer. 100% is no magnification."),
    );
    hyprrows::spin(
        &page,
        &magnifier,
        &options,
        &spin(
            "zoom_in",
            "Magnification (%)",
            "cursor:zoom_factor",
            100.0,
            (100, 500),
            10,
        ),
    );
    hyprrows::switch(
        &page,
        &magnifier,
        &options,
        "grid_on",
        &tr("Keep the magnified image sharp"),
        "cursor:zoom_disable_aa",
    );

    page.keep(options);
    page
}

use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, slider_row};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::platform::hyprconfig::Area;
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::slider::Slider;

pub const OPTIONS: [&str; 18] = [
    "input:left_handed",
    "input:sensitivity",
    "input:accel_profile",
    "input:natural_scroll",
    "input:scroll_method",
    "input:scroll_factor",
    "cursor:inactive_timeout",
    "cursor:hide_on_key_press",
    "cursor:no_hardware_cursors",
    "cursor:enable_hyprcursor",
    "input:touchpad:disable_while_typing",
    "input:touchpad:tap_to_click",
    "input:touchpad:tap_and_drag",
    "input:touchpad:middle_button_emulation",
    "input:touchpad:clickfinger_behavior",
    "input:touchpad:tap_button_map",
    "input:touchpad:natural_scroll",
    "input:touchpad:scroll_factor",
];
const SCROLL_METHODS: [(&str, &str); 4] = [
    ("Two fingers or wheel", "2fg"),
    ("Along the edge", "edge"),
    ("While a button is held", "on_button_down"),
    ("No scrolling", "no_scroll"),
];
const HARDWARE_CURSORS: [(&str, &str); 3] = [
    ("The screen draws the pointer, except while tearing", "2"),
    ("The screen always draws the pointer", "0"),
    ("The pointer is drawn with the rest of the screen", "1"),
];
pub(super) const SPEED: (f64, f64) = (-100.0, 100.0);

pub(super) fn choice(label: &str, icon: &'static str, value: Value) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value,
    }
}

pub(super) fn spin(
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

pub(super) fn show_speed(slider: &Slider, value: f64) {
    slider.set(value);
    slider.set_tooltip(&format!("{}", value.round()));
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Mouse, &OPTIONS);

    let general = page.section("mouse", &tr("General"));
    let primary = page.subsection(
        &general,
        &tr("Primary button"),
        &tr("Order of the physical buttons on mice and touchpads"),
    );
    hyprrows::selection(
        &page,
        &primary,
        &options,
        vec![
            choice(&tr("Left"), "arrow_back", Value::Bool(false)),
            choice(&tr("Right"), "arrow_forward", Value::Bool(true)),
        ],
        |options| Value::Bool(options.flag("input:left_handed")),
        |options, value| options.set("input:left_handed", &value.to_string()),
    );

    let mouse = page.section("mouse", &tr("Mouse"));
    let speed_group = page.subsection(&mouse, &tr("Pointer speed"), "");
    let (speed, _) = slider_row(&page.theme, &speed_group, "speed", &tr("Speed"), SPEED);
    let show_general_speed = {
        let speed = Rc::downgrade(&speed);
        let options = Rc::downgrade(&options);
        move || {
            if let (Some(speed), Some(options)) = (speed.upgrade(), options.upgrade()) {
                show_speed(
                    &speed,
                    (options.number("input:sensitivity") * 100.0).round(),
                );
            }
        }
    };
    show_general_speed();
    options.connect_changed(show_general_speed);
    speed.on_moved({
        let options = Rc::downgrade(&options);
        let tip = Rc::downgrade(&speed);
        move |value| {
            if let Some(slider) = tip.upgrade() {
                slider.set_tooltip(&format!("{}", value.round()));
            }
            if let Some(options) = options.upgrade() {
                options.set("input:sensitivity", &format!("{}", value / 100.0));
            }
        }
    });
    page.keep(speed);
    let acceleration = hyprrows::option_switch(
        &page,
        &mouse,
        &options,
        ("trending_up", tr("Mouse acceleration")),
        |options| options.text("input:accel_profile") != "flat",
        |options, accelerated| {
            options.set(
                "input:accel_profile",
                if accelerated { "adaptive" } else { "flat" },
            )
        },
    );
    page.tip(
        &acceleration.button,
        &tr("Off moves the pointer exactly as far as the mouse moved, which games and drawing want.\nOn speeds the pointer up as the mouse moves faster."),
    );
    let scrolling = page.subsection(&mouse, &tr("Scrolling"), "");
    let natural = hyprrows::switch(
        &page,
        &scrolling,
        &options,
        "swap_vert",
        &tr("Natural scrolling"),
        "input:natural_scroll",
    );
    page.tip(
        &natural.button,
        &tr("Scrolling moves the content rather than the view."),
    );
    hyprrows::combo(
        &page,
        &scrolling,
        &options,
        "mouse",
        ("input:scroll_method", "2fg"),
        &SCROLL_METHODS,
    );
    hyprrows::spin(
        &page,
        &scrolling,
        &options,
        &spin(
            "height",
            "Scroll amount (%)",
            "input:scroll_factor",
            100.0,
            (10, 500),
            10,
        ),
    );
    page.link_row(
        &mouse,
        "usb",
        &tr("This mouse only"),
        &tr("Speed, acceleration and scrolling for one connected mouse"),
        context.subpage_opener("mousedevice"),
    );

    let pointer = page.section("highlight_mouse_cursor", &tr("Pointer"));
    let hiding = page.subsection(&pointer, &tr("Hiding"), "");
    let (still, _) = hyprrows::spin(
        &page,
        &hiding,
        &options,
        &spin(
            "timer",
            "Hide when still for (s)",
            "cursor:inactive_timeout",
            1.0,
            (0, 120),
            1,
        ),
    );
    page.tip(
        &still,
        &tr("Zero keeps the pointer on screen no matter how long it sits still."),
    );
    hyprrows::switch(
        &page,
        &hiding,
        &options,
        "keyboard_hide",
        &tr("Hide while typing"),
        "cursor:hide_on_key_press",
    );
    let drawing = page.subsection(&pointer, &tr("Drawing"), "");
    let hardware = hyprrows::combo(
        &page,
        &drawing,
        &options,
        "memory",
        ("cursor:no_hardware_cursors", "2"),
        &HARDWARE_CURSORS,
    );
    page.tip(
        &hardware.button,
        &tr("A pointer the screen draws itself stays smooth whatever the rest of the screen is doing.\nPick the last choice if the pointer disappears or is drawn in the wrong place."),
    );
    hyprrows::switch(
        &page,
        &drawing,
        &options,
        "animated_images",
        &tr("Use hyprcursor themes"),
        "cursor:enable_hyprcursor",
    );

    let touchpad = page.section("", "");
    page.link_row(
        &touchpad,
        "touch_app",
        &tr("Touchpad"),
        &tr("Tapping, clicking, scrolling and gestures"),
        context.subpage_opener("touchpad"),
    );

    page.keep(options);
    page
}

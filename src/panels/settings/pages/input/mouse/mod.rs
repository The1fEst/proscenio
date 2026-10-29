pub mod mousedevice;
pub mod touchpad;

use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::inputs::slider_row;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::platform::hyprland::hyprconfig::Area;
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::ui::widgets::input::slider::Slider;

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
    "cursor:warp_on_monitor_change",
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
const WARP_ON_MONITOR_CHANGE: [(&str, &str); 4] = [
    ("Same as for a workspace on the same display", "-1"),
    ("Leave the pointer where it is", "0"),
    ("Move the pointer to the last focused window", "1"),
    ("Always move the pointer to the last focused window", "2"),
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

    let general_section = page.section("mouse", &tr("General"));
    let primary_section = page.subsection(
        &general_section,
        &tr("Primary button"),
        &tr("Order of the physical buttons on mice and touchpads"),
    );
    hyprrows::selection(
        &page,
        &primary_section,
        &options,
        vec![
            choice(&tr("Left"), "arrow_back", Value::Bool(false)),
            choice(&tr("Right"), "arrow_forward", Value::Bool(true)),
        ],
        |options| Value::Bool(options.flag("input:left_handed")),
        |options, value| options.set("input:left_handed", &value.to_string()),
    );

    let mouse_section = page.section("mouse", &tr("Mouse"));
    let speed_group = page.subsection(&mouse_section, &tr("Pointer speed"), "");
    let (speed_slider, _) = slider_row(&page.theme, &speed_group, "speed", &tr("Speed"), SPEED);
    let show_general_speed = {
        let speed_slider = Rc::downgrade(&speed_slider);
        let options = Rc::downgrade(&options);
        move || {
            if let (Some(slider), Some(options)) = (speed_slider.upgrade(), options.upgrade()) {
                show_speed(
                    &slider,
                    (options.number("input:sensitivity") * 100.0).round(),
                );
            }
        }
    };
    show_general_speed();
    options.connect_changed(show_general_speed);
    speed_slider.on_moved({
        let options = Rc::downgrade(&options);
        let slider = Rc::downgrade(&speed_slider);
        move |value| {
            if let Some(slider) = slider.upgrade() {
                slider.set_tooltip(&format!("{}", value.round()));
            }
            if let Some(options) = options.upgrade() {
                options.set("input:sensitivity", &format!("{}", value / 100.0));
            }
        }
    });
    page.keep(speed_slider);
    let acceleration_switch = hyprrows::option_switch(
        &page,
        &mouse_section,
        &options,
        ("trending_up", tr("Mouse acceleration")),
        |options| options.text("input:accel_profile") != "flat",
        |options, is_accelerated| {
            options.set(
                "input:accel_profile",
                if is_accelerated { "adaptive" } else { "flat" },
            )
        },
    );
    page.tip(
        &acceleration_switch.button,
        &tr("Off moves the pointer exactly as far as the mouse moved, which games and drawing want.\nOn speeds the pointer up as the mouse moves faster."),
    );
    let scrolling_section = page.subsection(&mouse_section, &tr("Scrolling"), "");
    let natural_switch = hyprrows::switch(
        &page,
        &scrolling_section,
        &options,
        "swap_vert",
        &tr("Natural scrolling"),
        "input:natural_scroll",
    );
    page.tip(
        &natural_switch.button,
        &tr("Scrolling moves the content rather than the view."),
    );
    hyprrows::combo(
        &page,
        &scrolling_section,
        &options,
        "mouse",
        ("input:scroll_method", "2fg"),
        &SCROLL_METHODS,
    );
    hyprrows::spin(
        &page,
        &scrolling_section,
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
        &mouse_section,
        "usb",
        &tr("This mouse only"),
        &tr("Speed, acceleration and scrolling for one connected mouse"),
        context.subpage_opener("mousedevice"),
    );

    let pointer_section = page.section("highlight_mouse_cursor", &tr("Pointer"));
    let hiding_section = page.subsection(&pointer_section, &tr("Hiding"), "");
    let (hide_delay_row, _) = hyprrows::spin(
        &page,
        &hiding_section,
        &options,
        &spin(
            "timer",
            "Hide when still for (s)",
            "cursor:inactive_timeout",
            1.0,
            (0, 20),
            1,
        ),
    );
    page.tip(
        &hide_delay_row,
        &tr("Zero keeps the pointer on screen no matter how long it sits still."),
    );
    hyprrows::switch(
        &page,
        &hiding_section,
        &options,
        "keyboard_hide",
        &tr("Hide while typing"),
        "cursor:hide_on_key_press",
    );
    let drawing_section = page.subsection(&pointer_section, &tr("Drawing"), "");
    let hardware_combo = hyprrows::combo(
        &page,
        &drawing_section,
        &options,
        "memory",
        ("cursor:no_hardware_cursors", "2"),
        &HARDWARE_CURSORS,
    );
    page.tip(
        &hardware_combo.button,
        &tr("A pointer the screen draws itself stays smooth whatever the rest of the screen is doing.\nPick the last choice if the pointer disappears or is drawn in the wrong place."),
    );
    let warp_section = page.subsection(&pointer_section, &tr("Between displays"), "");
    let warp_combo = hyprrows::combo(
        &page,
        &warp_section,
        &options,
        "ads_click",
        ("cursor:warp_on_monitor_change", "-1"),
        &WARP_ON_MONITOR_CHANGE,
    );
    page.tip(
        &warp_combo.button,
        &tr("Applies when a workspace on another display takes the focus.\nThe first choice follows cursor:warp_on_change_workspace, which is off unless the config sets it. The last choice also moves the pointer while cursor:no_warps is on."),
    );

    let touchpad_section = page.section("", "");
    page.link_row(
        &touchpad_section,
        "touch_app",
        &tr("Touchpad"),
        &tr("Tapping, clicking, scrolling and gestures"),
        context.subpage_opener("touchpad"),
    );

    let shell_section = page.section("swipe", &tr("Scrolling in the shell"));
    page.config_switch(
        &shell_section,
        "touch_app",
        &tr("Faster touchpad scrolling"),
        "/interactions/scrolling/fasterTouchpadScroll",
        false,
    );
    page.config_spin(
        &shell_section,
        "mouse",
        &tr("Mouse scroll distance"),
        "/interactions/scrolling/mouseScrollFactor",
        120,
        (10, 1000),
        10,
    );
    page.config_spin(
        &shell_section,
        "touchpad_mouse",
        &tr("Touchpad scroll distance"),
        "/interactions/scrolling/touchpadScrollFactor",
        450,
        (10, 1000),
        10,
    );
    let (threshold_row, _) = page.config_spin(
        &shell_section,
        "conversion_path",
        &tr("Mouse detection threshold"),
        "/interactions/scrolling/mouseScrollDeltaThreshold",
        120,
        (1, 500),
        10,
    );
    page.tip(
        &threshold_row,
        &tr("Scroll events at least this large are treated as coming from a mouse instead of a touchpad"),
    );

    page.keep(options);
    page
}

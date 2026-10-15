use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::settings::content::{Choice, Context, Page, Parent, Style};
use crate::panels::settings::pages::desktop::appearance::fonts::family_options;
use crate::panels::settings::pages::quick;
use crate::services::session::appearance::DesktopAppearance;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::layout::centered::Centered;

const CLOCK: &str = "/background/widgets/clock";
const SHOW_ONLY_WHEN_LOCKED: &str = "/background/widgets/clock/showOnlyWhenLocked";
const STYLE: &str = "/background/widgets/clock/style";
const STYLE_LOCKED: &str = "/background/widgets/clock/styleLocked";
const DIAL: &str = "/background/widgets/clock/cookie/dialNumberStyle";
const FONT_FAMILY: &str = "/background/widgets/clock/digital/font/family";
const DEFAULT_FAMILY: &str = "Google Sans Flex";
const LABEL_START: i32 = 2;

fn choice(label: &str, icon: &'static str, value: &str) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value: Value::from(value),
    }
}

fn string_selection(
    page: &Page,
    parent: &impl Parent,
    pointer: &'static str,
    default: &str,
    choices: Vec<Choice>,
) -> Rc<Selection> {
    page.selection(
        parent,
        choices,
        pointer,
        Value::from(default),
        move |value| config::store_value(pointer, value),
    )
}

fn is_style_present(style: &str) -> bool {
    let read_style = |pointer| config::value_str(pointer).unwrap_or_else(|| "cookie".to_owned());
    let is_shown = !config::value_bool(SHOW_ONLY_WHEN_LOCKED, false) && read_style(STYLE) == style;
    is_shown || read_style(STYLE_LOCKED) == style
}

fn placement_row(
    page: &Page,
    parent: &gtk4::Box,
    enable_pointer: &'static str,
    placement_pointer: &'static str,
    default: bool,
    strategy: &str,
) {
    let row = page.row(parent);
    let switch = page.config_switch(&row, "check", &tr("Enable"), enable_pointer, default);
    switch.button.set_hexpand(false);
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row.append(&spacer);
    let selection = string_selection(
        page,
        &row,
        placement_pointer,
        strategy,
        vec![
            choice(&tr("Draggable"), "drag_pan", "free"),
            choice(&tr("Random"), "shuffle", "random"),
        ],
    );
    selection.root.set_hexpand(false);
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let appearance = DesktopAppearance::new();

    let wallpaper_section = page.section("wallpaper", &tr("Wallpaper"));
    page.tools_notice(
        &wallpaper_section,
        &[&tools::KDIALOG],
        &tr("the system file picker does not open"),
    );
    let preview = quick::preview(&page);
    preview.set_halign(gtk4::Align::Start);
    wallpaper_section.append(&preview);
    wallpaper_section.append(&quick::choose_wallpaper(&page));
    page.config_switch(
        &wallpaper_section,
        "ad",
        &tr("Use system file picker"),
        "/wallpaperSelector/useSystemFileDialog",
        false,
    );
    let fullscreen_switch = page.config_switch(
        &wallpaper_section,
        "fullscreen",
        &tr("Hide when a window is fullscreen"),
        "/background/hideWhenFullscreen",
        true,
    );
    page.tip(
        &fullscreen_switch.button,
        &tr("Saves a bit of resources while gaming or watching videos"),
    );

    let parallax_section = page.section("sync_alt", &tr("Parallax"));
    let vertical_row = page.uniform_row(&parallax_section);
    page.config_switch(
        &vertical_row,
        "unfold_more_double",
        &tr("Vertical"),
        "/background/parallax/vertical",
        false,
    );
    let auto_vertical_switch = page.config_switch(
        &vertical_row,
        "aspect_ratio",
        &tr("Vertical for tall wallpapers"),
        "/background/parallax/autoVertical",
        false,
    );
    page.tip(
        &auto_vertical_switch.button,
        &tr("Automatically pans vertically when the wallpaper is taller than it is wide"),
    );
    let dependencies_row = page.uniform_row(&parallax_section);
    page.config_switch(
        &dependencies_row,
        "counter_1",
        &tr("Depends on workspace"),
        "/background/parallax/enableWorkspace",
        true,
    );
    page.config_switch(
        &dependencies_row,
        "side_navigation",
        &tr("Depends on sidebars"),
        "/background/parallax/enableSidebar",
        true,
    );
    page.config_spin_scaled(
        &parallax_section,
        "loupe",
        &tr("Preferred wallpaper zoom (%)"),
        "/background/parallax/workspaceZoom",
        1.07,
        100.0,
        (10, 200),
        1,
    );
    let (movement_row, _) = page.config_spin_scaled(
        &parallax_section,
        "widgets",
        &tr("Widget movement (%)"),
        "/background/parallax/widgetsFactor",
        1.2,
        100.0,
        (0, 300),
        10,
    );
    page.tip(
        &movement_row,
        &tr("How much the clock and weather widgets follow the wallpaper's movement"),
    );

    let clock_section = page.section("clock_loader_40", &tr("Widget: Clock"));
    placement_row(
        &page,
        &clock_section,
        "/background/widgets/clock/enable",
        "/background/widgets/clock/placementStrategy",
        true,
        "random",
    );
    page.config_switch(
        &clock_section,
        "lock_clock",
        &tr("Show only when locked"),
        SHOW_ONLY_WHEN_LOCKED,
        false,
    );
    let styles_row = page.row(&clock_section);
    let unlocked_section = page.subsection(&styles_row, &tr("Clock style"), "");
    let clock_styles = || {
        vec![
            choice(&tr("Digital"), "timer_10", "digital"),
            choice(&tr("Cookie"), "cookie", "cookie"),
        ]
    };
    string_selection(&page, &unlocked_section, STYLE, "cookie", clock_styles());
    let unlocked_root = Page::subsection_root(&unlocked_section);
    unlocked_root.set_hexpand(true);
    let locked_section = page.subsection(&styles_row, &tr("Clock style (locked)"), "");
    string_selection(
        &page,
        &locked_section,
        STYLE_LOCKED,
        "cookie",
        clock_styles(),
    );
    Page::subsection_root(&locked_section).set_hexpand(false);

    let digital_section = page.subsection(
        &clock_section,
        &tr("Digital clock settings"),
        &tr("Font width and roundness settings are only available for some fonts like Google Sans Flex"),
    );
    let arrangement_row = page.uniform_row(&digital_section);
    page.config_switch(
        &arrangement_row,
        "vertical_distribute",
        &tr("Vertical"),
        "/background/widgets/clock/digital/vertical",
        false,
    );
    page.config_switch(
        &arrangement_row,
        "animation",
        &tr("Animate time change"),
        "/background/widgets/clock/digital/animateChange",
        true,
    );
    let extras_row = page.uniform_row(&digital_section);
    page.config_switch(
        &extras_row,
        "date_range",
        &tr("Show date"),
        "/background/widgets/clock/digital/showDate",
        true,
    );
    let adaptive_switch = page.config_switch(
        &extras_row,
        "activity_zone",
        &tr("Use adaptive alignment"),
        "/background/widgets/clock/digital/adaptiveAlignment",
        true,
    );
    page.tip(
        &adaptive_switch.button,
        &tr("Aligns the date and quote to left, center or right depending on its position on the screen."),
    );
    let family_row = page.row(&digital_section);
    let family_label = text::styled(&tr("Font family"));
    text::set_color(&family_label, "colSubtext");
    let family_label_box = Centered::new(&family_label);
    family_label_box.set_margin_start(LABEL_START);
    family_row.append(&family_label_box);
    let family_combo = page.combo(&family_row, "font_download");
    let show_family = Rc::new({
        let family_combo = Rc::downgrade(&family_combo);
        let appearance = Rc::downgrade(&appearance);
        move || {
            let (Some(family_combo), Some(appearance)) =
                (family_combo.upgrade(), appearance.upgrade())
            else {
                return;
            };
            let current_family =
                config::value_str(FONT_FAMILY).unwrap_or_else(|| DEFAULT_FAMILY.to_owned());
            family_combo.set_items_showing(
                &family_options(&current_family, appearance.family_names()),
                &current_family,
            );
        }
    });
    show_family();
    page.keep(appearance.watch({
        let show_family = show_family.clone();
        move || show_family()
    }));
    page.watch(FONT_FAMILY, move || show_family());
    family_combo.connect_activated({
        let family_combo = Rc::downgrade(&family_combo);
        move |index| {
            if let Some(name) = family_combo
                .upgrade()
                .and_then(|family_combo| family_combo.item(index))
            {
                config::store_value(FONT_FAMILY, Value::from(name));
            }
        }
    });
    for (icon, label, pointer, default, range, stops) in [
        (
            "format_bold",
            "Font weight",
            "/background/widgets/clock/digital/font/weight",
            350.0,
            (1.0, 1000.0),
            vec![350.0],
        ),
        (
            "format_size",
            "Font size",
            "/background/widgets/clock/digital/font/size",
            90.0,
            (50.0, 700.0),
            vec![90.0],
        ),
        (
            "fit_width",
            "Font width",
            "/background/widgets/clock/digital/font/width",
            100.0,
            (25.0, 125.0),
            vec![100.0],
        ),
        (
            "line_curve",
            "Font roundness",
            "/background/widgets/clock/digital/font/roundness",
            0.0,
            (0.0, 100.0),
            vec![1.0],
        ),
    ] {
        page.config_slider(
            &digital_section,
            icon,
            &tr(label),
            pointer,
            default,
            range,
            stops,
        );
    }

    let cookie_section = page.subsection(&clock_section, &tr("Cookie clock settings"), "");
    let sine_switch = page.config_switch(
        &cookie_section,
        "airwave",
        &tr("Use old sine wave cookie implementation"),
        "/background/widgets/clock/cookie/useSineCookie",
        false,
    );
    page.tip(
        &sine_switch.button,
        &tr("Looks a bit softer and more consistent with different number of sides,\nbut has less impressive morphing"),
    );
    page.config_spin(
        &cookie_section,
        "add_triangle",
        &tr("Sides"),
        "/background/widgets/clock/cookie/sides",
        14,
        (0, 40),
        1,
    );
    let rotate_switch = page.config_switch(
        &cookie_section,
        "autoplay",
        &tr("Constantly rotate"),
        "/background/widgets/clock/cookie/constantlyRotate",
        false,
    );
    page.tip(
        &rotate_switch.button,
        &tr("Makes the clock always rotate. This is extremely expensive\n(expect 50% usage on Intel UHD Graphics) and thus impractical."),
    );
    let marks_row = page.row(&cookie_section);
    let marks_switch = page.config_switch(
        &marks_row,
        "brightness_7",
        &tr("Hour marks"),
        "/background/widgets/clock/cookie/hourMarks",
        false,
    );
    page.tip(
        &marks_switch.button,
        &tr("Can only be turned on using the 'Dots' or 'Full' dial style for aesthetic reasons"),
    );
    let digits_switch = page.config_switch(
        &marks_row,
        "timer_10",
        &tr("Digits in the middle"),
        "/background/widgets/clock/cookie/timeIndicators",
        true,
    );
    page.tip(
        &digits_switch.button,
        &tr("Can't be turned on when using 'Numbers' dial style for aesthetic reasons"),
    );

    let mut cookie_parts = vec![Page::subsection_root(&cookie_section)];
    let mut second_hand_root = None;
    for (title, pointer, default, choices) in [
        (
            "Dial style",
            DIAL,
            "full",
            vec![
                choice("", "block", "none"),
                choice(&tr("Dots"), "graph_6", "dots"),
                choice(&tr("Full"), "history_toggle_off", "full"),
                choice(&tr("Numbers"), "counter_1", "numbers"),
            ],
        ),
        (
            "Hour hand",
            "/background/widgets/clock/cookie/hourHandStyle",
            "fill",
            vec![
                choice("", "block", "hide"),
                choice(&tr("Classic"), "radio", "classic"),
                choice(&tr("Hollow"), "circle", "hollow"),
                choice(&tr("Fill"), "eraser_size_5", "fill"),
            ],
        ),
        (
            "Minute hand",
            "/background/widgets/clock/cookie/minuteHandStyle",
            "medium",
            vec![
                choice("", "block", "hide"),
                choice(&tr("Classic"), "radio", "classic"),
                choice(&tr("Thin"), "line_end", "thin"),
                choice(&tr("Medium"), "eraser_size_2", "medium"),
                choice(&tr("Bold"), "eraser_size_4", "bold"),
            ],
        ),
        (
            "Second hand",
            "/background/widgets/clock/cookie/secondHandStyle",
            "dot",
            vec![
                choice("", "block", "hide"),
                choice(&tr("Classic"), "radio", "classic"),
                choice(&tr("Line"), "line_end", "line"),
                choice(&tr("Dot"), "adjust", "dot"),
            ],
        ),
        (
            "Date style",
            "/background/widgets/clock/cookie/dateStyle",
            "bubble",
            vec![
                choice("", "block", "hide"),
                choice(&tr("Bubble"), "bubble_chart", "bubble"),
                choice(&tr("Border"), "rotate_right", "border"),
                choice(&tr("Rect"), "rectangle", "rect"),
            ],
        ),
    ] {
        let is_second_hand = title == "Second hand";
        let tip = if is_second_hand {
            tr("Shown only while Date & Time › Seconds is on")
        } else {
            String::new()
        };
        let group = page.subsection(&clock_section, &tr(title), &tip);
        string_selection(&page, &group, pointer, default, choices);
        if is_second_hand {
            second_hand_root = Some(Page::subsection_root(&group));
        }
        cookie_parts.push(Page::subsection_root(&group));
    }

    let quote_section = page.subsection(&clock_section, &tr("Quote"), "");
    page.config_switch(
        &quote_section,
        "check",
        &tr("Enable"),
        "/background/widgets/clock/quote/enable",
        false,
    );
    page.config_text(
        &quote_section,
        Style::Filled,
        &tr("Quote"),
        "/background/widgets/clock/quote/text",
        "",
    );

    let update_clock_controls = {
        let digital_root = Page::subsection_root(&digital_section);
        move || {
            unlocked_root.set_visible(!config::value_bool(SHOW_ONLY_WHEN_LOCKED, false));
            digital_root.set_visible(is_style_present("digital"));
            let is_cookie_shown = is_style_present("cookie");
            for part in &cookie_parts {
                part.set_visible(is_cookie_shown);
            }
            let dial = config::value_str(DIAL).unwrap_or_else(|| "full".to_owned());
            marks_switch.set_enabled(dial == "dots" || dial == "full");
            digits_switch.set_enabled(dial != "numbers");
            if let Some(second_hand_root) = &second_hand_root {
                let has_seconds = config::value_bool("/time/secondPrecision", false);
                second_hand_root.set_sensitive(has_seconds);
                second_hand_root.set_opacity(if has_seconds { 1.0 } else { 0.4 });
            }
        }
    };
    let update_clock_controls = Rc::new(update_clock_controls);
    update_clock_controls();
    page.watch(CLOCK, {
        let update_clock_controls = update_clock_controls.clone();
        move || update_clock_controls()
    });
    page.watch("/time/secondPrecision", move || update_clock_controls());

    let weather_section = page.section("weather_mix", &tr("Widget: Weather"));
    placement_row(
        &page,
        &weather_section,
        "/background/widgets/weather/enable",
        "/background/widgets/weather/placementStrategy",
        false,
        "free",
    );

    page.keep(appearance);
    page
}

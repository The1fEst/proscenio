use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::settings::content::{Choice, Context, Page, Parent, Style};
use crate::panels::settings::pages::fonts::family_options;
use crate::panels::settings::pages::quick;
use crate::services::appearance::DesktopAppearance;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::text;

const CLOCK: &str = "/background/widgets/clock";
const SHOW_ONLY_WHEN_LOCKED: &str = "/background/widgets/clock/showOnlyWhenLocked";
const STYLE: &str = "/background/widgets/clock/style";
const STYLE_LOCKED: &str = "/background/widgets/clock/styleLocked";
const DIAL: &str = "/background/widgets/clock/cookie/dialNumberStyle";
const FONT_FAMILY: &str = "/background/widgets/clock/digital/font/family";
const DEFAULT_FAMILY: &str = "Google Sans Flex";
const LABEL_START: i32 = 2;
const WALLPAPER_SPACING: i32 = 5;

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

fn style_present(style: &str) -> bool {
    let current = |pointer| config::value_str(pointer).unwrap_or_else(|| "cookie".to_owned());
    let shown = !config::value_bool(SHOW_ONLY_WHEN_LOCKED, false) && current(STYLE) == style;
    shown || current(STYLE_LOCKED) == style
}

fn placement_row(
    page: &Page,
    parent: &gtk4::Box,
    enable: &'static str,
    placement: &'static str,
    default: bool,
    strategy: &str,
) {
    let row = page.row(parent);
    let switch = page.config_switch(&row, "check", &tr("Enable"), enable, default);
    switch.button.set_hexpand(false);
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row.append(&spacer);
    let selection = string_selection(
        page,
        &row,
        placement,
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

    let wallpaper = page.section("wallpaper", &tr("Wallpaper"));
    page.tools_notice(
        &wallpaper,
        &[&tools::KDIALOG],
        &tr("the system file picker does not open"),
    );
    let top = Row::new(WALLPAPER_SPACING);
    top.append(&quick::preview(&page));
    let controls = gtk4::Box::new(gtk4::Orientation::Vertical, WALLPAPER_SPACING);
    controls.set_hexpand(true);
    controls.append(&quick::choose_wallpaper(&page));
    page.config_switch(
        &controls,
        "ad",
        &tr("Use system file picker"),
        "/wallpaperSelector/useSystemFileDialog",
        false,
    );
    let fullscreen = page.config_switch(
        &controls,
        "fullscreen",
        &tr("Hide when a window is fullscreen"),
        "/background/hideWhenFullscreen",
        true,
    );
    page.tip(
        &fullscreen.button,
        &tr("Saves a bit of resources while gaming or watching videos"),
    );
    top.append(&controls);
    wallpaper.append(&top);

    let parallax = page.section("sync_alt", &tr("Parallax"));
    let vertical = page.uniform_row(&parallax);
    page.config_switch(
        &vertical,
        "unfold_more_double",
        &tr("Vertical"),
        "/background/parallax/vertical",
        false,
    );
    let auto_vertical = page.config_switch(
        &vertical,
        "aspect_ratio",
        &tr("Vertical for tall wallpapers"),
        "/background/parallax/autoVertical",
        false,
    );
    page.tip(
        &auto_vertical.button,
        &tr("Automatically pans vertically when the wallpaper is taller than it is wide"),
    );
    let follows = page.uniform_row(&parallax);
    page.config_switch(
        &follows,
        "counter_1",
        &tr("Depends on workspace"),
        "/background/parallax/enableWorkspace",
        true,
    );
    page.config_switch(
        &follows,
        "side_navigation",
        &tr("Depends on sidebars"),
        "/background/parallax/enableSidebar",
        true,
    );
    page.config_spin_scaled(
        &parallax,
        "loupe",
        &tr("Preferred wallpaper zoom (%)"),
        "/background/parallax/workspaceZoom",
        1.07,
        100.0,
        (10, 200),
        1,
    );
    let (movement, _) = page.config_spin_scaled(
        &parallax,
        "widgets",
        &tr("Widget movement (%)"),
        "/background/parallax/widgetsFactor",
        1.2,
        100.0,
        (0, 300),
        10,
    );
    page.tip(
        &movement,
        &tr("How much the clock and weather widgets follow the wallpaper's movement"),
    );

    let clock = page.section("clock_loader_40", &tr("Widget: Clock"));
    placement_row(
        &page,
        &clock,
        "/background/widgets/clock/enable",
        "/background/widgets/clock/placementStrategy",
        true,
        "random",
    );
    page.config_switch(
        &clock,
        "lock_clock",
        &tr("Show only when locked"),
        SHOW_ONLY_WHEN_LOCKED,
        false,
    );
    let styles = page.row(&clock);
    let unlocked = page.subsection(&styles, &tr("Clock style"), "");
    let clock_styles = || {
        vec![
            choice(&tr("Digital"), "timer_10", "digital"),
            choice(&tr("Cookie"), "cookie", "cookie"),
        ]
    };
    string_selection(&page, &unlocked, STYLE, "cookie", clock_styles());
    let unlocked = Page::subsection_root(&unlocked);
    unlocked.set_hexpand(true);
    let locked = page.subsection(&styles, &tr("Clock style (locked)"), "");
    string_selection(&page, &locked, STYLE_LOCKED, "cookie", clock_styles());
    Page::subsection_root(&locked).set_hexpand(false);

    let digital = page.subsection(
        &clock,
        &tr("Digital clock settings"),
        &tr("Font width and roundness settings are only available for some fonts like Google Sans Flex"),
    );
    let arrangement = page.uniform_row(&digital);
    page.config_switch(
        &arrangement,
        "vertical_distribute",
        &tr("Vertical"),
        "/background/widgets/clock/digital/vertical",
        false,
    );
    page.config_switch(
        &arrangement,
        "animation",
        &tr("Animate time change"),
        "/background/widgets/clock/digital/animateChange",
        true,
    );
    let extras = page.uniform_row(&digital);
    page.config_switch(
        &extras,
        "date_range",
        &tr("Show date"),
        "/background/widgets/clock/digital/showDate",
        true,
    );
    let adaptive = page.config_switch(
        &extras,
        "activity_zone",
        &tr("Use adaptive alignment"),
        "/background/widgets/clock/digital/adaptiveAlignment",
        true,
    );
    page.tip(
        &adaptive.button,
        &tr("Aligns the date and quote to left, center or right depending on its position on the screen."),
    );
    let family_row = page.row(&digital);
    let family_label = text::styled(&tr("Font family"));
    text::set_color(&family_label, "colSubtext");
    let family_label = Centred::new(&family_label);
    family_label.set_margin_start(LABEL_START);
    family_row.append(&family_label);
    let family = page.combo(&family_row, "font_download");
    let show_family = Rc::new({
        let family = Rc::downgrade(&family);
        let appearance = Rc::downgrade(&appearance);
        move || {
            let (Some(family), Some(appearance)) = (family.upgrade(), appearance.upgrade()) else {
                return;
            };
            let current =
                config::value_str(FONT_FAMILY).unwrap_or_else(|| DEFAULT_FAMILY.to_owned());
            family.set_items_showing(
                &family_options(&current, appearance.family_names()),
                &current,
            );
        }
    });
    show_family();
    page.keep(appearance.watch({
        let show_family = show_family.clone();
        move || show_family()
    }));
    page.watch(FONT_FAMILY, move || show_family());
    family.connect_activated({
        let family = Rc::downgrade(&family);
        move |index| {
            if let Some(name) = family.upgrade().and_then(|family| family.item(index)) {
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
        page.config_slider(&digital, icon, &tr(label), pointer, default, range, stops);
    }

    let cookie = page.subsection(&clock, &tr("Cookie clock settings"), "");
    let sine = page.config_switch(
        &cookie,
        "airwave",
        &tr("Use old sine wave cookie implementation"),
        "/background/widgets/clock/cookie/useSineCookie",
        false,
    );
    page.tip(
        &sine.button,
        &tr("Looks a bit softer and more consistent with different number of sides,\nbut has less impressive morphing"),
    );
    page.config_spin(
        &cookie,
        "add_triangle",
        &tr("Sides"),
        "/background/widgets/clock/cookie/sides",
        14,
        (0, 40),
        1,
    );
    let rotate = page.config_switch(
        &cookie,
        "autoplay",
        &tr("Constantly rotate"),
        "/background/widgets/clock/cookie/constantlyRotate",
        false,
    );
    page.tip(
        &rotate.button,
        &tr("Makes the clock always rotate. This is extremely expensive\n(expect 50% usage on Intel UHD Graphics) and thus impractical."),
    );
    let marks_row = page.row(&cookie);
    let marks = page.config_switch(
        &marks_row,
        "brightness_7",
        &tr("Hour marks"),
        "/background/widgets/clock/cookie/hourMarks",
        false,
    );
    page.tip(
        &marks.button,
        &tr("Can only be turned on using the 'Dots' or 'Full' dial style for aesthetic reasons"),
    );
    let digits = page.config_switch(
        &marks_row,
        "timer_10",
        &tr("Digits in the middle"),
        "/background/widgets/clock/cookie/timeIndicators",
        true,
    );
    page.tip(
        &digits.button,
        &tr("Can't be turned on when using 'Numbers' dial style for aesthetic reasons"),
    );

    let mut cookie_parts = vec![Page::subsection_root(&cookie)];
    let mut second_hand = None;
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
        let seconds = title == "Second hand";
        let tip = if seconds {
            tr("Shown only while Date & Time › Seconds is on")
        } else {
            String::new()
        };
        let group = page.subsection(&clock, &tr(title), &tip);
        string_selection(&page, &group, pointer, default, choices);
        if seconds {
            second_hand = Some(Page::subsection_root(&group));
        }
        cookie_parts.push(Page::subsection_root(&group));
    }

    let quote = page.subsection(&clock, &tr("Quote"), "");
    page.config_switch(
        &quote,
        "check",
        &tr("Enable"),
        "/background/widgets/clock/quote/enable",
        false,
    );
    page.config_text(
        &quote,
        Style::Filled,
        &tr("Quote"),
        "/background/widgets/clock/quote/text",
        "",
    );

    let follow = {
        let digital = Page::subsection_root(&digital);
        move || {
            unlocked.set_visible(!config::value_bool(SHOW_ONLY_WHEN_LOCKED, false));
            digital.set_visible(style_present("digital"));
            let cookie_shown = style_present("cookie");
            for part in &cookie_parts {
                part.set_visible(cookie_shown);
            }
            let dial = config::value_str(DIAL).unwrap_or_else(|| "full".to_owned());
            marks.set_enabled(dial == "dots" || dial == "full");
            digits.set_enabled(dial != "numbers");
            if let Some(group) = &second_hand {
                let seconds = config::value_bool("/time/secondPrecision", false);
                group.set_sensitive(seconds);
                group.set_opacity(if seconds { 1.0 } else { 0.4 });
            }
        }
    };
    let follow = Rc::new(follow);
    follow();
    page.watch(CLOCK, {
        let follow = follow.clone();
        move || follow()
    });
    page.watch("/time/secondPrecision", move || follow());

    let weather = page.section("weather_mix", &tr("Widget: Weather"));
    placement_row(
        &page,
        &weather,
        "/background/widgets/weather/enable",
        "/background/widgets/weather/placementStrategy",
        false,
        "free",
    );

    page.keep(appearance);
    page
}

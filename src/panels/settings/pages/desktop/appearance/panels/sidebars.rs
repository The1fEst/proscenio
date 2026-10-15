use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::desktop::appearance::panels::choice;

const TOGGLES_STYLE: &str = "/sidebar/quickToggles/style";
const SLIDERS: &str = "/sidebar/quickSliders/enable";
const CORNER_OPEN: &str = "/sidebar/cornerOpen/enable";
const CLICKLESS: &str = "/sidebar/cornerOpen/clickless";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let sidebars_section = page.section("", "");
    let toggles_section = page.subsection(
        &sidebars_section,
        &tr("Quick toggles"),
        &tr(
            "Which toggles are shown, their size and their order are edited in the sidebar itself, with its edit mode",
        ),
    );
    let style_selection = page.selection(
        &toggles_section,
        vec![
            choice(&tr("Classic"), "password_2", "classic"),
            choice(&tr("Android"), "action_key", "android"),
        ],
        TOGGLES_STYLE,
        Value::from("android"),
        |value| config::store_value(TOGGLES_STYLE, value),
    );
    style_selection.root.set_hexpand(false);
    let (columns_row, columns_spin) = page.config_spin(
        &toggles_section,
        "splitscreen_left",
        &tr("Columns"),
        "/sidebar/quickToggles/android/columns",
        5,
        (1, 8),
        1,
    );
    let sliders_section = page.subsection(&sidebars_section, &tr("Sliders"), "");
    page.config_switch(&sliders_section, "check", &tr("Enable"), SLIDERS, false);
    let slider_switches: Vec<_> = [
        (
            "brightness_6",
            "Brightness",
            "/sidebar/quickSliders/showBrightness",
            true,
        ),
        (
            "volume_up",
            "Volume",
            "/sidebar/quickSliders/showVolume",
            true,
        ),
        ("mic", "Microphone", "/sidebar/quickSliders/showMic", false),
    ]
    .into_iter()
    .map(|(icon, label, pointer, default)| {
        page.config_switch(&sliders_section, icon, &tr(label), pointer, default)
    })
    .collect();

    let corners_section = page.subsection(
        &sidebars_section,
        &tr("Corner open"),
        &tr(
            "Allows you to open sidebars by clicking or hovering screen corners regardless of bar position",
        ),
    );
    let enable_row = page.uniform_row(&corners_section);
    page.config_switch(&enable_row, "check", &tr("Enable"), CORNER_OPEN, true);
    let clickless_switch = page.config_switch(
        &corners_section,
        "highlight_mouse_cursor",
        &tr("Hover to trigger"),
        CLICKLESS,
        false,
    );
    page.tip(
        &clickless_switch.button,
        &tr("When this is off you'll have to click"),
    );
    let corner_end_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    corner_end_row.set_halign(gtk4::Align::Start);
    corners_section.append(&corner_end_row);
    let corner_end_switch = page.config_switch(
        &corner_end_row,
        "",
        &tr("Force hover open at absolute corner"),
        "/sidebar/cornerOpen/clicklessCornerEnd",
        true,
    );
    corner_end_switch.button.set_hexpand(false);
    page.tip(
        &corner_end_switch.button,
        &tr(
            "When the previous option is off and this is on,\nyou can still hover the corner's end to open sidebar,\nand the remaining area can be used for volume/brightness scroll",
        ),
    );
    let (offset_row, offset_spin) = page.config_spin(
        &corner_end_row,
        "arrow_cool_down",
        &tr("with vertical offset"),
        "/sidebar/cornerOpen/clicklessCornerVerticalOffset",
        1,
        (0, 20),
        1,
    );
    offset_row.set_margin_start(0);
    offset_row.set_margin_end(0);
    offset_row.set_hexpand(false);
    page.tip(
        &offset_row,
        &tr(
            "Why this is cool:\nFor non-0 values, it won't trigger when you reach the\nscreen corner along the horizontal edge, but it will when\nyou do along the vertical edge",
        ),
    );
    let corner_options_row = page.uniform_row(&corners_section);
    let bottom_switch = page.config_switch(
        &corner_options_row,
        "vertical_align_bottom",
        &tr("Place at bottom"),
        "/sidebar/cornerOpen/bottom",
        false,
    );
    page.tip(
        &bottom_switch.button,
        &tr("Place the corners to trigger at the bottom"),
    );
    let value_scroll_switch = page.config_switch(
        &corner_options_row,
        "unfold_more_double",
        &tr("Value scroll"),
        "/sidebar/cornerOpen/valueScroll",
        true,
    );
    page.tip(&value_scroll_switch.button, &tr("Brightness and volume"));
    let visualize_switch = page.config_switch(
        &corners_section,
        "visibility",
        &tr("Visualize region"),
        "/sidebar/cornerOpen/visualize",
        false,
    );
    let region_row = page.row(&corners_section);
    let (region_width_row, region_width_spin) = page.config_spin(
        &region_row,
        "arrow_range",
        &tr("Region width"),
        "/sidebar/cornerOpen/cornerRegionWidth",
        250,
        (1, 300),
        1,
    );
    let (region_height_row, region_height_spin) = page.config_spin(
        &region_row,
        "height",
        &tr("Region height"),
        "/sidebar/cornerOpen/cornerRegionHeight",
        5,
        (1, 300),
        1,
    );

    let update_dependent_controls = move || {
        let toggles_style =
            config::value_str(TOGGLES_STYLE).unwrap_or_else(|| "android".to_owned());
        Page::set_spin_row_enabled(&columns_row, &columns_spin, toggles_style == "android");
        let are_sliders_on = config::value_bool(SLIDERS, false);
        for switch in &slider_switches {
            switch.set_enabled(are_sliders_on);
        }
        let are_corners_on = config::value_bool(CORNER_OPEN, true);
        let is_click_needed = are_corners_on && !config::value_bool(CLICKLESS, false);
        clickless_switch.set_enabled(are_corners_on);
        corner_end_switch.set_enabled(is_click_needed);
        Page::set_spin_row_enabled(&offset_row, &offset_spin, is_click_needed);
        bottom_switch.set_enabled(are_corners_on);
        value_scroll_switch.set_enabled(are_corners_on);
        visualize_switch.set_enabled(are_corners_on);
        Page::set_spin_row_enabled(&region_width_row, &region_width_spin, are_corners_on);
        Page::set_spin_row_enabled(&region_height_row, &region_height_spin, are_corners_on);
    };
    update_dependent_controls();
    page.watch("/sidebar", update_dependent_controls);
    page
}

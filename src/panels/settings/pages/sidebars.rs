use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::panels::choice;

const TOGGLES_STYLE: &str = "/sidebar/quickToggles/style";
const SLIDERS: &str = "/sidebar/quickSliders/enable";
const CORNER_OPEN: &str = "/sidebar/cornerOpen/enable";
const CLICKLESS: &str = "/sidebar/cornerOpen/clickless";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let sidebars = page.section("", "");
    let toggles = page.subsection(
        &sidebars,
        &tr("Quick toggles"),
        &tr(
            "Which toggles are shown, their size and their order are edited in the sidebar itself, with its edit mode",
        ),
    );
    let styles = page.selection(
        &toggles,
        vec![
            choice(&tr("Classic"), "password_2", "classic"),
            choice(&tr("Android"), "action_key", "android"),
        ],
        TOGGLES_STYLE,
        Value::from("android"),
        |value| config::store_value(TOGGLES_STYLE, value),
    );
    styles.root.set_hexpand(false);
    let columns = page.config_spin(
        &toggles,
        "splitscreen_left",
        &tr("Columns"),
        "/sidebar/quickToggles/android/columns",
        5,
        (1, 8),
        1,
    );
    let sliders = page.subsection(&sidebars, &tr("Sliders"), "");
    page.config_switch(&sliders, "check", &tr("Enable"), SLIDERS, false);
    let shown_sliders: Vec<_> = [
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
        page.config_switch(&sliders, icon, &tr(label), pointer, default)
    })
    .collect();

    let corners = page.subsection(
        &sidebars,
        &tr("Corner open"),
        &tr(
            "Allows you to open sidebars by clicking or hovering screen corners regardless of bar position",
        ),
    );
    let enable_row = page.uniform_row(&corners);
    page.config_switch(&enable_row, "check", &tr("Enable"), CORNER_OPEN, true);
    let clickless = page.config_switch(
        &corners,
        "highlight_mouse_cursor",
        &tr("Hover to trigger"),
        CLICKLESS,
        false,
    );
    page.tip(
        &clickless.button,
        &tr("When this is off you'll have to click"),
    );
    let corner_end_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    corner_end_row.set_halign(gtk4::Align::Start);
    corners.append(&corner_end_row);
    let corner_end = page.config_switch(
        &corner_end_row,
        "",
        &tr("Force hover open at absolute corner"),
        "/sidebar/cornerOpen/clicklessCornerEnd",
        true,
    );
    corner_end.button.set_hexpand(false);
    page.tip(
        &corner_end.button,
        &tr(
            "When the previous option is off and this is on,\nyou can still hover the corner's end to open sidebar,\nand the remaining area can be used for volume/brightness scroll",
        ),
    );
    let offset = page.config_spin(
        &corner_end_row,
        "arrow_cool_down",
        &tr("with vertical offset"),
        "/sidebar/cornerOpen/clicklessCornerVerticalOffset",
        1,
        (0, 20),
        1,
    );
    offset.0.set_margin_start(0);
    offset.0.set_margin_end(0);
    offset.0.set_hexpand(false);
    page.tip(
        &offset.0,
        &tr(
            "Why this is cool:\nFor non-0 values, it won't trigger when you reach the\nscreen corner along the horizontal edge, but it will when\nyou do along the vertical edge",
        ),
    );
    let placing = page.uniform_row(&corners);
    let bottom = page.config_switch(
        &placing,
        "vertical_align_bottom",
        &tr("Place at bottom"),
        "/sidebar/cornerOpen/bottom",
        false,
    );
    page.tip(
        &bottom.button,
        &tr("Place the corners to trigger at the bottom"),
    );
    let value_scroll = page.config_switch(
        &placing,
        "unfold_more_double",
        &tr("Value scroll"),
        "/sidebar/cornerOpen/valueScroll",
        true,
    );
    page.tip(&value_scroll.button, &tr("Brightness and volume"));
    let visualize = page.config_switch(
        &corners,
        "visibility",
        &tr("Visualize region"),
        "/sidebar/cornerOpen/visualize",
        false,
    );
    let region = page.row(&corners);
    let region_width = page.config_spin(
        &region,
        "arrow_range",
        &tr("Region width"),
        "/sidebar/cornerOpen/cornerRegionWidth",
        250,
        (1, 300),
        1,
    );
    let region_height = page.config_spin(
        &region,
        "height",
        &tr("Region height"),
        "/sidebar/cornerOpen/cornerRegionHeight",
        5,
        (1, 300),
        1,
    );

    let follow_sidebar = move || {
        let style = config::value_str(TOGGLES_STYLE).unwrap_or_else(|| "android".to_owned());
        Page::set_spin_row_enabled(&columns.0, &columns.1, style == "android");
        let sliders_on = config::value_bool(SLIDERS, false);
        for switch in &shown_sliders {
            switch.set_enabled(sliders_on);
        }
        let corners_on = config::value_bool(CORNER_OPEN, true);
        let clicking = corners_on && !config::value_bool(CLICKLESS, false);
        clickless.set_enabled(corners_on);
        corner_end.set_enabled(clicking);
        Page::set_spin_row_enabled(&offset.0, &offset.1, clicking);
        bottom.set_enabled(corners_on);
        value_scroll.set_enabled(corners_on);
        visualize.set_enabled(corners_on);
        Page::set_spin_row_enabled(&region_width.0, &region_width.1, corners_on);
        Page::set_spin_row_enabled(&region_height.0, &region_height.1, corners_on);
    };
    follow_sidebar();
    page.watch("/sidebar", follow_sidebar);
    page
}

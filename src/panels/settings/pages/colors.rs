use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::theming::{colors, switchwall};

const FIELD_MARGIN: i32 = 8;
const PERCENT: f64 = 100.0;
const ENABLE_TRANSPARENCY: &str = "/appearance/transparency/enable";
const AUTOMATIC_TRANSPARENCY: &str = "/appearance/transparency/automatic";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let palette = page.section("palette", &tr("Palette"));
    add_palette(&page, &palette);
    let shell = page.section("layers", &tr("Shell surfaces"));
    add_shell_surfaces(&page, &shell);

    let colors = page.section("colors", &tr("Color generation"));
    let themed = page.subsection(&colors, &tr("What gets themed"), "");
    page.tools_notice(
        &themed,
        &[&tools::MATUGEN],
        &tr("apps themed through matugen templates keep their colors"),
    );
    page.config_switch(
        &themed,
        "hardware",
        &tr("Shell & utilities"),
        "/appearance/wallpaperTheming/enableAppsAndShell",
        true,
    );
    for (icon, label, pointer) in [
        (
            "tv_options_input_settings",
            "Qt apps",
            "/appearance/wallpaperTheming/enableQtApps",
        ),
        (
            "terminal",
            "Terminal",
            "/appearance/wallpaperTheming/enableTerminal",
        ),
    ] {
        let switch = page.config_switch(&themed, icon, &tr(label), pointer, true);
        page.tip(
            &switch.button,
            &tr("Shell & utilities theming must also be enabled"),
        );
    }
    let terminal = page.subsection(
        &colors,
        &tr("Terminal colors"),
        &tr("Ignored if terminal theming is not enabled"),
    );
    page.config_switch(
        &terminal,
        "dark_mode",
        &tr("Force dark mode in terminal"),
        "/appearance/wallpaperTheming/terminalGenerationProps/forceDarkMode",
        false,
    );
    page.config_spin_scaled(
        &terminal,
        "invert_colors",
        &tr("Harmony (%)"),
        "/appearance/wallpaperTheming/terminalGenerationProps/harmony",
        colors::HARMONY,
        100.0,
        (0, 100),
        10,
    );
    page.config_spin(
        &terminal,
        "gradient",
        &tr("Harmonize threshold"),
        "/appearance/wallpaperTheming/terminalGenerationProps/harmonizeThreshold",
        colors::HARMONIZE_THRESHOLD as i64,
        (0, 100),
        10,
    );
    page.config_spin_scaled(
        &terminal,
        "format_color_text",
        &tr("Foreground boost (%)"),
        "/appearance/wallpaperTheming/terminalGenerationProps/termFgBoost",
        colors::TERM_FG_BOOST,
        100.0,
        (0, 100),
        10,
    );
    page
}

fn choice(label: &str, value: &str) -> Choice {
    Choice {
        label: label.to_owned(),
        icon: "",
        value: Value::from(value),
    }
}

pub(super) fn add_palette(page: &Page, parent: &gtk4::Box) {
    page.selection(
        parent,
        vec![
            choice(&tr("Auto"), "auto"),
            choice(&tr("Content"), "scheme-content"),
            choice(&tr("Expressive"), "scheme-expressive"),
            choice(&tr("Fidelity"), "scheme-fidelity"),
            choice(&tr("Fruit Salad"), "scheme-fruit-salad"),
            choice(&tr("Monochrome"), "scheme-monochrome"),
            choice(&tr("Neutral"), "scheme-neutral"),
            choice(&tr("Rainbow"), "scheme-rainbow"),
            choice(&tr("Tonal Spot"), "scheme-tonal-spot"),
        ],
        "/appearance/palette/type",
        Value::from("auto"),
        |value| {
            config::store_value("/appearance/palette/type", value);
            switchwall::detach(&["--noswitch"]);
        },
    );

    let accent = page.text_field(
        parent,
        Style::Outlined,
        &tr("Accent color (e.g. #8caaee, empty to use the wallpaper's)"),
        || config::value_str("/appearance/palette/accentColor").unwrap_or_default(),
        |text| {
            config::store_value("/appearance/palette/accentColor", Value::from(text.trim()));
            switchwall::detach(&["--noswitch"]);
        },
    );
    accent.root.set_margin_start(FIELD_MARGIN);
    accent.root.set_margin_end(FIELD_MARGIN);
    page.refresh_text_on("/appearance/palette/accentColor", &accent);
}

pub(super) fn add_shell_surfaces(page: &Page, parent: &gtk4::Box) {
    let tint = page.config_switch(
        parent,
        "invert_colors",
        &tr("Extra background tint"),
        "/appearance/extraBackgroundTint",
        true,
    );
    page.tip(
        &tint.button,
        &tr("Tints backgrounds of shell surfaces more strongly with the accent color"),
    );
    page.config_switch(
        parent,
        "ev_shadow",
        &tr("Transparency"),
        ENABLE_TRANSPARENCY,
        false,
    );
    let automatic = page.config_switch(
        parent,
        "auto_awesome",
        &tr("Automatic transparency values"),
        AUTOMATIC_TRANSPARENCY,
        true,
    );
    page.tip(
        &automatic.button,
        &tr("Derives the values below from the wallpaper instead of using them as entered"),
    );
    let amounts = page.uniform_row(parent);
    let (background_row, background) = page.config_spin_scaled(
        &amounts,
        "background_replace",
        &tr("Background (%)"),
        "/appearance/transparency/backgroundTransparency",
        0.11,
        PERCENT,
        (0, 100),
        1,
    );
    let (content_row, content) = page.config_spin_scaled(
        &amounts,
        "select_window",
        &tr("Content (%)"),
        "/appearance/transparency/contentTransparency",
        0.57,
        PERCENT,
        (0, 100),
        1,
    );
    page.tip(
        &content_row,
        &tr("Affects how surfaces are layered even when transparency is off"),
    );
    let follow = move || {
        let enabled = config::value_bool(ENABLE_TRANSPARENCY, false);
        let automatic = config::value_bool(AUTOMATIC_TRANSPARENCY, true);
        Page::set_spin_row_enabled(&background_row, &background, enabled && !automatic);
        Page::set_spin_row_enabled(&content_row, &content, !automatic);
    };
    follow();
    let follow = Rc::new(follow);
    for pointer in [ENABLE_TRANSPARENCY, AUTOMATIC_TRANSPARENCY] {
        let follow = follow.clone();
        page.watch(pointer, move || follow());
    }
}

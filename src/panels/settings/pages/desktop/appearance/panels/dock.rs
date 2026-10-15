use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};

const HOVER_TO_REVEAL: &str = "/dock/hoverToReveal";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let dock_section = page.section("", "");
    page.config_switch(&dock_section, "check", &tr("Enable"), "/dock/enable", false);
    let reveal_section = page.subsection(&dock_section, &tr("Reveal"), "");
    let reveal_row = page.uniform_row(&reveal_section);
    page.config_switch(
        &reveal_row,
        "highlight_mouse_cursor",
        &tr("Hover to reveal"),
        HOVER_TO_REVEAL,
        true,
    );
    page.config_switch(
        &reveal_row,
        "keep",
        &tr("Pinned on startup"),
        "/dock/pinnedOnStartup",
        false,
    );
    let (hover_region_row, hover_region_spin) = page.config_spin(
        &reveal_section,
        "highlight_mouse_cursor",
        &tr("Hover region height (px)"),
        "/dock/hoverRegionHeight",
        2,
        (1, 50),
        1,
    );
    let looks_section = page.subsection(&dock_section, &tr("Looks"), "");
    page.config_switch(
        &looks_section,
        "colors",
        &tr("Tint app icons"),
        "/dock/monochromeIcons",
        true,
    );
    page.config_spin(
        &looks_section,
        "apps",
        &tr("Icon size (px)"),
        "/dock/iconSize",
        35,
        (20, 120),
        5,
    );
    let ignored_section = page.subsection(
        &dock_section,
        &tr("Ignored apps"),
        &tr("Comma-separated regexes. Matching windows won't get a dock entry."),
    );
    page.config_list(
        &ignored_section,
        &tr("e.g. ^steam_app_.*"),
        "/dock/ignoredAppRegexes",
        &[],
    );

    let update_hover_region_row = move || {
        Page::set_spin_row_enabled(
            &hover_region_row,
            &hover_region_spin,
            config::value_bool(HOVER_TO_REVEAL, true),
        );
    };
    update_hover_region_row();
    page.watch("/dock", update_hover_region_row);
    page
}

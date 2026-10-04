use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};

const HOVER_TO_REVEAL: &str = "/dock/hoverToReveal";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let dock = page.section("", "");
    page.config_switch(&dock, "check", &tr("Enable"), "/dock/enable", false);
    let reveal = page.subsection(&dock, &tr("Reveal"), "");
    let reveal_row = page.uniform_row(&reveal);
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
    let hover_region = page.config_spin(
        &reveal,
        "highlight_mouse_cursor",
        &tr("Hover region height (px)"),
        "/dock/hoverRegionHeight",
        2,
        (1, 50),
        1,
    );
    let looks = page.subsection(&dock, &tr("Looks"), "");
    page.config_switch(
        &looks,
        "colors",
        &tr("Tint app icons"),
        "/dock/monochromeIcons",
        true,
    );
    page.config_spin(
        &looks,
        "height",
        &tr("Height (px)"),
        "/dock/height",
        60,
        (30, 150),
        5,
    );
    let pinned = page.subsection(
        &dock,
        &tr("Pinned apps"),
        &tr("Comma-separated desktop entry IDs, in the order they should appear"),
    );
    page.config_list(
        &pinned,
        &tr("e.g. org.kde.dolphin, kitty"),
        "/dock/pinnedApps",
        &config::DOCK_PINNED_APPS,
    );
    let ignored = page.subsection(
        &dock,
        &tr("Ignored apps"),
        &tr("Comma-separated regexes. Matching windows won't get a dock entry."),
    );
    page.config_list(
        &ignored,
        &tr("e.g. ^steam_app_.*"),
        "/dock/ignoredAppRegexes",
        &[],
    );

    let follow_dock = move || {
        Page::set_spin_row_enabled(
            &hover_region.0,
            &hover_region.1,
            config::value_bool(HOVER_TO_REVEAL, true),
        );
    };
    follow_dock();
    page.watch("/dock", follow_dock);
    page
}

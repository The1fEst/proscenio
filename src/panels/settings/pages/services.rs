use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};

const ENABLE_CHECK: &str = "/updates/enableCheck";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let resources = page.section("memory", &tr("Resources"));
    page.config_spin(
        &resources,
        "av_timer",
        &tr("Polling interval (ms)"),
        "/resources/updateInterval",
        3000,
        (100, 10000),
        100,
    );

    let conflicts = page.section("block", &tr("Conflict killer"));
    let kill = page.config_switch(
        &conflicts,
        "notifications_off",
        &tr("Kill notification daemons without asking"),
        "/conflictKiller/autoKillNotificationDaemons",
        false,
    );
    page.tip(
        &kill.button,
        &tr(
            "Conflicting daemons like dunst or mako are killed silently instead of showing a dialog",
        ),
    );

    let updates = page.section("deployed_code_update", &tr("System updates (Arch only)"));
    let check = page.config_switch(
        &updates,
        "check",
        &tr("Enable update checks"),
        ENABLE_CHECK,
        true,
    );
    page.tip(
        &check.button,
        &tr(
            "Counts the packages pacman could upgrade.\nThe bar shows a button once the count passes the threshold below.",
        ),
    );
    let missing = page.notice(
        &updates,
        "info",
        &tr(
            "checkupdates is not installed, so nothing is counted. It comes with the pacman-contrib package.",
        ),
    );
    let (interval_row, interval) = page.config_spin(
        &updates,
        "av_timer",
        &tr("Check interval (mins)"),
        "/updates/checkInterval",
        120,
        (60, 1440),
        60,
    );
    let thresholds = page.subsection(&updates, &tr("Pending package thresholds"), "");
    let (advise_row, advise) = page.config_spin(
        &thresholds,
        "info",
        &tr("Advise updating at"),
        "/updates/adviseUpdateThreshold",
        75,
        (1, 1000),
        25,
    );
    let (strongly_row, strongly) = page.config_spin(
        &thresholds,
        "warning",
        &tr("Strongly advise updating at"),
        "/updates/stronglyAdviseUpdateThreshold",
        200,
        (1, 2000),
        25,
    );
    let installed = glib::find_program_in_path("checkupdates").is_some();
    let follow = move || {
        let enabled = config::value_bool(ENABLE_CHECK, true);
        missing.set_visible(enabled && !installed);
        Page::set_spin_row_enabled(&interval_row, &interval, enabled);
        Page::set_spin_row_enabled(&advise_row, &advise, enabled);
        Page::set_spin_row_enabled(&strongly_row, &strongly, enabled);
    };
    follow();
    page.watch(ENABLE_CHECK, follow);

    let weather = page.section("weather_mix", &tr("Weather"));
    let switches = page.row(&weather);
    page.config_switch(
        &switches,
        "assistant_navigation",
        &tr("Enable GPS based location"),
        "/bar/weather/enableGPS",
        true,
    );
    let fahrenheit = page.config_switch(
        &switches,
        "thermometer",
        &tr("Fahrenheit unit"),
        "/bar/weather/useUSCS",
        false,
    );
    page.tip(
        &fahrenheit.button,
        &tr("It may take a few seconds to update"),
    );
    page.config_text(
        &weather,
        Style::Filled,
        &tr("City name"),
        "/bar/weather/city",
        "",
    );
    page.config_spin(
        &weather,
        "av_timer",
        &tr("Polling interval (m)"),
        "/bar/weather/fetchInterval",
        10,
        (5, 50),
        5,
    );
    page
}

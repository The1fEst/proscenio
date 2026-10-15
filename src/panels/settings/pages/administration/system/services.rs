use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};

const ENABLE_CHECK: &str = "/updates/enableCheck";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let resources_section = page.section("memory", &tr("Resources"));
    page.config_spin(
        &resources_section,
        "av_timer",
        &tr("Polling interval (ms)"),
        "/resources/updateInterval",
        3000,
        (100, 10000),
        100,
    );

    let conflicts_section = page.section("block", &tr("Conflict killer"));
    let auto_kill_switch = page.config_switch(
        &conflicts_section,
        "notifications_off",
        &tr("Kill notification daemons without asking"),
        "/conflictKiller/autoKillNotificationDaemons",
        false,
    );
    page.tip(
        &auto_kill_switch.button,
        &tr(
            "Conflicting daemons like dunst or mako are killed silently instead of showing a dialog",
        ),
    );

    let updates_section = page.section("deployed_code_update", &tr("Updates"));
    let check_switch = page.config_switch(
        &updates_section,
        "check",
        &tr("Enable update checks"),
        ENABLE_CHECK,
        true,
    );
    page.tip(
        &check_switch.button,
        &tr(
            "Counts the packages paru, yay or pacman could upgrade, the first one installed; paru and yay count the AUR too. Also counts the commits the shell is behind on GitHub.\nThe bar shows an icon and the sidebar a button when the shell is behind or the package count reaches the threshold below.",
        ),
    );
    let (interval_row, interval_spin) = page.config_spin(
        &updates_section,
        "av_timer",
        &tr("Check interval (mins)"),
        "/updates/checkInterval",
        120,
        (60, 1440),
        60,
    );
    let thresholds_section =
        page.subsection(&updates_section, &tr("Pending package thresholds"), "");
    let (advise_row, advise_spin) = page.config_spin(
        &thresholds_section,
        "info",
        &tr("Advise updating at"),
        "/updates/adviseUpdateThreshold",
        1,
        (1, 1000),
        25,
    );
    let (strongly_advise_row, strongly_advise_spin) = page.config_spin(
        &thresholds_section,
        "warning",
        &tr("Strongly advise updating at"),
        "/updates/stronglyAdviseUpdateThreshold",
        200,
        (1, 2000),
        25,
    );
    let update_check_rows = move || {
        let is_enabled = config::value_bool(ENABLE_CHECK, true);
        Page::set_spin_row_enabled(&interval_row, &interval_spin, is_enabled);
        Page::set_spin_row_enabled(&advise_row, &advise_spin, is_enabled);
        Page::set_spin_row_enabled(&strongly_advise_row, &strongly_advise_spin, is_enabled);
    };
    update_check_rows();
    page.watch(ENABLE_CHECK, update_check_rows);

    let weather_section = page.section("weather_mix", &tr("Weather"));
    let switches_row = page.row(&weather_section);
    page.config_switch(
        &switches_row,
        "assistant_navigation",
        &tr("Enable GPS based location"),
        "/bar/weather/enableGPS",
        true,
    );
    let fahrenheit_switch = page.config_switch(
        &switches_row,
        "thermometer",
        &tr("Fahrenheit unit"),
        "/bar/weather/useUSCS",
        false,
    );
    page.tip(
        &fahrenheit_switch.button,
        &tr("It may take a few seconds to update"),
    );
    page.config_text(
        &weather_section,
        Style::Filled,
        &tr("City name"),
        "/bar/weather/city",
        "",
    );
    page.config_spin(
        &weather_section,
        "av_timer",
        &tr("Polling interval (m)"),
        "/bar/weather/fetchInterval",
        10,
        (5, 50),
        5,
    );
    page
}

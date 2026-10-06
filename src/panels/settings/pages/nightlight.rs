use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};

const NIGHT_AUTOMATIC: &str = "/light/night/automatic";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let night = page.section("", "");
    page.config_switch(
        &night,
        "schedule",
        &tr("Automatic schedule"),
        NIGHT_AUTOMATIC,
        true,
    );
    let times = page.uniform_row(&night);
    let from = page.text_field(
        &times,
        Style::Outlined,
        &tr("From (HH:mm)"),
        || config::value_str("/light/night/from").unwrap_or_else(|| "19:00".to_owned()),
        |text| config::store_value("/light/night/from", Value::from(text.trim())),
    );
    page.refresh_text_on("/light/night/from", &from);
    let to = page.text_field(
        &times,
        Style::Outlined,
        &tr("To (HH:mm)"),
        || config::value_str("/light/night/to").unwrap_or_else(|| "06:30".to_owned()),
        |text| config::store_value("/light/night/to", Value::from(text.trim())),
    );
    page.refresh_text_on("/light/night/to", &to);
    let schedule_enabled = move || {
        let automatic = config::value_bool(NIGHT_AUTOMATIC, true);
        from.set_enabled(automatic);
        to.set_enabled(automatic);
    };
    schedule_enabled();
    page.watch(NIGHT_AUTOMATIC, schedule_enabled);
    page.config_spin(
        &night,
        "thermostat",
        &tr("Color temperature (K)"),
        "/light/night/colorTemperature",
        5000,
        (1000, 6500),
        100,
    );
    let (transition, _) = page.config_spin(
        &night,
        "line_curve",
        &tr("Transition (min)"),
        "/light/night/transition",
        config::NIGHT_TRANSITION as i64,
        (0, 120),
        5,
    );
    page.tip(
        &transition,
        &tr("How long the scheduled change takes, from the set time on; 0 switches at once"),
    );
    page
}

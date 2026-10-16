use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::services::session::parse_clock_time;

const NIGHT_AUTOMATIC: &str = "/light/night/automatic";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let night_section = page.section("", "");
    page.config_switch(
        &night_section,
        "schedule",
        &tr("Automatic schedule"),
        NIGHT_AUTOMATIC,
        true,
    );
    let times_row = page.uniform_row(&night_section);
    let from_field = page.text_field(
        &times_row,
        Style::Outlined,
        &tr("From (HH:mm)"),
        || config::value_str("/light/night/from").unwrap_or_else(|| config::NIGHT_FROM.to_owned()),
        |text| {
            if let Some(time) = parse_clock_time(&text) {
                config::store_value("/light/night/from", Value::from(time));
            }
        },
    );
    page.refresh_text_on("/light/night/from", &from_field);
    let to_field = page.text_field(
        &times_row,
        Style::Outlined,
        &tr("To (HH:mm)"),
        || config::value_str("/light/night/to").unwrap_or_else(|| config::NIGHT_TO.to_owned()),
        |text| {
            if let Some(time) = parse_clock_time(&text) {
                config::store_value("/light/night/to", Value::from(time));
            }
        },
    );
    page.refresh_text_on("/light/night/to", &to_field);
    let update_schedule_fields = move || {
        let is_automatic = config::value_bool(NIGHT_AUTOMATIC, true);
        from_field.set_enabled(is_automatic);
        to_field.set_enabled(is_automatic);
    };
    update_schedule_fields();
    page.watch(NIGHT_AUTOMATIC, update_schedule_fields);
    page.config_spin(
        &night_section,
        "thermostat",
        &tr("Color temperature (K)"),
        "/light/night/colorTemperature",
        5000,
        (1000, 6500),
        100,
    );
    let (transition_row, _) = page.config_spin(
        &night_section,
        "line_curve",
        &tr("Transition (min)"),
        "/light/night/transition",
        config::NIGHT_TRANSITION_MINUTES as i64,
        (0, 120),
        5,
    );
    page.tip(
        &transition_row,
        &tr(
            "How long the scheduled change takes: it is done by the start time and begins at the end time; 0 switches at once",
        ),
    );
    page
}

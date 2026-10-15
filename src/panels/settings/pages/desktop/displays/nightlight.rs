use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};

const NIGHT_AUTOMATIC: &str = "/light/night/automatic";

fn parse_clock_time(text: &str) -> Option<String> {
    let (hour, minute) = text.trim().split_once(':')?;
    let is_number =
        |part: &str| !part.is_empty() && part.chars().all(|character| character.is_ascii_digit());
    if !is_number(hour) || hour.len() > 2 || !is_number(minute) || minute.len() != 2 {
        return None;
    }
    let (hour, minute) = (hour.parse::<u32>().ok()?, minute.parse::<u32>().ok()?);
    (hour < 24 && minute < 60).then(|| format!("{hour:02}:{minute:02}"))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_clock_time_is_stored() {
        assert_eq!(parse_clock_time(" 19:30 ").as_deref(), Some("19:30"));
        assert_eq!(parse_clock_time("7:05").as_deref(), Some("07:05"));
        assert_eq!(parse_clock_time("00:00").as_deref(), Some("00:00"));
        for wrong in [
            "19.30", "7pm", "24:00", "12:60", "12:5", "12", "", "1:2:3", "-1:30",
        ] {
            assert_eq!(parse_clock_time(wrong), None, "{wrong}");
        }
    }
}

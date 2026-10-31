use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};

const NIGHT_AUTOMATIC: &str = "/light/night/automatic";
const NIGHT_SCHEDULE: &str = "/light/night/schedule";

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
    page.add_schedule_selection(&night_section, NIGHT_SCHEDULE);
    let times_row = page.uniform_row(&night_section);
    let from_field = page.add_clock_field(
        &times_row,
        &tr("From (HH:mm)"),
        "/light/night/from",
        config::NIGHT_FROM,
    );
    let to_field = page.add_clock_field(
        &times_row,
        &tr("To (HH:mm)"),
        "/light/night/to",
        config::NIGHT_TO,
    );
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

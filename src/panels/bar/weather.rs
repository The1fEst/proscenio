use gtk4::gdk;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::{tr, trf};
use crate::core::scope::Scope;
use crate::platform::dbus::notify::{self, Notification};
use crate::services::session::weather::{self, DailyEntry, HourlyEntry, Weather};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::{display::text, overlay::popup};

const PADDING: i32 = 10;
const LAYOUT_SPACING: i32 = 5;
const CARD_PADDING: i32 = 14;
const FORECAST_PADDING: i32 = 10;
const FORECAST_SPACING: i32 = 8;

pub fn build(weather: &Rc<Weather>, config: &Rc<Config>, scope: &Scope) -> gtk4::Widget {
    let symbol = text::symbol("cloud", pixel_size::LARGE as f64);
    text::set_color(&symbol, "colOnLayer1");
    symbol.set_valign(gtk4::Align::Center);

    let degrees = text::styled("--°");
    text::set_color(&degrees, "colOnLayer1");

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, LAYOUT_SPACING);
    row.append(&symbol);
    row.append(&Centered::optical(&degrees));
    let area = Centered::new(&row);
    area.add_css_class("press-sink");
    area.set_size_request(
        row.measure(gtk4::Orientation::Horizontal, -1).1 + PADDING * 2,
        crate::core::config::BASE_BAR_HEIGHT,
    );

    let panel = Panel::new();
    popup::attach(&area, popup::Bar::of(config), &panel.column);

    let show = {
        let weather = weather.clone();
        let area = area.clone();
        let row = row.clone();
        move || {
            let report = weather.report.borrow();
            symbol.set_text(weather::symbol(&report.code));
            degrees.set_text(if report.temperature.is_empty() {
                "--°"
            } else {
                &report.temperature
            });
            area.set_size_request(
                row.measure(gtk4::Orientation::Horizontal, -1).1 + PADDING * 2,
                crate::core::config::BASE_BAR_HEIGHT,
            );
            panel.show(&report);
        }
    };
    show();
    scope.keep(weather.subscribe(show));

    let refresh_gesture = gtk4::GestureClick::new();
    refresh_gesture.set_button(gdk::BUTTON_SECONDARY);
    refresh_gesture.connect_pressed({
        let weather = weather.clone();
        move |_, _, _, _| {
            weather.fetch();
            notify::send(&Notification {
                app: "Shell",
                summary: &tr("Weather"),
                body: &tr("Refreshing (manually triggered)"),
                ..Default::default()
            });
        }
    });
    area.add_controller(refresh_gesture);

    area.upcast()
}

struct Panel {
    column: gtk4::Box,
    city: gtk4::Label,
    summary: gtk4::Label,
    values: Vec<gtk4::Label>,
    hourly: gtk4::Box,
    hourly_card: gtk4::Box,
    daily: gtk4::Box,
    daily_card: gtk4::Box,
    footer: gtk4::Label,
}

impl Panel {
    fn new() -> Self {
        let pin = text::symbol("location_on", pixel_size::LARGE as f64);
        text::set_font(
            &pin,
            text::Family::Material,
            pixel_size::LARGE as f64,
            &format!("FILL=0,opsz={},wght=500", pixel_size::LARGE),
        );
        text::set_color(&pin, "colOnSurfaceVariant");
        let city = text::styled_sized("", pixel_size::NORMAL);
        text::set_color(&city, "colOnSurfaceVariant");

        let place = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        place.set_halign(gtk4::Align::Center);
        place.append(&pin);
        place.append(&city);

        let summary = text::styled_sized("", pixel_size::SMALLER);
        text::set_color(&summary, "colOnSurfaceVariant");

        let header = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        header.set_halign(gtk4::Align::Center);
        header.append(&place);
        header.append(&summary);

        let grid = gtk4::Grid::new();
        grid.set_row_spacing(5);
        grid.set_column_spacing(5);
        grid.set_column_homogeneous(true);
        let cards = [
            ("UV Index", "wb_sunny"),
            ("Wind", "air"),
            ("Precipitation", "rainy_light"),
            ("Humidity", "humidity_low"),
            ("Visibility", "visibility"),
            ("Pressure", "readiness_score"),
            ("Sunrise", "wb_twilight"),
            ("Sunset", "bedtime"),
        ];
        let mut values = Vec::new();
        for (index, (title, icon)) in cards.iter().enumerate() {
            let (card, value) = card(title, icon);
            grid.attach(&card, index as i32 % 2, index as i32 / 2, 1, 1);
            values.push(value);
        }

        let hourly = gtk4::Box::new(gtk4::Orientation::Horizontal, FORECAST_SPACING);
        hourly.set_homogeneous(true);
        let hourly_card = build_forecast_card(&hourly);

        let daily = gtk4::Box::new(gtk4::Orientation::Vertical, FORECAST_SPACING);
        let daily_card = build_forecast_card(&daily);

        let footer = text::styled_sized("", pixel_size::SMALLER);
        text::set_color(&footer, "colOnSurfaceVariant");
        footer.set_halign(gtk4::Align::Center);

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        column.append(&header);
        column.append(&grid);
        column.append(&hourly_card);
        column.append(&daily_card);
        column.append(&footer);

        Panel {
            column,
            city,
            summary,
            values,
            hourly,
            hourly_card,
            daily,
            daily_card,
            footer,
        }
    }

    fn show(&self, report: &weather::Report) {
        self.city.set_text(&report.city);
        self.summary.set_text(&format!(
            "{} • {}",
            report.temperature,
            trf("Feels like %1", &[&report.feels_like])
        ));
        let readings = [
            report.uv.clone(),
            format!("({}) {}", report.wind_direction, report.wind),
            report.precipitation.clone(),
            report.humidity.clone(),
            report.visibility.clone(),
            report.pressure.clone(),
            report.sunrise.clone(),
            report.sunset.clone(),
        ];
        for (label, reading) in self.values.iter().zip(readings) {
            label.set_text(&reading);
        }
        replace_children(
            &self.hourly,
            &self.hourly_card,
            report.hourly.iter().map(build_hourly_slot),
        );
        replace_children(
            &self.daily,
            &self.daily_card,
            report.daily.iter().map(build_daily_row),
        );
        self.footer
            .set_text(&trf("Last refresh: %1", &[&report.refresh_time]));
    }
}

fn build_forecast_card(content: &gtk4::Box) -> gtk4::Box {
    content.set_margin_top(FORECAST_PADDING);
    content.set_margin_bottom(FORECAST_PADDING);
    content.set_margin_start(FORECAST_PADDING);
    content.set_margin_end(FORECAST_PADDING);
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("weather-card");
    card.append(content);
    card
}

fn replace_children(
    container: &gtk4::Box,
    card: &gtk4::Box,
    children: impl Iterator<Item = gtk4::Widget>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    for child in children {
        container.append(&child);
    }
    card.set_visible(container.first_child().is_some());
}

fn build_muted_label(content: &str, size: i32) -> gtk4::Label {
    let label = text::styled_sized(content, size);
    text::set_color(&label, "colOnSurfaceVariant");
    label
}

fn build_condition_symbol(code: &str) -> gtk4::Label {
    let symbol = text::symbol(weather::symbol(code), pixel_size::LARGE as f64);
    text::set_color(&symbol, "colOnSurfaceVariant");
    symbol
}

fn build_hourly_slot(entry: &HourlyEntry) -> gtk4::Widget {
    let slot = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    slot.set_halign(gtk4::Align::Center);
    slot.append(&build_muted_label(&entry.time, pixel_size::SMALLER));
    slot.append(&build_condition_symbol(&entry.code));
    slot.append(&build_muted_label(&entry.temperature, pixel_size::SMALL));
    slot.append(&build_muted_label(
        &entry.chance_of_rain,
        pixel_size::SMALLEST,
    ));
    slot.upcast()
}

fn build_daily_row(entry: &DailyEntry) -> gtk4::Widget {
    let weekday = build_muted_label(&entry.weekday, pixel_size::SMALL);
    weekday.set_xalign(0.0);
    weekday.set_hexpand(true);
    let chance = build_muted_label(&entry.chance_of_rain, pixel_size::SMALLER);
    chance.set_width_chars(4);
    chance.set_xalign(1.0);
    let range = build_muted_label(
        &format!("{} / {}", entry.minimum, entry.maximum),
        pixel_size::SMALL,
    );
    range.set_width_chars(9);
    range.set_xalign(1.0);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, FORECAST_SPACING);
    row.append(&weekday);
    row.append(&chance);
    row.append(&build_condition_symbol(&entry.code));
    row.append(&range);
    row.upcast()
}

fn card(title: &str, icon: &str) -> (gtk4::Widget, gtk4::Label) {
    let symbol = text::symbol(icon, pixel_size::NORMAL as f64);
    text::set_color(&symbol, "colOnSurfaceVariant");
    let name_label = text::styled_sized(&tr(title), pixel_size::SMALLER);
    text::set_color(&name_label, "colOnSurfaceVariant");

    let head = gtk4::Box::new(gtk4::Orientation::Horizontal, LAYOUT_SPACING);
    head.append(&symbol);
    head.append(&name_label);

    let reading_label = text::styled("");
    text::set_color(&reading_label, "colOnSurfaceVariant");

    let column = Column::with_extra(-10, CARD_PADDING * 2);
    column.set_vexpand(true);
    column.append(&head);
    column.append(&reading_label);

    let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    holder.add_css_class("weather-card");
    holder.set_hexpand(true);
    holder.append(&column);
    (holder.upcast(), reading_label)
}

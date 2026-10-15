use gtk4::gdk;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::{tr, trf};
use crate::core::scope::Scope;
use crate::platform::dbus::notify::{self, Notification};
use crate::services::session::weather::{self, Weather};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::{display::text, overlay::popup};

const PADDING: i32 = 10;
const LAYOUT_SPACING: i32 = 5;
const CARD_PADDING: i32 = 14;

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

        let footer = text::styled_sized("", pixel_size::SMALLER);
        text::set_color(&footer, "colOnSurfaceVariant");
        footer.set_halign(gtk4::Align::Center);

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        column.append(&header);
        column.append(&grid);
        column.append(&footer);

        Panel {
            column,
            city,
            summary,
            values,
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
        self.footer
            .set_text(&trf("Last refresh: %1", &[&report.refresh_time]));
    }
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

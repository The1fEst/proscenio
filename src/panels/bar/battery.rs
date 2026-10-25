use gtk4::cairo;
use gtk4::gdk::RGBA;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::tr;
use crate::core::scope::Scope;
use crate::services::hardware::battery::{Battery, Charge, FULLY_CHARGED};
use crate::ui::theme::{SharedTheme, transparentize};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::{display::text, overlay::popup};

const WIDTH: i32 = 30;
const HEIGHT: i32 = 18;
const RADIUS: f64 = 2.0;
const TEXT_SIZE: f64 = 13.0;
const BOLT_SIZE: f64 = 12.0;
const VERTICAL_WIDTH: i32 = 20;
const VERTICAL_HEIGHT: i32 = 36;
const VERTICAL_ICON_SIZE: f64 = 16.0;
const VERTICAL_SPACING: f64 = -4.0;

pub fn build(
    battery: &Battery,
    theme: &SharedTheme,
    config: &Config,
    scope: &Scope,
) -> gtk4::Widget {
    let is_vertical = config.vertical;
    let area = gtk4::DrawingArea::new();
    area.add_css_class("halo");
    let holder = Centered::new(&area);
    if is_vertical {
        area.set_content_width(VERTICAL_WIDTH);
        area.set_content_height(VERTICAL_HEIGHT);
    } else {
        area.set_content_width(WIDTH);
        area.set_content_height(HEIGHT);
        holder.set_size_request(-1, crate::core::config::BASE_BAR_HEIGHT);
    }

    let charge = Rc::new(Cell::new(battery.charge.get()));
    area.set_draw_func({
        let charge = charge.clone();
        let theme = theme.clone();
        let low_threshold = config.battery_low;
        move |_, cr, width, height| {
            if is_vertical {
                draw_vertical(
                    cr,
                    width,
                    height,
                    charge.get(),
                    &theme.borrow(),
                    low_threshold,
                )
            } else {
                draw(
                    cr,
                    width,
                    height,
                    charge.get(),
                    &theme.borrow(),
                    low_threshold,
                )
            }
        }
    });

    let details = Details::new();
    popup::attach(&holder, popup::Bar::of(config), &details.column);
    let show = {
        let area = area.clone();
        let holder = holder.clone();
        let battery = battery.clone();
        let charge = charge.clone();
        move || {
            let current_charge = battery.charge.get();
            charge.set(current_charge);
            holder.set_visible(current_charge.is_available);
            details.show(current_charge);
            area.queue_draw();
        }
    };
    show();
    scope.keep(battery.subscribe(show));

    holder.upcast()
}

fn draw(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    charge: Charge,
    theme: &crate::ui::theme::Theme,
    low_threshold: f64,
) {
    let (width, height) = (width as f64, height as f64);
    let is_low = charge.percentage <= low_threshold && !charge.is_charging();
    let highlight = if is_low {
        theme.colors.col_error
    } else {
        theme.colors.col_on_secondary_container
    };

    let _ = cr.save();
    trace_capsule(cr, 0.0, 0.0, width, height);
    cr.clip();

    set_source(cr, transparentize(highlight, 0.5));
    let _ = cr.paint();

    set_source(cr, highlight);
    trace_rounded_rect(
        cr,
        0.0,
        0.0,
        width * charge.percentage.clamp(0.0, 1.0),
        height,
    );
    let _ = cr.fill();
    let _ = cr.restore();

    let percent = (charge.percentage * 100.0).round() as i64;
    let percent_text = percent.to_string();
    let shows_bolt = charge.is_charging() && charge.percentage < 1.0;

    let layout = pangocairo::functions::create_layout(cr);
    let (family, _) = text::application_font();
    let mut description = pango::FontDescription::new();
    description.set_family(&family);
    description.set_absolute_size(TEXT_SIZE * pango::SCALE as f64);
    description.set_variations(Some(if percent_text.len() > 2 {
        "wght=500,opsz=18"
    } else {
        "wght=600,opsz=18"
    }));
    layout.set_font_description(Some(&description));
    layout.set_text(&percent_text);
    let (text_width, text_height) = layout.pixel_size();

    let spark = shows_bolt.then(|| {
        let spark = pango::Layout::new(&layout.context());
        spark.set_font_description(Some(&text::font(
            text::Family::Material,
            BOLT_SIZE,
            &format!("FILL=1,opsz={BOLT_SIZE},wght=600"),
        )));
        spark.set_text("bolt");
        spark
    });

    let spark_width = spark
        .as_ref()
        .map(|spark| spark.pixel_size().0 - 4)
        .unwrap_or(0);
    let mut x = (width - (text_width + spark_width) as f64) / 2.0;

    cr.set_operator(cairo::Operator::DestOut);
    if let Some(spark) = &spark {
        cr.move_to(x, (height - spark.pixel_size().1 as f64) / 2.0);
        pangocairo::functions::show_layout(cr, spark);
        x += spark_width as f64;
    }
    cr.move_to(x, (height - text_height as f64) / 2.0);
    pangocairo::functions::show_layout(cr, &layout);
    cr.set_operator(cairo::Operator::Over);
}

fn draw_vertical(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    charge: Charge,
    theme: &crate::ui::theme::Theme,
    low_threshold: f64,
) {
    let (width, height) = (width as f64, height as f64);
    let is_low = charge.percentage <= low_threshold && !charge.is_charging();
    let highlight = if is_low {
        theme.colors.col_error
    } else {
        theme.colors.col_on_secondary_container
    };
    let value = charge.percentage.clamp(0.0, 1.0);

    let _ = cr.save();
    trace_capsule(cr, 0.0, 0.0, width, height);
    cr.clip();

    set_source(cr, transparentize(highlight, 0.5));
    let _ = cr.paint();

    set_source(cr, highlight);
    trace_rounded_rect(cr, 0.0, height - height * value, width, height * value);
    let _ = cr.fill();
    let _ = cr.restore();

    let symbol = if value >= 1.0 {
        "check"
    } else if charge.is_charging() {
        "bolt"
    } else {
        battery_icon(charge.percentage * 100.0)
    };
    let icon = pangocairo::functions::create_layout(cr);
    icon.set_font_description(Some(&text::font(
        text::Family::Material,
        VERTICAL_ICON_SIZE,
        &format!("FILL=1,opsz={VERTICAL_ICON_SIZE},wght=600"),
    )));
    icon.set_text(symbol);
    let (icon_width, icon_height) = icon.pixel_size();

    let percent = (charge.percentage * 100.0).round() as i64;
    let label = percent.to_string();
    let number = (label.len() <= 2).then(|| {
        let number = pango::Layout::new(&icon.context());
        let (family, _) = text::application_font();
        let mut description = pango::FontDescription::new();
        description.set_family(&family);
        description.set_absolute_size(TEXT_SIZE * pango::SCALE as f64);
        description.set_variations(Some("wght=600,opsz=18"));
        number.set_font_description(Some(&description));
        number.set_text(&label);
        number
    });

    let column_height = icon_height as f64
        + number.as_ref().map_or(0.0, |number| {
            VERTICAL_SPACING + number.pixel_size().1 as f64
        });
    let mut y = (height - column_height) / 2.0;

    cr.set_operator(cairo::Operator::DestOut);
    cr.move_to((width - icon_width as f64) / 2.0, y);
    pangocairo::functions::show_layout(cr, &icon);
    y += icon_height as f64 + VERTICAL_SPACING;
    if let Some(number) = &number {
        cr.move_to((width - number.pixel_size().0 as f64) / 2.0, y);
        pangocairo::functions::show_layout(cr, number);
    }
    cr.set_operator(cairo::Operator::Over);
}

fn battery_icon(percentage: f64) -> &'static str {
    match percentage {
        value if value >= 93.0 => "battery_android_full",
        value if value >= 78.0 => "battery_android_6",
        value if value >= 64.0 => "battery_android_5",
        value if value >= 50.0 => "battery_android_4",
        value if value >= 35.0 => "battery_android_3",
        value if value >= 21.0 => "battery_android_2",
        value if value >= 7.0 => "battery_android_1",
        _ => "battery_android_0",
    }
}

struct Details {
    column: gtk4::Box,
    time_row: gtk4::Widget,
    time_name_label: gtk4::Label,
    time_value: gtk4::Label,
    rate_row: gtk4::Widget,
    rate_name_label: gtk4::Label,
    rate_value: gtk4::Label,
    health_value: gtk4::Label,
}

impl Details {
    fn new() -> Self {
        let column = popup::column();
        column.append(&popup::header("battery_android_full", &tr("Battery")));
        let (time_row, time_value) = popup::value("schedule", &tr("Time to empty:"), "");
        let (rate_row, rate_value) = popup::value("bolt", &tr("Discharging:"), "");
        let (health_row, health_value) = popup::value("heart_check", &tr("Health:"), "");
        column.append(&time_row);
        column.append(&rate_row);
        column.append(&health_row);
        let find_name_label = |row: &gtk4::Widget| {
            row.first_child()
                .and_then(|icon| icon.next_sibling())
                .and_downcast::<gtk4::Label>()
                .expect("value row label")
        };
        Details {
            time_name_label: find_name_label(&time_row),
            rate_name_label: find_name_label(&rate_row),
            column,
            time_row,
            time_value,
            rate_row,
            rate_value,
            health_value,
        }
    }

    fn show(&self, charge: Charge) {
        let remaining_seconds = if charge.is_charging() {
            charge.time_to_full
        } else {
            charge.time_to_empty
        };
        self.time_row.set_visible(
            !(charge.state == FULLY_CHARGED || remaining_seconds <= 0 || charge.rate <= 0.01),
        );
        self.time_name_label.set_text(&tr(if charge.is_charging() {
            "Time to full:"
        } else {
            "Time to empty:"
        }));
        set_value(&self.time_value, &duration(remaining_seconds));

        self.rate_row
            .set_visible(!(charge.state != FULLY_CHARGED && charge.rate == 0.0));
        self.rate_name_label.set_text(&tr(match charge.state {
            FULLY_CHARGED => "Fully charged",
            state if state == crate::services::hardware::battery::CHARGING => "Charging:",
            _ => "Discharging:",
        }));
        set_value(
            &self.rate_value,
            &if charge.state == FULLY_CHARGED {
                String::new()
            } else {
                format!("{:.2}W", charge.rate)
            },
        );
        set_value(&self.health_value, &format!("{:.1}%", charge.health));
    }
}

fn set_value(label: &gtk4::Label, value: &str) {
    label.set_text(value);
    label.set_visible(!value.is_empty());
}

fn duration(seconds: i64) -> String {
    let (hours, minutes) = (seconds / 3600, (seconds % 3600) / 60);
    if hours > 0 {
        format!("{hours}h, {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn trace_capsule(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64) {
    trace_rounded_rect_with_radius(cr, x, y, width, height, width.min(height) / 2.0);
}

fn trace_rounded_rect(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    trace_rounded_rect_with_radius(
        cr,
        x,
        y,
        width,
        height,
        RADIUS.min(width / 2.0).min(height / 2.0),
    );
}

fn trace_rounded_rect_with_radius(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let quarter_turn = std::f64::consts::FRAC_PI_2;
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -quarter_turn, 0.0);
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        quarter_turn,
    );
    cr.arc(
        x + radius,
        y + height - radius,
        radius,
        quarter_turn,
        2.0 * quarter_turn,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        2.0 * quarter_turn,
        3.0 * quarter_turn,
    );
    cr.close_path();
}

fn set_source(cr: &cairo::Context, color: RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

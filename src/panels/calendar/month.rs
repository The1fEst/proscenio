use gtk4::glib;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text;
use crate::ui::widgets::tooltip::{self, Tooltip};

const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
const ROWS: usize = 6;
const COLUMNS: usize = 7;
const CELL: i32 = 38;
const SPACING: i32 = 5;
const HEADER_HEIGHT: i32 = 30;
const HEADER_PADDING: i32 = 10;
const MARGIN: i32 = 10;
const DISABLED_OPACITY: f64 = 0.4;

struct Day {
    number: i32,
    place: i32,
}

pub struct Month {
    pub widget: gtk4::Widget,
    shift: Rc<Cell<i32>>,
    show: Rc<dyn Fn()>,
}

impl Month {
    pub fn reset(&self) {
        self.shift.set(0);
        (self.show)();
    }

    pub fn step(&self, months: i32) {
        self.shift.set(self.shift.get() + months);
        (self.show)();
    }
}

pub fn build(theme: &SharedTheme) -> Rc<Month> {
    let shift = Rc::new(Cell::new(0i32));

    let title = text::styled_sized("", pixel_size::LARGER);
    text::set_color(&title, "colOnLayer1");
    let jump = header_button(theme);
    jump.set_content(&Centred::new(&title), HEADER_PADDING, 0);
    let hint = Tooltip::new(&jump, theme, tooltip::Kind::Styled);
    hint.place_like_qt();
    hint.set_text(&tr("Jump to current month"));

    let back = chevron(theme, "chevron_left");
    let forward = chevron(theme, "chevron_right");

    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
    header.append(&jump);
    header.append(&spacer);
    header.append(&back);
    header.append(&forward);

    let names = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
    names.set_halign(gtk4::Align::Center);
    for name in WEEKDAYS {
        let label = text::styled(&tr(name));
        text::set_color(&label, "colOnLayer1");
        let cell = Centred::new(&label);
        cell.set_size_request(CELL, CELL);
        cell.set_opacity(DISABLED_OPACITY);
        names.append(&cell);
    }

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
    column.set_halign(gtk4::Align::Center);
    column.set_valign(gtk4::Align::Start);
    column.set_margin_top(MARGIN);
    column.set_margin_bottom(MARGIN);
    column.append(&header);
    column.append(&names);

    let mut days: Vec<(RippleButton, gtk4::Label)> = Vec::with_capacity(ROWS * COLUMNS);
    for _ in 0..ROWS {
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        line.set_halign(gtk4::Align::Center);
        for _ in 0..COLUMNS {
            let label = text::styled("");
            label.add_css_class("color-fade");
            let button = RippleButton::new(theme);
            button.set_radius(rounding::SMALL as f64);
            button.set_size_request(CELL, CELL);
            button.set_content(&Centred::new(&label), 0, 0);
            line.append(&button);
            days.push((button, label));
        }
        column.append(&line);
    }

    let show: Rc<dyn Fn()> = {
        let shift = shift.clone();
        Rc::new(move || {
            let months = shift.get();
            let viewing = viewed(months);
            let mark = if months == 0 { "" } else { "• " };
            let month: String = viewing.format("%B %Y").map(Into::into).unwrap_or_default();
            title.set_text(&format!("{mark}{month}"));

            for (index, cell) in layout(&viewing, months == 0).iter().enumerate() {
                let (button, label) = &days[index];
                label.set_text(&cell.number.to_string());
                button.set_toggled(cell.place == 1);
                text::set_color(
                    label,
                    match cell.place {
                        1 => "m3onPrimary",
                        0 => "colOnLayer1",
                        _ => "colOutlineVariant",
                    },
                );
            }
        })
    };
    show();

    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let shift = shift.clone();
        let hint = hint.clone();
        move |_, _, _| hint.show(shift.get() != 0)
    });
    hover.connect_leave({
        let hint = hint.clone();
        move |_| hint.show(false)
    });
    jump.add_controller(hover);

    let month = Rc::new(Month {
        widget: column.clone().upcast(),
        shift,
        show,
    });

    jump.connect_down({
        let month = Rc::downgrade(&month);
        let hint = hint.clone();
        move || {
            if let Some(month) = month.upgrade() {
                month.reset();
                hint.show(false);
            }
        }
    });
    back.connect_down({
        let month = Rc::downgrade(&month);
        move || {
            if let Some(month) = month.upgrade() {
                month.step(-1);
            }
        }
    });
    forward.connect_down({
        let month = Rc::downgrade(&month);
        move || {
            if let Some(month) = month.upgrade() {
                month.step(1);
            }
        }
    });

    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll({
        let month = Rc::downgrade(&month);
        move |_, _, delta| {
            let Some(month) = month.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if delta == 0.0 {
                return glib::Propagation::Proceed;
            }
            month.step(if delta < 0.0 { -1 } else { 1 });
            glib::Propagation::Stop
        }
    });
    column.add_controller(wheel);

    month
}

fn header_button(theme: &SharedTheme) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_look(Look {
        background: |theme| theme.colors.col_layer2,
        hover: |theme| theme.colors.col_layer2_hover,
        ripple: |theme| theme.colors.col_layer2_active,
        ..Look::default()
    });
    button.set_radius(rounding::FULL as f64);
    button.set_size_request(-1, HEADER_HEIGHT);
    button
}

fn chevron(theme: &SharedTheme, icon: &str) -> RippleButton {
    let symbol = text::symbol(icon, pixel_size::LARGER as f64);
    text::set_color(&symbol, "colOnLayer1");
    let button = header_button(theme);
    button.set_size_request(HEADER_HEIGHT, HEADER_HEIGHT);
    button.set_content(&Centred::new(&symbol), 0, 0);
    button
}

fn viewed(months: i32) -> glib::DateTime {
    let now = glib::DateTime::now_local().expect("local time");
    if months == 0 {
        return now;
    }
    let target = now.month() - 1 + months;
    let year = now.year() + target.div_euclid(12);
    let month = target.rem_euclid(12) + 1;
    glib::DateTime::new(&glib::TimeZone::local(), year, month, 1, 0, 0, 0.0)
        .expect("first of the month")
}

fn leap(year: i32) -> bool {
    year % 400 == 0 || (year % 4 == 0 && year % 100 != 0)
}

fn days_in(month: i32, year: i32) -> i32 {
    if (month <= 7 && month % 2 == 1) || (month >= 8 && month % 2 == 0) {
        return 31;
    }
    if month == 2 {
        return if leap(year) { 29 } else { 28 };
    }
    30
}

fn layout(viewing: &glib::DateTime, highlight: bool) -> Vec<Day> {
    let weekday = (viewing.day_of_week() - 1).rem_euclid(7);
    let day = viewing.day_of_month();
    let month = viewing.month();
    let year = viewing.year();
    let first = (weekday + 35 - (day - 1)).rem_euclid(7);

    let this_month = days_in(month, year);
    let next_month = days_in(month % 12 + 1, if month == 12 { year + 1 } else { year });
    let previous_month = days_in(
        (month + 10) % 12 + 1,
        if month == 1 { year - 1 } else { year },
    );

    let mut difference = if first == 0 { 0 } else { -1 };
    let (mut fill, mut length) = if first == 0 {
        (1, this_month)
    } else {
        (previous_month - (first - 1), previous_month)
    };

    let mut cells = Vec::with_capacity(ROWS * COLUMNS);
    for _ in 0..ROWS * COLUMNS {
        cells.push(Day {
            number: fill,
            place: match (difference == 0, fill == day && highlight) {
                (true, true) => 1,
                (true, false) => 0,
                (false, _) => -1,
            },
        });
        fill += 1;
        if fill > length {
            difference += 1;
            length = if difference == 0 {
                this_month
            } else {
                next_month
            };
            fill = 1;
        }
    }
    cells
}

use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::events::{self, Dots};
use crate::core::i18n::tr;
use crate::core::scope::Scope;
use crate::services::session::agenda::civil::days_from_civil;
use crate::services::session::agenda::{Agenda, Window};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
const ROWS: usize = 6;
const COLUMNS: usize = 7;
const CELL: i32 = 38;
const SPACING: i32 = 5;
const HEADER_HEIGHT: i32 = 30;
const HEADER_PADDING: i32 = 10;
const MARGIN: i32 = 10;
const DISABLED_OPACITY: f64 = 0.4;
const DOTS_MARGIN: i32 = 4;

type ShowSlot = Rc<RefCell<Option<Weak<dyn Fn()>>>>;

struct Day {
    number: i32,
    place: i32,
    days: i64,
}

struct DayCell {
    button: RippleButton,
    label: gtk4::Label,
    dots: Dots,
}

pub struct Month {
    pub widget: gtk4::Widget,
    shift: Rc<Cell<i32>>,
    today: Rc<RefCell<glib::DateTime>>,
    selection: Rc<Cell<Option<i64>>>,
    window: Rc<Cell<Window>>,
    agenda: Rc<Agenda>,
    is_following_agenda: Rc<Cell<bool>>,
    show: Rc<dyn Fn()>,
}

impl Month {
    pub fn follow_agenda(&self) {
        if self.is_following_agenda.replace(true) {
            return;
        }
        (self.show)();
        self.agenda.activate(self.window.get());
    }

    pub fn unfollow_agenda(&self) {
        if self.is_following_agenda.replace(false) {
            self.agenda.deactivate();
        }
    }

    pub fn set_today(&self, now: &glib::DateTime) {
        let day = |date: &glib::DateTime| (date.year(), date.day_of_year());
        if day(&self.today.borrow()) == day(now) {
            return;
        }
        self.today.replace(now.clone());
        (self.show)();
    }

    pub fn reset(&self) {
        self.shift.set(0);
        self.selection.set(None);
        (self.show)();
    }

    pub fn step(&self, months: i32) {
        self.shift.set(self.shift.get() + months);
        (self.show)();
    }
}

pub fn build(theme: &SharedTheme, agenda: &Rc<Agenda>, scope: &Scope) -> Rc<Month> {
    let shift = Rc::new(Cell::new(0i32));
    let selection: Rc<Cell<Option<i64>>> = Rc::default();
    let window = Rc::new(Cell::new(Window {
        first_day: 0,
        end_day: 0,
    }));
    let today = Rc::new(RefCell::new(
        glib::DateTime::now_local().expect("local time"),
    ));

    let title = text::styled_sized("", pixel_size::LARGER);
    text::set_color(&title, "colOnLayer1");
    let jump_button = header_button(theme);
    jump_button.set_content(&Centered::new(&title), HEADER_PADDING, 0);
    let hint = Tooltip::new(&jump_button, theme, tooltip::Kind::Styled);
    hint.place_like_qt();
    hint.set_text(&tr("Jump to current month"));

    let back_button = chevron(theme, "chevron_left");
    let forward_button = chevron(theme, "chevron_right");

    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
    header.append(&jump_button);
    header.append(&spacer);
    header.append(&back_button);
    header.append(&forward_button);

    let names = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
    names.set_halign(gtk4::Align::Center);
    for name in WEEKDAYS {
        let label = text::styled(&tr(name));
        text::set_color(&label, "colOnLayer1");
        let cell = Centered::new(&label);
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

    let cell_days: Rc<Vec<Cell<i64>>> =
        Rc::new((0..ROWS * COLUMNS).map(|_| Cell::new(0)).collect());
    let show_slot = ShowSlot::default();
    let mut cells: Vec<DayCell> = Vec::with_capacity(ROWS * COLUMNS);
    for _ in 0..ROWS {
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        line.set_halign(gtk4::Align::Center);
        for _ in 0..COLUMNS {
            let label = text::styled("");
            label.add_css_class("color-fade");
            let dots = Dots::new(theme);
            dots.widget.set_margin_bottom(DOTS_MARGIN);
            let content = gtk4::Overlay::new();
            content.set_child(Some(&Centered::new(&label)));
            content.add_overlay(&dots.widget);
            let button = RippleButton::new(theme);
            button.set_radius(rounding::SMALL as f64);
            button.set_size_request(CELL, CELL);
            button.set_content(&content, 0, 0);
            button.connect_down({
                let index = cells.len();
                let cell_days = cell_days.clone();
                let selection = selection.clone();
                let show_slot = show_slot.clone();
                move || {
                    selection.set(Some(cell_days[index].get()));
                    let show = show_slot.borrow().as_ref().and_then(Weak::upgrade);
                    if let Some(show) = show {
                        show();
                    }
                }
            });
            line.append(&button);
            cells.push(DayCell {
                button,
                label,
                dots,
            });
        }
        column.append(&line);
    }

    let event_list = events::build_list(theme);
    event_list.widget.set_margin_bottom(MARGIN);
    column.append(&event_list.widget);

    let is_following_agenda = Rc::new(Cell::new(false));
    let show: Rc<dyn Fn()> = {
        let is_following_agenda = is_following_agenda.clone();
        let shift = shift.clone();
        let today = today.clone();
        let selection = selection.clone();
        let window = window.clone();
        let agenda = agenda.clone();
        let cell_days = cell_days.clone();
        let selected_look = Look {
            background: |theme| theme.colors.col_secondary_container,
            hover: |theme| theme.colors.col_secondary_container_hover,
            ripple: |theme| theme.colors.col_secondary_container_active,
            ..Look::default()
        };
        Rc::new(move || {
            let months = shift.get();
            let view_date = shift_month(&today.borrow(), months);
            let mark = if months == 0 { "" } else { "• " };
            let month: String = view_date
                .format("%B %Y")
                .map(Into::into)
                .unwrap_or_default();
            title.set_text(&format!("{mark}{month}"));

            let grid = layout(&view_date, months == 0);
            let grid_window = Window {
                first_day: grid[0].days,
                end_day: grid[0].days + (ROWS * COLUMNS) as i64,
            };
            window.set(grid_window);
            if is_following_agenda.get() {
                agenda.set_window(grid_window);
            }
            let now = today.borrow();
            let today_days = days_from_civil(
                now.year() as i64,
                now.month() as i64,
                now.day_of_month() as i64,
            );
            let selected_day = resolve_selection(
                selection.get(),
                today_days,
                grid_window,
                days_from_civil(view_date.year() as i64, view_date.month() as i64, 1),
            );

            for (index, day) in grid.iter().enumerate() {
                let cell = &cells[index];
                cell_days[index].set(day.days);
                cell.label.set_text(&day.number.to_string());
                let is_selected = day.days == selected_day && day.place != 1;
                cell.button.set_toggled(day.place == 1);
                cell.button.set_look(if is_selected {
                    selected_look
                } else {
                    Look::default()
                });
                text::set_color(
                    &cell.label,
                    match (day.place, is_selected) {
                        (1, _) => "m3onPrimary",
                        (_, true) => "colOnSecondaryContainer",
                        (0, _) => "colOnLayer1",
                        _ => "colOutlineVariant",
                    },
                );
                cell.dots.set(&agenda.occurrences_on(day.days));
            }

            event_list.show(selected_day, &agenda.occurrences_on(selected_day));
            event_list.widget.set_visible(agenda.is_available());
        })
    };
    show();
    show_slot.replace(Some(Rc::downgrade(&show)));
    scope.keep(agenda.subscribe({
        let show = show.clone();
        let is_following_agenda = is_following_agenda.clone();
        move || {
            if is_following_agenda.get() {
                show();
            }
        }
    }));

    let hover_controller = gtk4::EventControllerMotion::new();
    hover_controller.connect_enter({
        let shift = shift.clone();
        let hint = hint.clone();
        move |_, _, _| hint.show(shift.get() != 0)
    });
    hover_controller.connect_leave({
        let hint = hint.clone();
        move |_| hint.show(false)
    });
    jump_button.add_controller(hover_controller);

    let month = Rc::new(Month {
        widget: column.clone().upcast(),
        shift,
        today,
        selection,
        window,
        agenda: agenda.clone(),
        is_following_agenda,
        show,
    });

    jump_button.connect_down({
        let month = Rc::downgrade(&month);
        let hint = hint.clone();
        move || {
            if let Some(month) = month.upgrade() {
                month.reset();
                hint.show(false);
            }
        }
    });
    back_button.connect_down({
        let month = Rc::downgrade(&month);
        move || {
            if let Some(month) = month.upgrade() {
                month.step(-1);
            }
        }
    });
    forward_button.connect_down({
        let month = Rc::downgrade(&month);
        move || {
            if let Some(month) = month.upgrade() {
                month.step(1);
            }
        }
    });

    let scroll_controller =
        gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    scroll_controller.connect_scroll({
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
    column.add_controller(scroll_controller);

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
    button.set_content(&Centered::new(&symbol), 0, 0);
    button
}

fn shift_month(today: &glib::DateTime, months: i32) -> glib::DateTime {
    if months == 0 {
        return today.clone();
    }
    let target = today.month() - 1 + months;
    let year = today.year() + target.div_euclid(12);
    let month = target.rem_euclid(12) + 1;
    glib::DateTime::new(&glib::TimeZone::local(), year, month, 1, 0, 0, 0.0)
        .expect("first of the month")
}

fn is_leap_year(year: i32) -> bool {
    year % 400 == 0 || (year % 4 == 0 && year % 100 != 0)
}

fn days_in(month: i32, year: i32) -> i32 {
    if (month <= 7 && month % 2 == 1) || (month >= 8 && month % 2 == 0) {
        return 31;
    }
    if month == 2 {
        return if is_leap_year(year) { 29 } else { 28 };
    }
    30
}

fn layout(view_date: &glib::DateTime, should_mark_day: bool) -> Vec<Day> {
    let weekday = (view_date.day_of_week() - 1).rem_euclid(7);
    let day = view_date.day_of_month();
    let month = view_date.month();
    let year = view_date.year();
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

    let first_days = days_from_civil(year as i64, month as i64, 1) - first as i64;
    let mut cells = Vec::with_capacity(ROWS * COLUMNS);
    for index in 0..ROWS * COLUMNS {
        cells.push(Day {
            number: fill,
            place: match (difference == 0, fill == day && should_mark_day) {
                (true, true) => 1,
                (true, false) => 0,
                (false, _) => -1,
            },
            days: first_days + index as i64,
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

fn resolve_selection(
    selection: Option<i64>,
    today_days: i64,
    window: Window,
    month_first_days: i64,
) -> i64 {
    let day = selection.unwrap_or(today_days);
    if (window.first_day..window.end_day).contains(&day) {
        day
    } else {
        month_first_days
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::session::agenda::civil::civil_from_days;

    #[test]
    fn every_cell_knows_the_day_it_shows() {
        for (year, month, first_cell) in [(2026, 11, (2026, 10, 26)), (2026, 6, (2026, 6, 1))] {
            let view_date =
                glib::DateTime::new(&glib::TimeZone::local(), year, month, 1, 0, 0, 0.0)
                    .expect("date");
            let grid = layout(&view_date, false);
            let (first_year, first_month, first_day) = first_cell;
            assert_eq!(
                grid[0].days,
                days_from_civil(first_year, first_month, first_day)
            );
            for day in &grid {
                assert_eq!(civil_from_days(day.days).2, day.number as i64);
                assert_eq!(civil_from_days(day.days).1 == month as i64, day.place != -1);
            }
        }
    }

    #[test]
    fn a_selection_outside_the_grid_falls_back_to_the_first_of_the_viewed_month() {
        let window = Window {
            first_day: 100,
            end_day: 142,
        };
        assert_eq!(resolve_selection(Some(120), 105, window, 110), 120);
        assert_eq!(resolve_selection(None, 105, window, 110), 105);
        assert_eq!(resolve_selection(Some(99), 105, window, 110), 110);
        assert_eq!(resolve_selection(None, 200, window, 110), 110);
    }

    #[test]
    fn the_month_marks_the_day_it_is_given_and_steps_from_it() {
        let today =
            glib::DateTime::new(&glib::TimeZone::local(), 2026, 10, 11, 0, 5, 0.0).expect("date");
        let marked_numbers: Vec<i32> = layout(&shift_month(&today, 0), true)
            .iter()
            .filter(|day| day.place == 1)
            .map(|day| day.number)
            .collect();
        assert_eq!(marked_numbers, vec![11]);
        let shifted_date = shift_month(&today, 3);
        assert_eq!(
            (
                shifted_date.year(),
                shifted_date.month(),
                shifted_date.day_of_month()
            ),
            (2027, 1, 1)
        );
    }
}

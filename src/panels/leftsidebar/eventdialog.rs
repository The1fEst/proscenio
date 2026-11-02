use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::services::session::agenda::civil::{days_from_civil, days_in_month};
use crate::services::session::agenda::{Agenda, Draft, Slot, Target};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::{ComboBox, Switch};
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::spinbox::SpinBox;
use crate::ui::widgets::layout::group::pointer_cursor;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const WIDTH: f64 = 380.0;
const ROW_SPACING: i32 = 8;
const YEARS: (i64, i64) = (2000, 2100);
const DEFAULT_LENGTH_MINUTES: i64 = 30;
const MAX_LENGTH_MINUTES: i64 = 24 * 60;
const MINUTE_STEP: i64 = 5;

pub struct Note {
    pub id: u64,
    pub title: String,
    pub excerpt: String,
}

struct Field {
    spin: Rc<SpinBox>,
    value: Rc<Cell<i64>>,
}

impl Field {
    fn new(theme: &SharedTheme, range: (i64, i64), step: i64, initial: i64) -> Field {
        let spin = SpinBox::new(theme, range.0, range.1, step, 0);
        spin.set_value(initial);
        spin.root.set_hexpand(true);
        let value = Rc::new(Cell::new(initial));
        spin.connect_changed({
            let value = value.clone();
            move |new_value| value.set(new_value)
        });
        Field { spin, value }
    }
}

struct Form {
    note: Note,
    agenda: Rc<Agenda>,
    year: Field,
    month: Field,
    day: Field,
    hour: Field,
    minute: Field,
    length: Field,
    is_all_day: Cell<bool>,
    targets: RefCell<Vec<Target>>,
    chosen_index: Cell<usize>,
    combo: Rc<ComboBox>,
    status: gtk4::Label,
    create_button: RippleButton,
}

impl Form {
    fn build_draft(&self) -> Draft {
        let slot = if self.is_all_day.get() {
            Slot::AllDay
        } else {
            Slot::Interval {
                start_minute: self.hour.value.get() * 60 + self.minute.value.get(),
                length_minutes: self.length.value.get(),
            }
        };
        Draft {
            note_id: self.note.id,
            title: self.note.title.clone(),
            excerpt: self.note.excerpt.clone(),
            day: clamp_day(
                self.year.value.get(),
                self.month.value.get(),
                self.day.value.get(),
            ),
            slot,
        }
    }

    fn show_targets(&self, targets: Vec<Target>) {
        if targets.is_empty() {
            self.combo
                .set_items(&[tr("No calendar can take events")], 0);
            return;
        }
        let chosen_index = targets
            .iter()
            .position(|target| target.is_selected)
            .unwrap_or(0);
        self.chosen_index.set(chosen_index);
        self.combo
            .set_items(&list_names(&targets), chosen_index as i32);
        self.targets.replace(targets);
        self.create_button.set_sensitive(true);
    }

    fn set_time_enabled(&self, is_enabled: bool) {
        for field in [&self.hour, &self.minute, &self.length] {
            field.spin.set_enabled(is_enabled);
        }
    }
}

fn list_names(targets: &[Target]) -> Vec<String> {
    targets.iter().map(|target| target.name.clone()).collect()
}

fn clamp_day(year: i64, month: i64, day: i64) -> i64 {
    days_from_civil(year, month, day.min(days_in_month(year, month)))
}

fn build_caption(label: &str) -> gtk4::Label {
    let caption = text::styled_sized(label, pixel_size::SMALLER);
    text::set_color(&caption, "colSubtext");
    caption.set_xalign(0.0);
    caption
}

fn build_spin_row(fields: &[&Field]) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    row.set_homogeneous(true);
    for field in fields {
        row.append(&field.spin.root);
    }
    row
}

pub fn build(
    theme: &SharedTheme,
    agenda: &Rc<Agenda>,
    note: Note,
    now: &glib::DateTime,
) -> Rc<WindowDialog> {
    let dialog = WindowDialog::new(theme, None);
    dialog.set_background_width(WIDTH);
    dialog
        .column
        .add(&windowdialog::title(&tr("Add to calendar")), Place::wide());

    let hour = (i64::from(now.hour()) + 1) % 24;
    let form = Rc::new(Form {
        note,
        agenda: agenda.clone(),
        year: Field::new(theme, YEARS, 1, i64::from(now.year())),
        month: Field::new(theme, (1, 12), 1, i64::from(now.month())),
        day: Field::new(theme, (1, 31), 1, i64::from(now.day_of_month())),
        hour: Field::new(theme, (0, 23), 1, hour),
        minute: Field::new(theme, (0, 59), MINUTE_STEP, 0),
        length: Field::new(
            theme,
            (MINUTE_STEP, MAX_LENGTH_MINUTES),
            MINUTE_STEP,
            DEFAULT_LENGTH_MINUTES,
        ),
        is_all_day: Cell::new(true),
        targets: RefCell::default(),
        chosen_index: Cell::new(0),
        combo: ComboBox::new(theme),
        status: text::styled_sized("", pixel_size::SMALL),
        create_button: windowdialog::button(theme, &tr("Create")),
    });
    for field in [
        &form.year,
        &form.month,
        &form.day,
        &form.hour,
        &form.minute,
        &form.length,
    ] {
        dialog.keep(field.spin.clone());
    }

    dialog
        .column
        .add(&build_caption(&tr("Date")), Place::wide());
    dialog.column.add(
        &build_spin_row(&[&form.day, &form.month, &form.year]),
        Place::wide(),
    );

    let all_day_label = text::styled(&tr("All day"));
    text::set_color(&all_day_label, "colOnSurface");
    all_day_label.set_xalign(0.0);
    all_day_label.set_hexpand(true);
    let all_day_switch = Switch::new(theme);
    all_day_switch.set(true);
    let all_day_row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    all_day_row.append(&all_day_label);
    all_day_row.append(&all_day_switch.area);
    let toggle_click = gtk4::GestureClick::new();
    toggle_click.connect_released({
        let form = Rc::downgrade(&form);
        let all_day_switch = all_day_switch.clone();
        move |_, _, _, _| {
            let Some(form) = form.upgrade() else {
                return;
            };
            let is_all_day = !form.is_all_day.get();
            form.is_all_day.set(is_all_day);
            all_day_switch.set(is_all_day);
            form.set_time_enabled(!is_all_day);
        }
    });
    all_day_row.add_controller(toggle_click);
    pointer_cursor(all_day_row.upcast_ref());
    dialog.column.add(&all_day_row, Place::wide());

    dialog
        .column
        .add(&build_caption(&tr("Time")), Place::wide());
    dialog
        .column
        .add(&build_spin_row(&[&form.hour, &form.minute]), Place::wide());
    dialog
        .column
        .add(&build_caption(&tr("Duration (minutes)")), Place::wide());
    dialog
        .column
        .add(&build_spin_row(&[&form.length]), Place::wide());
    form.set_time_enabled(false);

    dialog
        .column
        .add(&build_caption(&tr("Calendar")), Place::wide());
    form.combo.button.set_hexpand(true);
    form.combo.set_items(&[String::new()], 0);
    form.combo.connect_activated({
        let form = Rc::downgrade(&form);
        move |index| {
            let Some(form) = form.upgrade() else {
                return;
            };
            form.chosen_index.set(index);
            form.combo
                .set_items(&list_names(&form.targets.borrow()), index as i32);
        }
    });
    dialog.column.add(&form.combo.button, Place::wide());
    dialog
        .column
        .add(&form.combo.use_inline_list(), Place::wide());
    dialog.keep(form.combo.clone());

    text::set_color(&form.status, "colError");
    form.status.set_wrap(true);
    form.status.set_xalign(0.0);
    form.status.set_visible(false);
    dialog.column.add(&form.status, Place::wide());

    let (buttons, place) = windowdialog::button_row();
    let cancel_button = windowdialog::button(theme, &tr("Cancel"));
    buttons.append(&windowdialog::spacer());
    buttons.append(&cancel_button);
    buttons.append(&form.create_button);
    dialog.column.add(&buttons, place);
    form.create_button.set_sensitive(false);

    cancel_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    dialog.keep(form.clone());
    form.create_button.connect_clicked({
        let form = Rc::downgrade(&form);
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            let Some(form) = form.upgrade() else {
                return;
            };
            let Some(target) = form
                .targets
                .borrow()
                .get(form.chosen_index.get())
                .map(|target| target.uid.clone())
            else {
                return;
            };
            form.create_button.set_sensitive(false);
            form.status.set_visible(false);
            let dialog = dialog.clone();
            glib::spawn_future_local(async move {
                let result = form.agenda.add_event(&target, &form.build_draft()).await;
                match result {
                    Ok(()) => {
                        if let Some(dialog) = dialog.upgrade() {
                            dialog.dismiss();
                        }
                    }
                    Err(message) => {
                        form.status.set_text(&message);
                        form.status.set_visible(true);
                        form.create_button.set_sensitive(true);
                    }
                }
            });
        }
    });

    glib::spawn_future_local({
        let form = form.clone();
        async move {
            let targets = form.agenda.read_targets().await;
            form.show_targets(targets);
        }
    });
    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_past_the_end_of_the_month_falls_back_to_its_last_day() {
        assert_eq!(clamp_day(2027, 2, 31), days_from_civil(2027, 2, 28));
        assert_eq!(clamp_day(2028, 2, 31), days_from_civil(2028, 2, 29));
        assert_eq!(clamp_day(2026, 11, 15), days_from_civil(2026, 11, 15));
    }
}

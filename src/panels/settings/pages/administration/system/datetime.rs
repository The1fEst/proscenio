use gtk4::prelude::*;
use gtk4::{gio, glib};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::platform::dbus;
use crate::services::session::timedate;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const TWENTY_FOUR_HOUR: &str = "hh:mm";
const MANUAL_PATTERNS: [&str; 2] = ["%Y-%m-%d", "%H:%M:%S"];
const NOTE_START: i32 = 8;
const DIALOG_WIDTH: f64 = 460.0;
const DIALOG_HEIGHT: f64 = 600.0;
const ZONE_SPACING: i32 = 10;
const ZONE_VERTICAL: i32 = 12;

struct Clock {
    page: Weak<Page>,
    state: RefCell<Option<timedate::State>>,
    problem: RefCell<Option<String>>,
    on_changed: RefCell<Option<Rc<dyn Fn()>>>,
    present: Rc<dyn Fn(Rc<WindowDialog>)>,
    subscription: RefCell<Option<gio::SignalSubscription>>,
}

impl Clock {
    fn refresh(self: &Rc<Self>) {
        let clock = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let state = timedate::state().await;
            if let Some(clock) = clock.upgrade() {
                clock.state.replace(state);
                clock.announce();
            }
        });
    }

    fn announce(&self) {
        let on_changed = self.on_changed.borrow().clone();
        if let Some(on_changed) = on_changed {
            on_changed();
        }
    }

    fn run(self: &Rc<Self>, call: impl Future<Output = Result<(), String>> + 'static) {
        let clock = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = call.await;
            if let Some(clock) = clock.upgrade() {
                clock.problem.replace(result.err());
                clock.refresh();
            }
        });
    }

    fn zone(&self) -> String {
        self.state
            .borrow()
            .as_ref()
            .map(|state| state.timezone.clone())
            .unwrap_or_default()
    }

    fn choose_zone(self: &Rc<Self>) {
        let clock = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let zones = timedate::zones().await;
            let Some(clock) = clock.upgrade() else {
                return;
            };
            let Some(page) = clock.page.upgrade() else {
                return;
            };
            let current_zone = clock.zone();
            let dialog = zone_dialog(&page.theme, &zones, &current_zone, {
                let clock = Rc::downgrade(&clock);
                move |zone| {
                    if let Some(clock) = clock.upgrade() {
                        clock.run(async move { timedate::set_timezone(&zone).await });
                    }
                }
            });
            (clock.present)(dialog);
        });
    }
}

fn zone_dialog(
    theme: &SharedTheme,
    zones: &[String],
    current_zone: &str,
    on_picked: impl Fn(String) + 'static,
) -> Rc<WindowDialog> {
    let dialog = WindowDialog::new(theme, Some(DIALOG_HEIGHT));
    dialog.set_background_width(DIALOG_WIDTH);
    dialog
        .column
        .add(&windowdialog::title(&tr("Time zone")), Place::default());
    let search = TextField::new(theme, Style::Outlined, &tr("Search time zones"));
    dialog.column.add(&search.root, Place::wide());
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let on_picked = Rc::new(on_picked);
    let mut rows: Vec<(String, RippleButton)> = Vec::new();
    for zone in zones {
        let item = windowdialog::list_item(theme, zone == current_zone);
        let row = Row::new(ZONE_SPACING);
        let name_label = text::styled(&zone.replace('_', " "));
        text::set_color(&name_label, "colOnLayer3");
        name_label.set_xalign(0.0);
        name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let name_box = Centered::filling_width(&name_label);
        name_box.set_hexpand(true);
        row.append(&name_box);
        let offset_label = text::styled_sized(&timedate::offset_name(zone), pixel_size::SMALLER);
        text::set_color(&offset_label, "colSubtext");
        row.append(&Centered::new(&offset_label));
        item.set_content(&row, windowdialog::PADDING as i32, ZONE_VERTICAL);
        item.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let on_picked = on_picked.clone();
            let zone = zone.clone();
            move |_| {
                on_picked(zone.clone());
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        list.append(&item);
        rows.push((zone.replace('_', " ").to_lowercase(), item));
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(&list));
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);
    dialog.column.add(
        &scroll,
        Place {
            left: -windowdialog::PADDING,
            right: -windowdialog::PADDING,
            fills_width: true,
            fills_height: true,
            ..Place::default()
        },
    );
    search.connect_changed({
        let search = Rc::downgrade(&search);
        move || {
            let Some(search) = search.upgrade() else {
                return;
            };
            let words: Vec<String> = search
                .text()
                .to_lowercase()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            for (name, item) in &rows {
                item.set_visible(words.iter().all(|word| name.contains(word.as_str())));
            }
        }
    });
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
    let (buttons, place) = windowdialog::button_row();
    buttons.append(&windowdialog::spacer());
    let cancel_button = windowdialog::button(theme, &tr("Cancel"));
    cancel_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&cancel_button);
    dialog.column.add(&buttons, place);
    glib::idle_add_local_once({
        let search = Rc::downgrade(&search);
        move || {
            if let Some(search) = search.upgrade() {
                search.grab_focus();
            }
        }
    });
    dialog.keep(search);
    dialog
}

fn system_clock(page: &Rc<Page>, context: &Context) {
    let section = page.section("schedule", &tr("Date & Time"));
    let now_label = text::styled("");
    text::set_color(&now_label, "colOnLayer1");
    now_label.set_xalign(0.0);
    now_label.set_margin_start(NOTE_START);
    section.append(&now_label);
    page.every(Duration::from_secs(1), {
        let now_label = now_label.downgrade();
        move || {
            let now_text = glib::DateTime::now_local()
                .and_then(|moment| moment.format("%A %-d %B %Y, %H:%M:%S"));
            if let (Some(now_label), Ok(now_text)) = (now_label.upgrade(), now_text) {
                now_label.set_text(&now_text);
            }
        }
    });

    let clock = Rc::new(Clock {
        page: Rc::downgrade(page),
        state: RefCell::new(None),
        problem: RefCell::new(None),
        on_changed: RefCell::new(None),
        present: Rc::new(context.dialog_presenter()),
        subscription: RefCell::new(None),
    });
    if let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        let weak = Rc::downgrade(&clock);
        clock
            .subscription
            .replace(Some(dbus::subscribe_properties_changed(
                &system,
                timedate::BUS,
                move || {
                    if let Some(clock) = weak.upgrade() {
                        clock.refresh();
                    }
                },
            )));
    }

    let automatic_switch = page.switch(&section, "sync", &tr("Set the time automatically"), {
        let clock = Rc::downgrade(&clock);
        move |is_on| {
            if let Some(clock) = clock.upgrade() {
                clock.run(async move { timedate::set_ntp(is_on).await });
            }
        }
    });
    automatic_switch.bind({
        let clock = Rc::downgrade(&clock);
        move || {
            clock
                .upgrade()
                .and_then(|clock| clock.state.borrow().as_ref().map(|state| state.uses_ntp))
                .unwrap_or(false)
        }
    });
    let sync_label = text::styled_sized("", pixel_size::SMALLER);
    text::set_color(&sync_label, "colSubtext");
    sync_label.set_xalign(0.0);
    sync_label.set_margin_start(NOTE_START);
    section.append(&sync_label);

    let zone_label = page.link_row(&section, "public", &tr("Time zone"), " ", {
        let clock = Rc::downgrade(&clock);
        move || {
            if let Some(clock) = clock.upgrade() {
                clock.choose_zone();
            }
        }
    });

    let manual_section = page.subsection(&section, &tr("Set the time"), "");
    let fields_row = page.row(&manual_section);
    let prefilled_texts = RefCell::new(
        glib::DateTime::now_local()
            .map(|now| MANUAL_PATTERNS.map(|pattern| format_moment(&now, pattern)))
            .unwrap_or_default(),
    );
    let date_field = TextField::new(&page.theme, Style::Outlined, &tr("Date (YYYY-MM-DD)"));
    date_field.set_text(&prefilled_texts.borrow()[0]);
    date_field.root.set_hexpand(true);
    fields_row.append(&date_field.root);
    let time_field = TextField::new(&page.theme, Style::Outlined, &tr("Time (HH:MM:SS)"));
    time_field.set_text(&prefilled_texts.borrow()[1]);
    time_field.root.set_hexpand(true);
    fields_row.append(&time_field.root);
    let (set_button, _) = page.icon_button("check", true, &tr("Set"), {
        let clock = Rc::downgrade(&clock);
        let (date_field, time_field) = (Rc::downgrade(&date_field), Rc::downgrade(&time_field));
        move || {
            let (Some(clock), Some(date_field), Some(time_field)) =
                (clock.upgrade(), date_field.upgrade(), time_field.upgrade())
            else {
                return;
            };
            let zone = clock.zone();
            if let Ok(now) = glib::DateTime::now(&glib::TimeZone::new(Some(&zone))) {
                let [date_text, time_text] = refresh_untouched(
                    [date_field.text(), time_field.text()],
                    &mut prefilled_texts.borrow_mut(),
                    &now,
                );
                date_field.set_text(&date_text);
                time_field.set_text(&time_text);
            }
            match timedate::parse_time(&date_field.text(), &time_field.text(), &zone) {
                Some(moment) => clock.run(async move { timedate::set_time(moment).await }),
                None => {
                    clock.problem.replace(Some(tr(
                        "Write the date as 2026-10-02 and the time as 14:05",
                    )));
                    clock.announce();
                }
            }
        }
    });
    set_button.set_valign(gtk4::Align::Center);
    fields_row.append(&set_button);
    page.keep(date_field);
    page.keep(time_field);

    let local_rtc_switch = page.switch(
        &section,
        "memory",
        &tr("Hardware clock keeps local time"),
        {
            let clock = Rc::downgrade(&clock);
            move |is_on| {
                if let Some(clock) = clock.upgrade() {
                    clock.run(async move { timedate::set_local_rtc(is_on).await });
                }
            }
        },
    );
    local_rtc_switch.bind({
        let clock = Rc::downgrade(&clock);
        move || {
            clock
                .upgrade()
                .and_then(|clock| {
                    clock
                        .state
                        .borrow()
                        .as_ref()
                        .map(|state| state.is_rtc_in_local_time)
                })
                .unwrap_or(false)
        }
    });
    page.tip(
        &local_rtc_switch.button,
        &tr("Turn on when Windows shares this computer and its clock comes up hours off"),
    );

    let problem_label = text::styled("");
    text::set_color(&problem_label, "colError");
    problem_label.set_xalign(0.0);
    problem_label.set_wrap(true);
    problem_label.set_margin_start(NOTE_START);
    problem_label.set_visible(false);
    section.append(&problem_label);

    let manual_root = Page::subsection_root(&manual_section);
    clock.on_changed.replace(Some(Rc::new({
        let clock = Rc::downgrade(&clock);
        let section = section.clone();
        move || {
            let Some(clock) = clock.upgrade() else {
                return;
            };
            let state = clock.state.borrow().clone();
            section.set_sensitive(state.is_some());
            let state = state.unwrap_or_default();
            automatic_switch.refresh();
            automatic_switch.set_enabled(state.can_use_ntp);
            local_rtc_switch.refresh();
            sync_label.set_visible(state.uses_ntp);
            sync_label.set_text(&if state.is_synchronized {
                tr("Synchronized with a time server")
            } else {
                tr("Not synchronized yet")
            });
            zone_label.set_text(&format!(
                "{} · {}",
                state.timezone.replace('_', " "),
                timedate::offset_name(&state.timezone)
            ));
            manual_root.set_visible(!state.uses_ntp);
            let message = clock.problem.borrow().clone();
            problem_label.set_visible(message.is_some());
            problem_label.set_text(&message.unwrap_or_default());
        }
    })));
    clock.refresh();
    page.keep(clock);
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    system_clock(&page, context);

    let main_section = page.section("", "");
    let format_section = page.subsection(&main_section, &tr("Time Format"), "");
    page.selection(
        &format_section,
        vec![
            Choice {
                label: tr("24h"),
                icon: "",
                value: Value::from(TWENTY_FOUR_HOUR),
            },
            Choice {
                label: tr("12h am/pm"),
                icon: "",
                value: Value::from("h:mm ap"),
            },
            Choice {
                label: tr("12h AM/PM"),
                icon: "",
                value: Value::from("h:mm AP"),
            },
        ],
        "/time/format",
        Value::from(config::TIME_FORMAT),
        |value| config::store_value("/time/format", value),
    );

    let clock_section = page.section("nest_clock_farsight_analog", &tr("Clock & Calendar"));
    let seconds_switch = page.config_switch(
        &clock_section,
        "pace",
        &tr("Seconds"),
        "/time/secondPrecision",
        false,
    );
    page.tip(
        &seconds_switch.button,
        &tr("Enable if you want clocks to show seconds accurately"),
    );
    let dates_section = page.subsection(
        &clock_section,
        &tr("Date formats"),
        &tr("Qt date format strings, see https://doc.qt.io/qt-6/qdate.html#toString"),
    );
    page.config_text(
        &dates_section,
        Style::Outlined,
        &tr("Date (e.g. ddd, dd/MM)"),
        "/time/dateFormat",
        config::DATE_FORMAT,
    );
    page.config_text(
        &dates_section,
        Style::Outlined,
        &tr("Short date (e.g. dd/MM)"),
        "/time/shortDateFormat",
        "dd/MM",
    );
    page.config_text(
        &dates_section,
        Style::Outlined,
        &tr("Date with year (e.g. dd/MM/yyyy)"),
        "/time/dateWithYearFormat",
        "dd/MM/yyyy",
    );

    let pomodoro_section = page.section("timer", &tr("Pomodoro"));
    let first_row = page.uniform_row(&pomodoro_section);
    page.config_spin_multiple(
        &first_row,
        "target",
        &tr("Focus (min)"),
        "/time/pomodoro/focus",
        1500,
        60,
        (1, 180),
        5,
    );
    page.config_spin_multiple(
        &first_row,
        "coffee",
        &tr("Break (min)"),
        "/time/pomodoro/breakTime",
        300,
        60,
        (1, 60),
        1,
    );
    let second_row = page.uniform_row(&pomodoro_section);
    page.config_spin_multiple(
        &second_row,
        "airline_seat_recline_extra",
        &tr("Long break (min)"),
        "/time/pomodoro/longBreak",
        900,
        60,
        (1, 120),
        5,
    );
    let (cycles_row, _) = page.config_spin(
        &second_row,
        "repeat",
        &tr("Cycles before long break"),
        "/time/pomodoro/cyclesBeforeLongBreak",
        4,
        (1, 12),
        1,
    );
    page.tip(&cycles_row, &tr("Cycles before long break"));
    page
}

fn format_moment(moment: &glib::DateTime, pattern: &str) -> String {
    moment
        .format(pattern)
        .map(|text| text.to_string())
        .unwrap_or_default()
}

fn refresh_untouched(
    mut texts: [String; 2],
    prefilled_texts: &mut [String; 2],
    now: &glib::DateTime,
) -> [String; 2] {
    for ((text, prefilled_text), pattern) in
        texts.iter_mut().zip(prefilled_texts).zip(MANUAL_PATTERNS)
    {
        if text == prefilled_text {
            *text = format_moment(now, pattern);
            prefilled_text.clone_from(text);
        }
    }
    texts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_field_takes_the_current_value_when_set() {
        let now = glib::DateTime::new(&glib::TimeZone::utc(), 2026, 10, 14, 10, 20, 0.0).unwrap();
        let mut prefilled_texts = ["2026-10-14".to_owned(), "10:00:00".to_owned()];
        assert_eq!(
            refresh_untouched(
                ["2026-10-01".to_owned(), "10:00:00".to_owned()],
                &mut prefilled_texts,
                &now
            ),
            ["2026-10-01", "10:20:00"]
        );
        assert_eq!(prefilled_texts, ["2026-10-14", "10:20:00"]);
        assert_eq!(
            refresh_untouched(
                ["2026-10-14".to_owned(), "08:00".to_owned()],
                &mut prefilled_texts,
                &now
            ),
            ["2026-10-14", "08:00"]
        );
    }
}

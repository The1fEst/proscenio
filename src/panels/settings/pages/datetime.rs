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
use crate::services::timedate;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

const TWENTY_FOUR_HOUR: &str = "hh:mm";
const NOTE_START: i32 = 8;
const DIALOG_WIDTH: f64 = 460.0;
const DIALOG_HEIGHT: f64 = 600.0;
const ZONE_SPACING: i32 = 10;
const ZONE_VERTICAL: i32 = 12;

struct Clock {
    page: Weak<Page>,
    state: RefCell<Option<timedate::State>>,
    problem: RefCell<Option<String>>,
    changed: RefCell<Option<Rc<dyn Fn()>>>,
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
        let changed = self.changed.borrow().clone();
        if let Some(changed) = changed {
            changed();
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
            let current = clock.zone();
            let dialog = zone_dialog(&page.theme, &zones, &current, {
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
    current: &str,
    picked: impl Fn(String) + 'static,
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
    let picked = Rc::new(picked);
    let mut rows: Vec<(String, RippleButton)> = Vec::new();
    for zone in zones {
        let item = windowdialog::list_item(theme, zone == current);
        let row = Row::new(ZONE_SPACING);
        let name = text::styled(&zone.replace('_', " "));
        text::set_color(&name, "colOnLayer3");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let name = Centred::filling_width(&name);
        name.set_hexpand(true);
        row.append(&name);
        let offset = text::styled_sized(&timedate::offset_name(zone), pixel_size::SMALLER);
        text::set_color(&offset, "colSubtext");
        row.append(&Centred::new(&offset));
        item.set_content(&row, windowdialog::PADDING as i32, ZONE_VERTICAL);
        item.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let picked = picked.clone();
            let zone = zone.clone();
            move |_| {
                picked(zone.clone());
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
    crate::ui::widgets::flickable::follow_scroll_settings(&scroll);
    dialog.column.add(
        &scroll,
        Place {
            left: -windowdialog::PADDING,
            right: -windowdialog::PADDING,
            fill_width: true,
            fill_height: true,
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
    let cancel = windowdialog::button(theme, &tr("Cancel"));
    cancel.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&cancel);
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
    let now = text::styled("");
    text::set_color(&now, "colOnLayer1");
    now.set_xalign(0.0);
    now.set_margin_start(NOTE_START);
    section.append(&now);
    page.every(Duration::from_secs(1), {
        let now = now.downgrade();
        move || {
            let shown = glib::DateTime::now_local()
                .and_then(|moment| moment.format("%A %-d %B %Y, %H:%M:%S"));
            if let (Some(now), Ok(shown)) = (now.upgrade(), shown) {
                now.set_text(&shown);
            }
        }
    });

    let clock = Rc::new(Clock {
        page: Rc::downgrade(page),
        state: RefCell::new(None),
        problem: RefCell::new(None),
        changed: RefCell::new(None),
        present: Rc::new(context.dialog_presenter()),
        subscription: RefCell::new(None),
    });
    if let Ok(system) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        let weak = Rc::downgrade(&clock);
        clock.subscription.replace(Some(dbus::on_properties_changed(
            &system,
            timedate::BUS,
            move || {
                if let Some(clock) = weak.upgrade() {
                    clock.refresh();
                }
            },
        )));
    }

    let automatic = page.switch(&section, "sync", &tr("Set the time automatically"), {
        let clock = Rc::downgrade(&clock);
        move |on| {
            if let Some(clock) = clock.upgrade() {
                clock.run(async move { timedate::set_ntp(on).await });
            }
        }
    });
    automatic.bind({
        let clock = Rc::downgrade(&clock);
        move || {
            clock
                .upgrade()
                .and_then(|clock| clock.state.borrow().as_ref().map(|state| state.ntp))
                .unwrap_or(false)
        }
    });
    let synced = text::styled_sized("", pixel_size::SMALLER);
    text::set_color(&synced, "colSubtext");
    synced.set_xalign(0.0);
    synced.set_margin_start(NOTE_START);
    section.append(&synced);

    let zone = page.link_row(&section, "public", &tr("Time zone"), " ", {
        let clock = Rc::downgrade(&clock);
        move || {
            if let Some(clock) = clock.upgrade() {
                clock.choose_zone();
            }
        }
    });

    let manual = page.subsection(&section, &tr("Set the time"), "");
    let fields = page.row(&manual);
    let today = glib::DateTime::now_local().ok();
    let format = |pattern: &str| {
        today
            .as_ref()
            .and_then(|moment| moment.format(pattern).ok())
            .map(|text| text.to_string())
            .unwrap_or_default()
    };
    let date = TextField::new(&page.theme, Style::Outlined, &tr("Date (YYYY-MM-DD)"));
    date.set_text(&format("%Y-%m-%d"));
    date.root.set_hexpand(true);
    fields.append(&date.root);
    let time = TextField::new(&page.theme, Style::Outlined, &tr("Time (HH:MM:SS)"));
    time.set_text(&format("%H:%M:%S"));
    time.root.set_hexpand(true);
    fields.append(&time.root);
    let (set, _) = page.icon_button("check", true, &tr("Set"), {
        let clock = Rc::downgrade(&clock);
        let (date, time) = (Rc::downgrade(&date), Rc::downgrade(&time));
        move || {
            let (Some(clock), Some(date), Some(time)) =
                (clock.upgrade(), date.upgrade(), time.upgrade())
            else {
                return;
            };
            match timedate::parse_time(&date.text(), &time.text(), &clock.zone()) {
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
    set.set_valign(gtk4::Align::Center);
    fields.append(&set);
    page.keep(date);
    page.keep(time);

    let local = page.switch(
        &section,
        "memory",
        &tr("Hardware clock keeps local time"),
        {
            let clock = Rc::downgrade(&clock);
            move |on| {
                if let Some(clock) = clock.upgrade() {
                    clock.run(async move { timedate::set_local_rtc(on).await });
                }
            }
        },
    );
    local.bind({
        let clock = Rc::downgrade(&clock);
        move || {
            clock
                .upgrade()
                .and_then(|clock| clock.state.borrow().as_ref().map(|state| state.local_rtc))
                .unwrap_or(false)
        }
    });
    page.tip(
        &local.button,
        &tr("Turn on when Windows shares this computer and its clock comes up hours off"),
    );

    let problem = text::styled("");
    text::set_color(&problem, "colError");
    problem.set_xalign(0.0);
    problem.set_wrap(true);
    problem.set_margin_start(NOTE_START);
    problem.set_visible(false);
    section.append(&problem);

    let manual_root = Page::subsection_root(&manual);
    clock.changed.replace(Some(Rc::new({
        let clock = Rc::downgrade(&clock);
        let section = section.clone();
        move || {
            let Some(clock) = clock.upgrade() else {
                return;
            };
            let state = clock.state.borrow().clone();
            section.set_sensitive(state.is_some());
            let state = state.unwrap_or_default();
            automatic.refresh();
            automatic.set_enabled(state.can_ntp);
            local.refresh();
            synced.set_visible(state.ntp);
            synced.set_text(&if state.synchronized {
                tr("Synchronized with a time server")
            } else {
                tr("Not synchronized yet")
            });
            zone.set_text(&format!(
                "{} · {}",
                state.timezone.replace('_', " "),
                timedate::offset_name(&state.timezone)
            ));
            manual_root.set_visible(!state.ntp);
            let message = clock.problem.borrow().clone();
            problem.set_visible(message.is_some());
            problem.set_text(&message.unwrap_or_default());
        }
    })));
    clock.refresh();
    page.keep(clock);
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    system_clock(&page, context);

    let main = page.section("", "");
    let format = page.subsection(&main, &tr("Time Format"), "");
    page.selection(
        &format,
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
        Value::from("h:mm AP"),
        |value| {
            let twelve_hour = value.as_str() != Some(TWENTY_FOUR_HOUR);
            switch_hyprlock_clock(twelve_hour);
            config::store_value("/time/format", value);
        },
    );

    let clock = page.section("nest_clock_farsight_analog", &tr("Clock & Calendar"));
    let seconds = page.config_switch(
        &clock,
        "pace",
        &tr("Seconds"),
        "/time/secondPrecision",
        false,
    );
    page.tip(
        &seconds.button,
        &tr("Enable if you want clocks to show seconds accurately"),
    );
    let dates = page.subsection(
        &clock,
        &tr("Date formats"),
        &tr("Qt date format strings, see https://doc.qt.io/qt-6/qdate.html#toString"),
    );
    page.config_text(
        &dates,
        Style::Outlined,
        &tr("Date (e.g. ddd, dd/MM)"),
        "/time/dateFormat",
        "ddd d MMM",
    );
    page.config_text(
        &dates,
        Style::Outlined,
        &tr("Short date (e.g. dd/MM)"),
        "/time/shortDateFormat",
        "dd/MM",
    );
    page.config_text(
        &dates,
        Style::Outlined,
        &tr("Date with year (e.g. dd/MM/yyyy)"),
        "/time/dateWithYearFormat",
        "dd/MM/yyyy",
    );

    let pomodoro = page.section("timer", &tr("Pomodoro"));
    let first = page.uniform_row(&pomodoro);
    page.config_spin_multiple(
        &first,
        "target",
        &tr("Focus (min)"),
        "/time/pomodoro/focus",
        1500,
        60,
        (1, 180),
        5,
    );
    page.config_spin_multiple(
        &first,
        "coffee",
        &tr("Break (min)"),
        "/time/pomodoro/breakTime",
        300,
        60,
        (1, 60),
        1,
    );
    let second = page.uniform_row(&pomodoro);
    page.config_spin_multiple(
        &second,
        "airline_seat_recline_extra",
        &tr("Long break (min)"),
        "/time/pomodoro/longBreak",
        900,
        60,
        (1, 120),
        5,
    );
    let (cycles, _) = page.config_spin(
        &second,
        "repeat",
        &tr("Cycles before long break"),
        "/time/pomodoro/cyclesBeforeLongBreak",
        4,
        (1, 12),
        1,
    );
    page.tip(&cycles, &tr("Cycles before long break"));
    page
}

fn switch_hyprlock_clock(twelve_hour: bool) {
    let path = glib::user_config_dir().join("hypr/hyprlock.conf");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let (from, to) = if twelve_hour {
        ("TIME", "TIME12")
    } else {
        ("TIME12", "TIME")
    };
    let switched = replace_word_ends(&text, from, to);
    if switched != text {
        let _ = std::fs::write(&path, switched);
    }
}

/// The first `from` on each line that ends at a word boundary becomes `to`, the substitution the QML shell runs.
fn replace_word_ends(text: &str, from: &str, to: &str) -> String {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.split_inclusive('\n')
        .map(|line| {
            let found = line
                .match_indices(from)
                .find(|(at, _)| !line[at + from.len()..].chars().next().is_some_and(word));
            match found {
                Some((at, _)) => format!("{}{to}{}", &line[..at], &line[at + from.len()..]),
                None => line.to_owned(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprlock_clock_words_switch_like_sed() {
        let cases = [
            ("text = $TIME\n", "TIME", "TIME12", "text = $TIME12\n"),
            ("text = $TIME12\n", "TIME", "TIME12", "text = $TIME12\n"),
            ("text = $TIME12\n", "TIME12", "TIME", "text = $TIME\n"),
            (
                "$TIME $TIME\n$TIME",
                "TIME",
                "TIME12",
                "$TIME12 $TIME\n$TIME12",
            ),
            ("TIMES TIME", "TIME", "TIME12", "TIMES TIME12"),
        ];
        for (text, from, to, expected) in cases {
            assert_eq!(replace_word_ends(text, from, to), expected, "{text:?}");
        }
    }
}

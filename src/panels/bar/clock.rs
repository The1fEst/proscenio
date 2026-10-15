use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::i18n::{tr, trf};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::services::session::background::BackgroundTasks;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::{display::text, overlay::popup};

const VERTICAL_PIECE_SPACING: i32 = -4;
const VERTICAL_AM_PM_SPACING: i32 = 6;

pub fn build(
    config: &Config,
    shows_date: bool,
    background: &Rc<BackgroundTasks>,
    on_tick: impl Fn(&glib::DateTime) + 'static,
    scope: &Scope,
) -> gtk4::Widget {
    let (area, face, date_format_selector) = if config.vertical {
        vertical_face()
    } else {
        horizontal_face(shows_date)
    };

    let details = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let (today, today_label) = popup::header_with_label("calendar_month", "");
    details.append(&today);
    let (uptime_row, uptime_value) = popup::value("timelapse", &tr("System uptime:"), &uptime());
    details.append(&uptime_row);
    let tasks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    tasks.append(&popup::value("checklist", &tr("To Do:"), "").0);
    let tasks_note = popup::note(&pending_tasks());
    tasks.append(&tasks_note);
    details.append(&tasks);

    let show: Rc<dyn Fn()> = Rc::new(move || {
        let config = config::current();
        let now = glib::DateTime::now_local().expect("local time");
        face(&now, &config);
        if let Ok(text) = now.format("%A, %B %d, %Y") {
            today_label.set_text(&text);
        }
        on_tick(&now);
    });
    show();

    scope.keep(background.add_scoped("clock details", move || {
        uptime_value.set_text(&uptime());
        tasks_note.set_text(&pending_tasks());
        Ok(())
    }));

    let ticker: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    arm(&ticker, &show, date_format_selector);
    scope.defer({
        let ticker = ticker.clone();
        move || {
            if let Some(source) = ticker.take() {
                source.remove();
            }
        }
    });
    scope.hold(watch::config("/time", move || {
        show();
        arm(&ticker, &show, date_format_selector);
    }));

    popup::attach(&area, popup::Bar::of(config), &details);

    area
}

type Face = Box<dyn Fn(&glib::DateTime, &Config)>;
type DateFormatSelector = fn(&Config) -> &str;

fn horizontal_face(shows_date: bool) -> (gtk4::Widget, Face, DateFormatSelector) {
    let time = text::styled_sized("", pixel_size::LARGE);
    text::set_color(&time, "colOnLayer1");

    let separator = text::styled("•");
    text::set_color(&separator, "colOnLayer1");
    let date = text::styled("");
    text::set_color(&date, "colOnLayer1");

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    row.append(&Centered::optical(&time));
    if shows_date {
        row.append(&Centered::optical(&separator));
        row.append(&Centered::optical(&date));
    }
    let area = Centered::new(&row);
    area.set_hexpand(true);
    area.set_size_request(-1, crate::core::config::BASE_BAR_HEIGHT);

    let date_format_selector: DateFormatSelector = if shows_date {
        |config| config.date_format.as_str()
    } else {
        |_| ""
    };
    let face = move |now: &glib::DateTime, config: &Config| {
        if let Ok(text) = now.format(&strftime_from_qt(&config.time_format)) {
            time.set_text(&text);
        }
        if let Ok(text) = now.format(&strftime_from_qt(date_format_selector(config))) {
            date.set_text(&text);
        }
    };
    (area.upcast(), Box::new(face), date_format_selector)
}

fn vertical_face() -> (gtk4::Widget, Face, DateFormatSelector) {
    let pieces = Column::new(VERTICAL_PIECE_SPACING, true);
    let date = text::styled_sized("", pixel_size::SMALLEST);
    text::set_color(&date, "colOnLayer1");
    let column = Column::new(0, true);
    column.append(&pieces);
    column.append(&date);

    let area = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.set_halign(gtk4::Align::Center);
    area.append(&column);

    let date_format_selector: DateFormatSelector = |config| config.short_date_format.as_str();
    let labels: RefCell<Vec<gtk4::Label>> = RefCell::new(Vec::new());
    let face = move |now: &glib::DateTime, config: &Config| {
        let Ok(time) = now.format(&strftime_from_qt(&config.time_format)) else {
            return;
        };
        let lower = time.to_lowercase();
        column.set_spacing(if lower.contains("am") || lower.contains("pm") {
            VERTICAL_AM_PM_SPACING
        } else {
            0
        });
        let parts = time_parts(&time);
        let mut labels = labels.borrow_mut();
        while labels.len() > parts.len() {
            if let Some(label) = labels.pop() {
                pieces.remove(&label);
            }
        }
        while labels.len() < parts.len() {
            let label = text::styled("");
            text::set_color(&label, "colOnLayer1");
            pieces.append(&label);
            labels.push(label);
        }
        for (label, (part, size)) in labels.iter().zip(parts) {
            text::set_font(label, text::Family::Main, size as f64, "wght=450");
            label.set_text(&part);
        }
        if let Ok(text) = now.format(&strftime_from_qt(date_format_selector(config))) {
            date.set_text(&text);
        }
    };
    (area.upcast(), Box::new(face), date_format_selector)
}

fn time_parts(time: &str) -> Vec<(String, i32)> {
    time.split([':', ' '])
        .map(|part| {
            let lower = part.to_lowercase();
            let size = if lower.contains("am") || lower.contains("pm") {
                pixel_size::SMALLER
            } else {
                pixel_size::LARGE
            };
            (format!("{part:0>2}"), size)
        })
        .collect()
}

fn arm(
    ticker: &Rc<RefCell<Option<glib::SourceId>>>,
    show: &Rc<dyn Fn()>,
    date_format_selector: DateFormatSelector,
) {
    if let Some(source) = ticker.take() {
        source.remove();
    }
    let config = config::current();
    let period_seconds = tick_period_seconds(
        config.time_second_precision,
        &config.time_format,
        date_format_selector(&config),
    );
    let (second, microsecond) = glib::DateTime::now_local().map_or((0, 0), |now| {
        (now.second() as u64, now.microsecond() as u64)
    });
    let delay = time_to_next_tick(second, microsecond, period_seconds);
    let (ticker_slot, show) = (ticker.clone(), show.clone());
    let source = glib::timeout_add_local_once(delay + Duration::from_millis(1), move || {
        ticker_slot.take();
        show();
        arm(&ticker_slot, &show, date_format_selector);
    });
    ticker.replace(Some(source));
}

pub(crate) fn tick_period_seconds(
    uses_second_precision: bool,
    time_format: &str,
    date_format: &str,
) -> u64 {
    if uses_second_precision || time_format.contains('s') || date_format.contains('s') {
        1
    } else {
        60
    }
}

pub(crate) fn time_to_next_tick(second: u64, microsecond: u64, period_seconds: u64) -> Duration {
    let elapsed_micros = (second % period_seconds) * 1_000_000 + microsecond;
    Duration::from_micros(period_seconds * 1_000_000 - elapsed_micros)
}

fn uptime() -> String {
    let seconds = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as i64;
    let (days, hours, minutes) = (
        seconds / 86400,
        (seconds % 86400) / 3600,
        (seconds % 3600) / 60,
    );

    let mut text = String::new();
    if days > 0 {
        text.push_str(&format!("{days}d"));
    }
    if hours > 0 {
        if !text.is_empty() {
            text.push_str(", ");
        }
        text.push_str(&format!("{hours}h"));
    }
    if minutes > 0 || text.is_empty() {
        if !text.is_empty() {
            text.push_str(", ");
        }
        text.push_str(&format!("{minutes}m"));
    }
    text
}

fn pending_tasks() -> String {
    let list = std::fs::read_to_string(crate::services::session::todo::path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();

    let unfinished_tasks: Vec<&str> = list
        .iter()
        .filter(|item| item.get("done").and_then(Value::as_bool) != Some(true))
        .filter_map(|item| item.get("content").and_then(Value::as_str))
        .collect();
    if unfinished_tasks.is_empty() {
        return tr("No pending tasks");
    }

    let mut text = unfinished_tasks
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, content)| format!("  {}. {content}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    if unfinished_tasks.len() > 5 {
        text.push_str(&format!(
            "\n  {}",
            trf(
                "... and %1 more",
                &[&(unfinished_tasks.len() - 5).to_string()]
            )
        ));
    }
    text
}

pub fn strftime_from_qt(pattern: &str) -> String {
    let is_twelve_hour = pattern.contains("AP") || pattern.contains("ap") || pattern.contains('A');
    let mut strftime_pattern = String::with_capacity(pattern.len() * 2);
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index..].starts_with(&['A', 'P']) {
            strftime_pattern.push_str("%p");
            index += 2;
            continue;
        }
        if chars[index..].starts_with(&['a', 'p']) {
            strftime_pattern.push_str("%P");
            index += 2;
            continue;
        }
        let current = chars[index];
        let run = chars[index..]
            .iter()
            .take_while(|next| **next == current)
            .count();
        let token: &str = match (current, run) {
            ('h', 1) if is_twelve_hour => "%-I",
            ('h', _) if is_twelve_hour => "%I",
            ('h', 1) => "%-H",
            ('h', _) => "%H",
            ('H', 1) => "%-H",
            ('H', _) => "%H",
            ('m', 1) => "%-M",
            ('m', _) => "%M",
            ('s', 1) => "%-S",
            ('s', _) => "%S",
            ('d', 1) => "%-d",
            ('d', 2) => "%d",
            ('d', 3) => "%a",
            ('d', _) => "%A",
            ('M', 1) => "%-m",
            ('M', 2) => "%m",
            ('M', 3) => "%b",
            ('M', _) => "%B",
            ('y', 2) => "%y",
            ('y', _) => "%Y",
            ('A', _) => "%p",
            ('a', _) => "%P",
            ('%', _) => "%%",
            _ => {
                strftime_pattern.extend(std::iter::repeat_n(current, run));
                index += run;
                continue;
            }
        };
        strftime_pattern.push_str(token);
        index += run;
    }
    strftime_pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_time_splits_into_padded_parts_with_a_smaller_am_pm() {
        assert_eq!(
            time_parts("9:05 PM"),
            vec![
                ("09".to_owned(), pixel_size::LARGE),
                ("05".to_owned(), pixel_size::LARGE),
                ("PM".to_owned(), pixel_size::SMALLER),
            ]
        );
        assert_eq!(
            time_parts("14:32"),
            vec![
                ("14".to_owned(), pixel_size::LARGE),
                ("32".to_owned(), pixel_size::LARGE),
            ]
        );
    }

    #[test]
    fn the_next_tick_lands_on_the_next_wall_clock_boundary() {
        assert_eq!(
            time_to_next_tick(30, 900_000, 60),
            Duration::from_micros(29_100_000)
        );
        assert_eq!(
            time_to_next_tick(59, 999_000, 60),
            Duration::from_micros(1_000)
        );
        assert_eq!(time_to_next_tick(0, 0, 60), Duration::from_secs(60));
        assert_eq!(
            time_to_next_tick(12, 250_000, 1),
            Duration::from_micros(750_000)
        );
    }

    #[test]
    fn the_clock_ticks_every_second_when_its_format_shows_seconds() {
        assert_eq!(tick_period_seconds(false, "hh:mm", "dd/MM"), 60);
        assert_eq!(tick_period_seconds(false, "hh:mm:ss", "dd/MM"), 1);
        assert_eq!(tick_period_seconds(false, "hh:mm", "dd/MM ss"), 1);
        assert_eq!(tick_period_seconds(true, "hh:mm", "dd/MM"), 1);
    }
}

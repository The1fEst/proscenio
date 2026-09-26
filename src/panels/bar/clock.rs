use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::services::background::BackgroundTasks;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::column::Column;
use crate::ui::widgets::{popup, text};

const VERTICAL_PIECE_SPACING: i32 = -4;
const VERTICAL_AM_PM_SPACING: i32 = 6;

pub fn build(
    config: &Config,
    show_date: bool,
    background: &Rc<BackgroundTasks>,
    scope: &Scope,
) -> gtk4::Widget {
    let (area, face) = if config.vertical {
        vertical_face()
    } else {
        horizontal_face(show_date)
    };

    let details = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let (today, today_label) = popup::header_with_label("calendar_month", "");
    details.append(&today);
    let (uptime_row, uptime_value) = popup::value("timelapse", "System uptime:", &uptime());
    details.append(&uptime_row);
    let tasks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    tasks.append(&popup::value("checklist", "To Do:", "").0);
    let pending = popup::note(&pending_tasks());
    tasks.append(&pending);
    details.append(&tasks);

    let show: Rc<dyn Fn()> = Rc::new(move || {
        let config = config::current();
        let now = glib::DateTime::now_local().expect("local time");
        face(&now, &config);
        if let Ok(text) = now.format("%A, %B %d, %Y") {
            today_label.set_text(&text);
        }
    });
    show();

    scope.keep(background.add_scoped("clock details", move || {
        uptime_value.set_text(&uptime());
        pending.set_text(&pending_tasks());
        Ok(())
    }));

    let ticker: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    arm(&ticker, &show);
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
        arm(&ticker, &show);
    }));

    popup::attach(&area, popup::Bar::of(config), &details);

    area
}

type Face = Box<dyn Fn(&glib::DateTime, &Config)>;

fn horizontal_face(show_date: bool) -> (gtk4::Widget, Face) {
    let time = text::styled_sized("", pixel_size::LARGE);
    text::set_color(&time, "colOnLayer1");

    let separator = text::styled("•");
    text::set_color(&separator, "colOnLayer1");
    let date = text::styled("");
    text::set_color(&date, "colOnLayer1");

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    row.append(&Centred::optical(&time));
    if show_date {
        row.append(&Centred::optical(&separator));
        row.append(&Centred::optical(&date));
    }
    let area = Centred::new(&row);
    area.set_hexpand(true);
    area.set_size_request(-1, crate::core::config::BASE_BAR_HEIGHT);

    let face = move |now: &glib::DateTime, config: &Config| {
        if let Ok(text) = now.format(&strftime_from_qt(&config.time_format)) {
            time.set_text(&text);
        }
        if let Ok(text) = now.format(&strftime_from_qt(&config.date_format)) {
            date.set_text(&text);
        }
    };
    (area.upcast(), Box::new(face))
}

fn vertical_face() -> (gtk4::Widget, Face) {
    let pieces = Column::new(VERTICAL_PIECE_SPACING, true);
    let date = text::styled_sized("", pixel_size::SMALLEST);
    text::set_color(&date, "colOnLayer1");
    let column = Column::new(0, true);
    column.append(&pieces);
    column.append(&date);

    let area = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.set_halign(gtk4::Align::Center);
    area.append(&column);

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
        if let Ok(text) = now.format(&strftime_from_qt(&config.short_date_format)) {
            date.set_text(&text);
        }
    };
    (area.upcast(), Box::new(face))
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

fn arm(ticker: &Rc<RefCell<Option<glib::SourceId>>>, show: &Rc<dyn Fn()>) {
    if let Some(source) = ticker.take() {
        source.remove();
    }
    let period = if config::current().time_second_precision {
        1
    } else {
        60
    };
    let second = glib::DateTime::now_local().map_or(0, |now| now.second() as u32);
    let to_next = if period == 1 { 1 } else { 60 - second };
    let (later, show) = (ticker.clone(), show.clone());
    let source = glib::timeout_add_local_once(Duration::from_secs(to_next as u64), move || {
        show();
        let source = glib::timeout_add_seconds_local(period, move || {
            show();
            glib::ControlFlow::Continue
        });
        later.replace(Some(source));
    });
    ticker.replace(Some(source));
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

    let mut out = String::new();
    if days > 0 {
        out.push_str(&format!("{days}d"));
    }
    if hours > 0 {
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(&format!("{hours}h"));
    }
    if minutes > 0 || out.is_empty() {
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(&format!("{minutes}m"));
    }
    out
}

fn pending_tasks() -> String {
    let list = std::fs::read_to_string(crate::services::todo::path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();

    let pending: Vec<&str> = list
        .iter()
        .filter(|item| item.get("done").and_then(Value::as_bool) != Some(true))
        .filter_map(|item| item.get("content").and_then(Value::as_str))
        .collect();
    if pending.is_empty() {
        return "No pending tasks".to_owned();
    }

    let mut text = pending
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, content)| format!("  {}. {content}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    if pending.len() > 5 {
        text.push_str(&format!("\n  ... and {} more", pending.len() - 5));
    }
    text
}

pub fn strftime_from_qt(pattern: &str) -> String {
    let twelve_hour = pattern.contains("AP") || pattern.contains("ap") || pattern.contains('A');
    let mut out = String::with_capacity(pattern.len() * 2);
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index..].starts_with(&['A', 'P']) {
            out.push_str("%p");
            index += 2;
            continue;
        }
        if chars[index..].starts_with(&['a', 'p']) {
            out.push_str("%P");
            index += 2;
            continue;
        }
        let current = chars[index];
        let run = chars[index..]
            .iter()
            .take_while(|next| **next == current)
            .count();
        let token: &str = match (current, run) {
            ('h', 1) if twelve_hour => "%-I",
            ('h', _) if twelve_hour => "%I",
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
                out.extend(std::iter::repeat_n(current, run));
                index += run;
                continue;
            }
        };
        out.push_str(token);
        index += run;
    }
    out
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
}

use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::bar::clock::{strftime_from_qt, strip_seconds};
use crate::services::session::agenda::Occurrence;
use crate::services::session::agenda::civil::SECONDS_PER_DAY;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::group::pointer_cursor;

const LIST_HEIGHT: i32 = 100;
const SPACING: i32 = 5;
const CARD_PADDING: i32 = 6;
const CARD_MARGIN: i32 = 10;
const BAR_WIDTH: i32 = 4;
const TITLE_MAX_CHARS: i32 = 32;
const MAX_DOTS: usize = 3;
const DOT_SIZE: f32 = 5.0;
const DOT_GAP: f32 = 2.0;

pub struct EventList {
    pub widget: gtk4::Widget,
    title: gtk4::Label,
    list: gtk4::Box,
    theme: SharedTheme,
    open_note: Rc<dyn Fn(u64) -> bool>,
}

impl EventList {
    pub fn show(&self, day: i64, occurrences: &[Occurrence]) {
        let heading: String = glib::DateTime::from_unix_utc(day * SECONDS_PER_DAY)
            .and_then(|date| date.format("%A, %B %-d"))
            .map(Into::into)
            .unwrap_or_default();
        self.title.set_text(&heading);
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        if occurrences.is_empty() {
            let label = text::styled(&tr("No events"));
            text::set_color(&label, "m3outline");
            label.set_margin_top(CARD_PADDING);
            self.list.append(&Centered::new(&label));
            return;
        }
        let time_pattern = strip_seconds(&strftime_from_qt(&config::current().time_format));
        for occurrence in occurrences {
            self.list.append(&build_card(
                &self.theme,
                occurrence,
                &time_pattern,
                self.open_note.clone(),
            ));
        }
    }
}

pub fn build_list(theme: &SharedTheme, open_note: Rc<dyn Fn(u64) -> bool>) -> Rc<EventList> {
    let title = text::styled(" ");
    text::set_color(&title, "colOnLayer1");
    title.set_xalign(0.0);

    let list = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
    list.set_valign(gtk4::Align::Start);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_min_content_height(LIST_HEIGHT);
    scroll.set_max_content_height(LIST_HEIGHT);
    scroll.set_child(Some(&list));
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
    column.append(&title);
    column.append(&scroll);
    Rc::new(EventList {
        widget: column.upcast(),
        title,
        list,
        theme: theme.clone(),
        open_note,
    })
}

fn build_card(
    theme: &SharedTheme,
    occurrence: &Occurrence,
    time_pattern: &str,
    open_note: Rc<dyn Fn(u64) -> bool>,
) -> gtk4::Widget {
    let summary = if occurrence.summary.is_empty() {
        tr("No title")
    } else {
        occurrence.summary.clone()
    };
    let title = text::styled(&summary);
    text::set_color(&title, "colOnLayer2");
    title.set_xalign(0.0);
    title.set_ellipsize(pango::EllipsizeMode::End);
    title.set_max_width_chars(TITLE_MAX_CHARS);

    let details = text::styled_sized(
        &format_details(occurrence, time_pattern),
        pixel_size::SMALLER,
    );
    text::set_color(&details, "m3outline");
    details.set_xalign(0.0);
    details.set_ellipsize(pango::EllipsizeMode::End);
    details.set_max_width_chars(TITLE_MAX_CHARS);

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_margin_top(CARD_PADDING);
    lines.set_margin_bottom(CARD_PADDING);
    lines.append(&title);
    lines.append(&details);

    let color = occurrence.color.as_deref().and_then(parse_color);
    let theme = theme.clone();
    let bar = Paint::new(move |snapshot, width, height| {
        let color = color.unwrap_or_else(|| theme.borrow().colors.col_primary);
        fill_rounded(snapshot, &color, 0.0, 0.0, width, height);
    });
    bar.set_size_request(BAR_WIDTH, -1);
    bar.set_margin_top(CARD_PADDING);
    bar.set_margin_bottom(CARD_PADDING);
    bar.set_margin_start(CARD_PADDING);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, CARD_PADDING);
    row.add_css_class("calendar-event");
    row.append(&bar);
    row.append(&lines);
    row.set_margin_end(CARD_MARGIN / 2);
    if let Some(note_id) = occurrence.note_id {
        let icon = text::symbol("sticky_note_2", pixel_size::LARGER as f64);
        text::set_color(&icon, "m3outline");
        icon.set_valign(gtk4::Align::Center);
        icon.set_margin_end(CARD_PADDING);
        row.append(&icon);
        pointer_cursor(row.upcast_ref());
        let click = gtk4::GestureClick::new();
        click.connect_released(move |_, _, _, _| {
            if !open_note(note_id) {
                details.set_text(&tr("This note no longer exists"));
            }
        });
        row.add_controller(click);
    }
    row.upcast()
}

fn format_details(occurrence: &Occurrence, time_pattern: &str) -> String {
    let when = if occurrence.is_all_day {
        tr("All day")
    } else {
        let clock = |wall: i64| {
            glib::DateTime::from_unix_utc(wall)
                .and_then(|date| date.format(time_pattern))
                .map(String::from)
                .unwrap_or_default()
        };
        if occurrence.end > occurrence.start {
            format!("{} – {}", clock(occurrence.start), clock(occurrence.end))
        } else {
            clock(occurrence.start)
        }
    };
    let location = occurrence.location.lines().next().unwrap_or_default();
    if location.is_empty() {
        when
    } else {
        format!("{when} · {location}")
    }
}

pub fn parse_color(text: &str) -> Option<RGBA> {
    RGBA::parse(text).ok()
}

fn fill_rounded(snapshot: &gtk4::Snapshot, color: &RGBA, x: f32, y: f32, width: f32, height: f32) {
    let bounds = graphene::Rect::new(x, y, width, height);
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(
        bounds,
        width.min(height) / 2.0,
    ));
    snapshot.append_color(color, &bounds);
    snapshot.pop();
}

pub struct Dots {
    pub widget: Paint,
    colors: Rc<RefCell<Vec<Option<RGBA>>>>,
}

impl Dots {
    pub fn new(theme: &SharedTheme) -> Dots {
        let colors: Rc<RefCell<Vec<Option<RGBA>>>> = Rc::default();
        let theme = theme.clone();
        let widget = Paint::new({
            let colors = colors.clone();
            move |snapshot, width, height| {
                let colors = colors.borrow();
                let span = colors.len() as f32 * (DOT_SIZE + DOT_GAP) - DOT_GAP;
                let first_x = (width - span) / 2.0;
                for (index, color) in colors.iter().enumerate() {
                    let color = color.unwrap_or_else(|| theme.borrow().colors.col_primary);
                    let x = first_x + index as f32 * (DOT_SIZE + DOT_GAP);
                    fill_rounded(snapshot, &color, x, height - DOT_SIZE, DOT_SIZE, DOT_SIZE);
                }
            }
        });
        let width = MAX_DOTS as f32 * (DOT_SIZE + DOT_GAP) - DOT_GAP;
        widget.set_size_request(width as i32, DOT_SIZE as i32);
        widget.set_halign(gtk4::Align::Center);
        widget.set_valign(gtk4::Align::End);
        widget.set_can_target(false);
        Dots { widget, colors }
    }

    pub fn set(&self, occurrences: &[Occurrence]) {
        let mut colors: Vec<Option<RGBA>> = Vec::new();
        let mut seen_colors: Vec<&Option<String>> = Vec::new();
        for occurrence in occurrences {
            if seen_colors.contains(&&occurrence.color) || seen_colors.len() == MAX_DOTS {
                continue;
            }
            seen_colors.push(&occurrence.color);
            colors.push(occurrence.color.as_deref().and_then(parse_color));
        }
        if *self.colors.borrow() == colors {
            return;
        }
        self.colors.replace(colors);
        self.widget.queue_draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_times_leave_out_the_seconds_of_the_shell_time_format() {
        let time_pattern = strip_seconds(&strftime_from_qt("HH:mm:ss"));
        let occurrence = Occurrence {
            start: 13 * 3600,
            end: 14 * 3600 + 30 * 60,
            is_all_day: false,
            summary: String::new(),
            location: "Room 4\nFloor 2".into(),
            note_id: None,
            color: None,
        };
        assert_eq!(
            format_details(&occurrence, &time_pattern),
            "13:00 – 14:30 · Room 4"
        );
    }
}

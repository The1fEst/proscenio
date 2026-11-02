use super::civil::{SECONDS_PER_DAY, format_date, format_date_time};
use super::event::NOTE_URL_PREFIX;
use super::time::Zones;
use crate::core::icalendar::{escape_text, fold_line};

const EXCERPT_CHARS: usize = 500;

pub enum Slot {
    AllDay,
    Interval {
        start_minute: i64,
        length_minutes: i64,
    },
}

pub struct Draft {
    pub note_id: u64,
    pub title: String,
    pub excerpt: String,
    pub day: i64,
    pub slot: Slot,
}

pub fn make_excerpt(text: &str) -> String {
    text.trim().chars().take(EXCERPT_CHARS).collect()
}

pub fn render(draft: &Draft, zones: &Zones, uid: &str, now_unix: i64) -> String {
    let mut lines = vec![
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{uid}"),
        format!("DTSTAMP:{}Z", format_date_time(now_unix)),
    ];
    lines.extend(render_time_lines(draft, zones));
    lines.push(format!("SUMMARY:{}", escape_text(&draft.title)));
    if !draft.excerpt.is_empty() {
        lines.push(format!("DESCRIPTION:{}", escape_text(&draft.excerpt)));
    }
    lines.push(format!("URL:{NOTE_URL_PREFIX}{}", draft.note_id));
    lines.push(format!("X-PROSCENIO-NOTE:{}", draft.note_id));
    lines.push("END:VEVENT".to_owned());
    lines
        .iter()
        .map(|line| format!("{}\r\n", fold_line(line)))
        .collect()
}

fn render_time_lines(draft: &Draft, zones: &Zones) -> [String; 2] {
    let (start_minute, length_minutes) = match draft.slot {
        Slot::AllDay => {
            return [
                format!("DTSTART;VALUE=DATE:{}", format_date(draft.day)),
                format!("DTEND;VALUE=DATE:{}", format_date(draft.day + 1)),
            ];
        }
        Slot::Interval {
            start_minute,
            length_minutes,
        } => (start_minute, length_minutes),
    };
    let start_wall = draft.day * SECONDS_PER_DAY + start_minute * 60;
    let start_unix = zones.local_to_unix(start_wall);
    let end_unix = start_unix + length_minutes * 60;
    let zone = zones.local_identifier();
    if zone.is_empty() || zone.starts_with(['/', '+', '-']) {
        return [
            format!("DTSTART:{}Z", format_date_time(start_unix)),
            format!("DTEND:{}Z", format_date_time(end_unix)),
        ];
    }
    [
        format!("DTSTART;TZID={zone}:{}", format_date_time(start_wall)),
        format!(
            "DTEND;TZID={zone}:{}",
            format_date_time(zones.unix_to_local(end_unix))
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::civil::days_from_civil;
    use super::super::event::parse_events;
    use super::*;
    use crate::core::icalendar;
    use gtk4::glib;

    fn draft(slot: Slot, day: i64) -> Draft {
        Draft {
            note_id: 42,
            title: "Plan, A; B".into(),
            excerpt: "First line\nSecond \\ line".into(),
            day,
            slot,
        }
    }

    fn zones(name: &str) -> Zones {
        Zones::new(glib::TimeZone::new(Some(name)))
    }

    fn lines(text: &str) -> Vec<&str> {
        text.split("\r\n").filter(|line| !line.is_empty()).collect()
    }

    #[test]
    fn an_all_day_event_ends_on_the_next_date_and_names_its_note() {
        let day = days_from_civil(2026, 12, 31);
        let text = render(
            &draft(Slot::AllDay, day),
            &zones("Europe/Berlin"),
            "uid-1",
            1_793_491_200,
        );
        assert_eq!(
            lines(&text),
            [
                "BEGIN:VEVENT",
                "UID:uid-1",
                "DTSTAMP:20261101T000000Z",
                "DTSTART;VALUE=DATE:20261231",
                "DTEND;VALUE=DATE:20270101",
                "SUMMARY:Plan\\, A\\; B",
                "DESCRIPTION:First line\\nSecond \\\\ line",
                "URL:proscenio://note/42",
                "X-PROSCENIO-NOTE:42",
                "END:VEVENT",
            ]
        );
    }

    #[test]
    fn a_timed_event_uses_the_local_zone_and_ends_after_its_real_length() {
        let interval_slot = |start_minute, length_minutes| Slot::Interval {
            start_minute,
            length_minutes,
        };
        let berlin = zones("Europe/Berlin");
        let times = |slot, day| {
            let text = render(&draft(slot, day), &berlin, "u", 0);
            let lines = lines(&text);
            (lines[3].to_owned(), lines[4].to_owned())
        };
        assert_eq!(
            times(
                interval_slot(23 * 60 + 45, 30),
                days_from_civil(2026, 11, 4)
            ),
            (
                "DTSTART;TZID=Europe/Berlin:20261104T234500".to_owned(),
                "DTEND;TZID=Europe/Berlin:20261105T001500".to_owned()
            )
        );
        assert_eq!(
            times(interval_slot(90, 120), days_from_civil(2026, 10, 25)),
            (
                "DTSTART;TZID=Europe/Berlin:20261025T013000".to_owned(),
                "DTEND;TZID=Europe/Berlin:20261025T023000".to_owned()
            )
        );
    }

    #[test]
    fn a_zone_without_a_name_gives_utc_times() {
        let text = render(
            &draft(
                Slot::Interval {
                    start_minute: 600,
                    length_minutes: 30,
                },
                days_from_civil(2026, 11, 4),
            ),
            &zones("+05:00"),
            "u",
            0,
        );
        let lines = lines(&text);
        assert_eq!(lines[3], "DTSTART:20261104T050000Z");
        assert_eq!(lines[4], "DTEND:20261104T053000Z");
    }

    #[test]
    fn long_lines_fold_and_the_event_parses_back_with_its_note() {
        let mut long_draft = draft(Slot::AllDay, days_from_civil(2026, 11, 4));
        long_draft.excerpt = "Ж, текст; ".repeat(80);
        let text = render(&long_draft, &zones("UTC"), "u", 0);
        assert!(text.split("\r\n").all(|line| line.len() <= 75));
        let roots = icalendar::parse(&text);
        let component = &roots[0];
        assert_eq!(component.name, "VEVENT");
        assert_eq!(
            component.property("DESCRIPTION").unwrap().text(),
            long_draft.excerpt
        );
        let events = parse_events(&text);
        assert_eq!(events[0].summary, "Plan, A; B");
        assert_eq!(events[0].note_id, Some(42));
    }

    #[test]
    fn an_excerpt_is_trimmed_and_cut_to_the_limit() {
        assert_eq!(make_excerpt("  hello\n"), "hello");
        assert_eq!(
            make_excerpt(&"я".repeat(900)).chars().count(),
            EXCERPT_CHARS
        );
    }
}

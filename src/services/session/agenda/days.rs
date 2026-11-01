use std::collections::HashMap;

use super::civil::SECONDS_PER_DAY;
use super::event::Event;
use super::time::Zones;

const ZONE_SLACK_SECONDS: i64 = 2 * SECONDS_PER_DAY;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub first_day: i64,
    pub end_day: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub start: i64,
    pub end: i64,
    pub is_all_day: bool,
    pub summary: String,
    pub location: String,
    pub color: Option<String>,
}

pub fn expand_events(
    events: &[Event],
    window: Window,
    zones: &Zones,
    color: Option<&str>,
) -> Vec<Occurrence> {
    let from = window.first_day * SECONDS_PER_DAY;
    let to = window.end_day * SECONDS_PER_DAY;
    let mut replaced_starts: HashMap<&str, Vec<i64>> = HashMap::new();
    for event in events {
        if let Some(original) = &event.recurrence_id {
            replaced_starts
                .entry(&event.uid)
                .or_default()
                .push(zones.to_local(original));
        }
    }
    let mut occurrences = Vec::new();
    for event in events {
        let first = zones.to_local(&event.start);
        let is_all_day = event.start.is_date();
        let length = match (&event.end, event.duration) {
            (Some(end), _) => (zones.to_local(end) - first).max(0),
            (None, Some(duration)) => duration.max(0),
            (None, None) if is_all_day => SECONDS_PER_DAY,
            (None, None) => 0,
        };
        let length = if is_all_day && length == 0 {
            SECONDS_PER_DAY
        } else {
            length
        };
        let mut starts = vec![first];
        if event.recurrence_id.is_none() {
            if let Some(rule) = &event.rule {
                let excluded_starts: Vec<i64> = event
                    .exceptions
                    .iter()
                    .map(|time| zones.to_local(time))
                    .chain(
                        replaced_starts
                            .get(event.uid.as_str())
                            .into_iter()
                            .flatten()
                            .copied(),
                    )
                    .collect();
                let latest_start = rule.until.as_ref().map(|until| {
                    let local = zones.to_local(until);
                    if until.is_date() {
                        local + SECONDS_PER_DAY - 1
                    } else {
                        local
                    }
                });
                starts = rule
                    .expand(
                        event.start.wall,
                        from - length - ZONE_SLACK_SECONDS,
                        to + ZONE_SLACK_SECONDS,
                    )
                    .into_iter()
                    .map(|wall| zones.to_local(&event.start.with_wall(wall)))
                    .filter(|start| {
                        !excluded_starts.contains(start)
                            && latest_start.is_none_or(|latest| *start <= latest)
                    })
                    .collect();
            }
            starts.extend(event.extra_starts.iter().map(|time| zones.to_local(time)));
        }
        for start in starts {
            let end = start + length;
            if start < to && end.max(start + 1) > from {
                occurrences.push(Occurrence {
                    start,
                    end,
                    is_all_day,
                    summary: event.summary.clone(),
                    location: event.location.clone(),
                    color: color.map(str::to_owned),
                });
            }
        }
    }
    occurrences
}

pub fn bucket(occurrences: &[Occurrence], window: Window) -> HashMap<i64, Vec<Occurrence>> {
    let mut days: HashMap<i64, Vec<Occurrence>> = HashMap::new();
    for occurrence in occurrences {
        let first_day = occurrence.start.div_euclid(SECONDS_PER_DAY);
        let last_day = if occurrence.end > occurrence.start {
            (occurrence.end - 1).div_euclid(SECONDS_PER_DAY)
        } else {
            first_day
        };
        for day in first_day.max(window.first_day)..=last_day.min(window.end_day - 1) {
            let day_start = day * SECONDS_PER_DAY;
            let day_end = day_start + SECONDS_PER_DAY;
            days.entry(day).or_default().push(Occurrence {
                start: occurrence.start.max(day_start),
                end: occurrence.end.min(day_end),
                is_all_day: occurrence.is_all_day
                    || (occurrence.start <= day_start && occurrence.end >= day_end),
                ..occurrence.clone()
            });
        }
    }
    for list in days.values_mut() {
        list.sort_by(|first, second| {
            (!first.is_all_day, first.start, &first.summary).cmp(&(
                !second.is_all_day,
                second.start,
                &second.summary,
            ))
        });
    }
    days
}

#[cfg(test)]
mod tests {
    use super::super::civil::days_from_civil;
    use super::super::event::parse_events;
    use super::*;
    use gtk4::glib;

    fn to_days(year: i64, month: i64, date: i64) -> i64 {
        days_from_civil(year, month, date)
    }

    fn to_wall(year: i64, month: i64, date: i64, hour: i64, minute: i64) -> i64 {
        to_days(year, month, date) * SECONDS_PER_DAY + hour * 3600 + minute * 60
    }

    fn make_berlin_zones() -> Zones {
        Zones::new(glib::TimeZone::new(Some("Europe/Berlin")))
    }

    fn make_window() -> Window {
        Window {
            first_day: to_days(2026, 11, 1),
            end_day: to_days(2026, 12, 13),
        }
    }

    fn list_summaries(days: &HashMap<i64, Vec<Occurrence>>, date: i64) -> Vec<&str> {
        days.get(&date)
            .map(|list| list.iter().map(|item| item.summary.as_str()).collect())
            .unwrap_or_default()
    }

    fn bucket_text(text: &str) -> HashMap<i64, Vec<Occurrence>> {
        let occurrences = expand_events(
            &parse_events(text),
            make_window(),
            &make_berlin_zones(),
            Some("#62a0ea"),
        );
        bucket(&occurrences, make_window())
    }

    #[test]
    fn a_utc_event_lands_on_the_local_day_it_starts_in() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Late call\nDTSTART:20261104T233000Z\nDTEND:20261105T003000Z\nEND:VEVENT",
        );
        assert!(list_summaries(&days, to_days(2026, 11, 4)).is_empty());
        let entries = &days[&to_days(2026, 11, 5)];
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].start, to_wall(2026, 11, 5, 0, 30));
        assert_eq!(entries[0].end, to_wall(2026, 11, 5, 1, 30));
        assert_eq!(entries[0].color.as_deref(), Some("#62a0ea"));
    }

    #[test]
    fn an_event_across_midnight_appears_on_both_days_clipped_to_each() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Night\nDTSTART:20261104T223000\nDTEND:20261105T013000\nEND:VEVENT",
        );
        let first = &days[&to_days(2026, 11, 4)][0];
        assert_eq!(
            (first.start, first.end),
            (to_wall(2026, 11, 4, 22, 30), to_wall(2026, 11, 5, 0, 0))
        );
        let second = &days[&to_days(2026, 11, 5)][0];
        assert_eq!(
            (second.start, second.end),
            (to_wall(2026, 11, 5, 0, 0), to_wall(2026, 11, 5, 1, 30))
        );
        assert!(!first.is_all_day && !second.is_all_day);
    }

    #[test]
    fn an_event_that_ends_at_midnight_stays_on_its_day() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Evening\nDTSTART:20261104T200000\nDTEND:20261105T000000\nEND:VEVENT",
        );
        assert_eq!(list_summaries(&days, to_days(2026, 11, 4)), ["Evening"]);
        assert!(list_summaries(&days, to_days(2026, 11, 5)).is_empty());
    }

    #[test]
    fn a_multi_day_all_day_event_covers_its_dates_and_ends_before_the_end_date() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Trip\nDTSTART;VALUE=DATE:20261104\nDTEND;VALUE=DATE:20261107\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:b\nSUMMARY:Holiday\nDTSTART;VALUE=DATE:20261105\nEND:VEVENT",
        );
        for date in 4..=6 {
            assert!(
                list_summaries(&days, to_days(2026, 11, date)).contains(&"Trip"),
                "{date}"
            );
        }
        assert!(list_summaries(&days, to_days(2026, 11, 7)).is_empty());
        assert!(
            days[&to_days(2026, 11, 5)]
                .iter()
                .all(|entry| entry.is_all_day)
        );
        assert_eq!(days[&to_days(2026, 11, 5)].len(), 2);
    }

    #[test]
    fn a_timed_event_spanning_whole_days_is_all_day_on_the_middle_ones() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Conference\nDTSTART:20261104T150000\nDTEND:20261107T100000\nEND:VEVENT",
        );
        assert!(!days[&to_days(2026, 11, 4)][0].is_all_day);
        assert!(days[&to_days(2026, 11, 5)][0].is_all_day);
        assert!(days[&to_days(2026, 11, 6)][0].is_all_day);
        assert!(!days[&to_days(2026, 11, 7)][0].is_all_day);
    }

    #[test]
    fn all_day_entries_come_first_then_timed_ones_by_start() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Late\nDTSTART:20261104T180000\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:b\nSUMMARY:Early\nDTSTART:20261104T080000\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:c\nSUMMARY:Birthday\nDTSTART;VALUE=DATE:20261104\nEND:VEVENT",
        );
        assert_eq!(
            list_summaries(&days, to_days(2026, 11, 4)),
            ["Birthday", "Early", "Late"]
        );
    }

    #[test]
    fn a_weekly_series_skips_excluded_dates_and_uses_the_zone_offset_of_each_date() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Standup\nDTSTART;TZID=America/New_York:20261029T193000\n\
             DTEND;TZID=America/New_York:20261029T200000\nRRULE:FREQ=WEEKLY;BYDAY=TH\n\
             EXDATE;TZID=America/New_York:20261119T193000\nEND:VEVENT",
        );
        let mut day_offsets: Vec<(i64, i64)> = days
            .iter()
            .map(|(date, list)| (*date, list[0].start - date * SECONDS_PER_DAY))
            .collect();
        day_offsets.sort();
        let after_midnight = 3600 + 30 * 60;
        assert_eq!(
            day_offsets,
            [
                (to_days(2026, 11, 6), after_midnight),
                (to_days(2026, 11, 13), after_midnight),
                (to_days(2026, 11, 27), after_midnight),
                (to_days(2026, 12, 4), after_midnight),
                (to_days(2026, 12, 11), after_midnight),
            ]
        );
    }

    #[test]
    fn a_replaced_instance_shows_at_its_new_time_and_not_at_the_original_one() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Review\nDTSTART:20261105T100000Z\nDTEND:20261105T110000Z\nRRULE:FREQ=DAILY;COUNT=3\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:a\nSUMMARY:Review moved\nRECURRENCE-ID:20261106T100000Z\nDTSTART:20261106T150000Z\nDTEND:20261106T160000Z\nEND:VEVENT",
        );
        assert_eq!(list_summaries(&days, to_days(2026, 11, 5)), ["Review"]);
        assert_eq!(
            list_summaries(&days, to_days(2026, 11, 6)),
            ["Review moved"]
        );
        assert_eq!(list_summaries(&days, to_days(2026, 11, 7)), ["Review"]);
        assert_eq!(
            days[&to_days(2026, 11, 6)][0].start,
            to_wall(2026, 11, 6, 16, 0)
        );
    }

    #[test]
    fn a_series_ends_at_its_until_date_inclusive() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Daily\nDTSTART;VALUE=DATE:20261104\nRRULE:FREQ=DAILY;UNTIL=20261106\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:b\nSUMMARY:Timed\nDTSTART:20261104T090000Z\nRRULE:FREQ=DAILY;UNTIL=20261105T085959Z\nEND:VEVENT",
        );
        assert_eq!(list_summaries(&days, to_days(2026, 11, 6)), ["Daily"]);
        assert!(list_summaries(&days, to_days(2026, 11, 7)).is_empty());
        assert_eq!(
            list_summaries(&days, to_days(2026, 11, 4)),
            ["Daily", "Timed"]
        );
        assert_eq!(list_summaries(&days, to_days(2026, 11, 5)), ["Daily"]);
    }

    #[test]
    fn a_yearly_birthday_shows_on_its_date_in_the_window_only() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Ada's birthday\nDTSTART;VALUE=DATE:19901110\nRRULE:FREQ=YEARLY\nEND:VEVENT",
        );
        assert_eq!(days.len(), 1);
        assert_eq!(
            list_summaries(&days, to_days(2026, 11, 10)),
            ["Ada's birthday"]
        );
    }

    #[test]
    fn events_outside_the_window_and_series_starting_after_it_are_left_out() {
        let days = bucket_text(
            "BEGIN:VEVENT\nUID:a\nSUMMARY:Old\nDTSTART:20261001T100000Z\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:b\nSUMMARY:Future\nDTSTART:20270101T100000Z\nRRULE:FREQ=DAILY\nEND:VEVENT",
        );
        assert!(days.is_empty());
    }
}

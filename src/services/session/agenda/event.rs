use super::recurrence::Rule;
use super::time::{Time, parse_duration};
use crate::core::icalendar::{self, Component, Property};

pub struct Event {
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub start: Time,
    pub end: Option<Time>,
    pub duration: Option<i64>,
    pub rule: Option<Rule>,
    pub exceptions: Vec<Time>,
    pub extra_starts: Vec<Time>,
    pub recurrence_id: Option<Time>,
}

impl Event {
    fn from_component(component: &Component) -> Option<Event> {
        let read_text = |name: &str| {
            component
                .property(name)
                .map(Property::text)
                .unwrap_or_default()
        };
        if read_text("STATUS").eq_ignore_ascii_case("CANCELLED") {
            return None;
        }
        let read_times = |name: &str| -> Vec<Time> {
            component
                .properties_named(name)
                .flat_map(|property| {
                    property.value.split(',').filter_map(|piece| {
                        Time::parse(&Property {
                            value: piece.to_owned(),
                            parameters: property.parameters.clone(),
                            ..Property::default()
                        })
                    })
                })
                .collect()
        };
        Some(Event {
            uid: read_text("UID"),
            summary: read_text("SUMMARY"),
            location: read_text("LOCATION"),
            start: Time::parse(component.property("DTSTART")?)?,
            end: component.property("DTEND").and_then(Time::parse),
            duration: component
                .property("DURATION")
                .and_then(|property| parse_duration(&property.value)),
            rule: component
                .property("RRULE")
                .map(|property| Rule::parse(&property.value)),
            exceptions: read_times("EXDATE"),
            extra_starts: read_times("RDATE"),
            recurrence_id: component.property("RECURRENCE-ID").and_then(Time::parse),
        })
    }
}

pub fn parse_events(text: &str) -> Vec<Event> {
    icalendar::parse(text)
        .iter()
        .flat_map(|root| root.find_all("VEVENT"))
        .filter_map(Event::from_component)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::time::Frame;
    use super::*;

    #[test]
    fn a_calendar_object_gives_its_events_with_unescaped_text() {
        let events = parse_events(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:one@example\r\n\
             SUMMARY:Sync\\, weekly\r\nLOCATION:Room 4\\nFloor 2\r\n\
             DTSTART;TZID=Europe/Berlin:20261104T100000\r\n\
             DTEND;TZID=Europe/Berlin:20261104T110000\r\n\
             RRULE:FREQ=WEEKLY;BYDAY=WE\r\n\
             EXDATE;TZID=Europe/Berlin:20261111T100000,20261118T100000\r\n\
             EXDATE;TZID=Europe/Berlin:20261125T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        );
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.uid, "one@example");
        assert_eq!(event.summary, "Sync, weekly");
        assert_eq!(event.location, "Room 4\nFloor 2");
        assert_eq!(event.start.frame, Frame::Zone("Europe/Berlin".into()));
        assert_eq!(event.end.as_ref().unwrap().wall - event.start.wall, 3600);
        assert!(event.rule.is_some());
        assert_eq!(event.exceptions.len(), 3);
    }

    #[test]
    fn bare_events_parse_and_cancelled_or_undated_ones_are_dropped() {
        let events = parse_events(
            "BEGIN:VEVENT\nUID:a\nDTSTART;VALUE=DATE:20261104\nDTEND;VALUE=DATE:20261106\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:b\nSTATUS:CANCELLED\nDTSTART:20261104T100000Z\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:c\nSUMMARY:No start\nEND:VEVENT\n\
             BEGIN:VEVENT\nUID:d\nDTSTART:20261104T100000Z\nDURATION:PT45M\nRECURRENCE-ID:20261111T100000Z\nEND:VEVENT\n",
        );
        let uids: Vec<&str> = events.iter().map(|event| event.uid.as_str()).collect();
        assert_eq!(uids, ["a", "d"]);
        assert!(events[0].start.is_date());
        assert_eq!(events[1].duration, Some(2700));
        assert!(events[1].recurrence_id.is_some());
    }
}

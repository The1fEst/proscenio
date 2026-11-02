use gtk4::glib;
use std::cell::RefCell;
use std::collections::HashMap;

use super::civil::{SECONDS_PER_DAY, civil_from_days, days_from_civil};
use crate::core::icalendar::Property;

#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    Date,
    Local,
    Utc,
    Zone(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Time {
    pub frame: Frame,
    pub wall: i64,
}

impl Time {
    pub fn parse(property: &Property) -> Option<Time> {
        let value = property.value.trim();
        let is_date = property.parameter("VALUE") == Some("DATE") || value.len() == 8;
        let (date, clock) = match value.split_once(['T', 't']) {
            Some((date, clock)) => (date, Some(clock)),
            None => (value, None),
        };
        let read_date_number = |range: std::ops::Range<usize>| date.get(range)?.parse::<i64>().ok();
        if date.len() != 8 {
            return None;
        }
        let days = days_from_civil(
            read_date_number(0..4)?,
            read_date_number(4..6)?,
            read_date_number(6..8)?,
        );
        if is_date {
            return Some(Time {
                frame: Frame::Date,
                wall: days * SECONDS_PER_DAY,
            });
        }
        let clock = clock?;
        let is_utc = clock.ends_with(['Z', 'z']);
        let clock = clock.trim_end_matches(['Z', 'z']);
        let read_clock_number =
            |range: std::ops::Range<usize>| clock.get(range)?.parse::<i64>().ok();
        let seconds = read_clock_number(0..2)? * 3600
            + read_clock_number(2..4)? * 60
            + read_clock_number(4..6).unwrap_or(0);
        let frame = match (is_utc, property.parameter("TZID")) {
            (true, _) => Frame::Utc,
            (false, Some(zone)) => Frame::Zone(zone.to_owned()),
            (false, None) => Frame::Local,
        };
        Some(Time {
            frame,
            wall: days * SECONDS_PER_DAY + seconds,
        })
    }

    pub fn is_date(&self) -> bool {
        self.frame == Frame::Date
    }

    pub fn with_wall(&self, wall: i64) -> Time {
        Time {
            frame: self.frame.clone(),
            wall,
        }
    }
}

pub fn parse_duration(value: &str) -> Option<i64> {
    let (sign, rest) = match value.trim().strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, value.trim().trim_start_matches('+')),
    };
    let rest = rest.strip_prefix('P')?;
    let mut seconds = 0;
    let mut number = String::new();
    let mut is_clock = false;
    for character in rest.chars() {
        let unit = match character {
            'T' => {
                is_clock = true;
                continue;
            }
            '0'..='9' => {
                number.push(character);
                continue;
            }
            'W' => 7 * SECONDS_PER_DAY,
            'D' => SECONDS_PER_DAY,
            'H' if is_clock => 3600,
            'M' if is_clock => 60,
            'S' if is_clock => 1,
            _ => return None,
        };
        seconds += number.parse::<i64>().ok()? * unit;
        number.clear();
    }
    number.is_empty().then_some(sign * seconds)
}

pub struct Zones {
    local: glib::TimeZone,
    cache: RefCell<HashMap<String, Option<glib::TimeZone>>>,
}

impl Zones {
    pub fn new(local: glib::TimeZone) -> Self {
        Zones {
            local,
            cache: RefCell::default(),
        }
    }

    pub fn local() -> Self {
        Zones::new(glib::TimeZone::local())
    }

    pub fn local_identifier(&self) -> String {
        self.local.identifier().to_string()
    }

    pub fn to_local(&self, time: &Time) -> i64 {
        match &time.frame {
            Frame::Date | Frame::Local => time.wall,
            Frame::Utc => self.unix_to_local(time.wall),
            Frame::Zone(name) => match self.lookup_zone(name) {
                Some(zone) => self.unix_to_local(wall_to_unix(&zone, time.wall)),
                None => time.wall,
            },
        }
    }

    pub fn local_to_unix(&self, wall: i64) -> i64 {
        wall_to_unix(&self.local, wall)
    }

    pub fn unix_to_local(&self, unix: i64) -> i64 {
        let interval = self.local.find_interval(glib::TimeType::Universal, unix);
        unix + i64::from(self.local.offset(interval))
    }

    fn lookup_zone(&self, name: &str) -> Option<glib::TimeZone> {
        self.cache
            .borrow_mut()
            .entry(name.to_owned())
            .or_insert_with(|| find_zone(name))
            .clone()
    }
}

fn wall_to_unix(zone: &glib::TimeZone, wall: i64) -> i64 {
    let days = wall.div_euclid(SECONDS_PER_DAY);
    let seconds = wall.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    glib::DateTime::new(
        zone,
        year as i32,
        month as i32,
        day as i32,
        (seconds / 3600) as i32,
        (seconds / 60 % 60) as i32,
        (seconds % 60) as f64,
    )
    .map_or(wall, |date| date.to_unix())
}

fn find_zone(name: &str) -> Option<glib::TimeZone> {
    let segments: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
    (1..=segments.len().min(3)).rev().find_map(|count| {
        let candidate = segments[segments.len() - count..].join("/");
        let zone = glib::TimeZone::new(Some(&candidate));
        (zone.identifier() == candidate).then_some(zone)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::icalendar::parse;

    fn parse_time(line: &str) -> Time {
        let component = &parse(&format!("BEGIN:X\n{line}\nEND:X"))[0];
        Time::parse(&component.properties[0]).unwrap()
    }

    #[test]
    fn dates_local_utc_and_zone_values_parse_into_their_frames() {
        let day = days_from_civil(2026, 11, 4) * SECONDS_PER_DAY;
        assert_eq!(
            parse_time("DTSTART;VALUE=DATE:20261104"),
            Time {
                frame: Frame::Date,
                wall: day
            }
        );
        assert_eq!(parse_time("DTSTART:20261104").frame, Frame::Date);
        assert_eq!(
            parse_time("DTSTART:20261104T093015"),
            Time {
                frame: Frame::Local,
                wall: day + 9 * 3600 + 30 * 60 + 15
            }
        );
        assert_eq!(
            parse_time("DTSTART:20261104T093000Z"),
            Time {
                frame: Frame::Utc,
                wall: day + 9 * 3600 + 30 * 60
            }
        );
        assert_eq!(
            parse_time("DTSTART;TZID=Europe/Berlin:20261104T100000"),
            Time {
                frame: Frame::Zone("Europe/Berlin".into()),
                wall: day + 10 * 3600
            }
        );
    }

    #[test]
    fn malformed_values_are_rejected() {
        let component = &parse("BEGIN:X\nDTSTART:soon\nDTEND:2026110\nEND:X")[0];
        assert!(
            component
                .properties
                .iter()
                .all(|property| Time::parse(property).is_none())
        );
    }

    #[test]
    fn durations_add_weeks_days_and_clock_units() {
        assert_eq!(parse_duration("PT1H30M"), Some(5400));
        assert_eq!(parse_duration("P1DT2H"), Some(93_600));
        assert_eq!(parse_duration("P2W"), Some(1_209_600));
        assert_eq!(parse_duration("-PT15M"), Some(-900));
        assert_eq!(parse_duration("1H"), None);
        assert_eq!(parse_duration("PT5X"), None);
        assert_eq!(parse_duration("PT1H30"), None);
        assert_eq!(parse_duration("P1W2"), None);
    }

    #[test]
    fn times_convert_to_the_local_zone_with_its_offset_at_that_date() {
        let zones = Zones::new(glib::TimeZone::new(Some("Asia/Yekaterinburg")));
        let to_wall = |year: i64, month: i64, day: i64, hour: i64| {
            (days_from_civil(year, month, day) * SECONDS_PER_DAY) + hour * 3600
        };
        let utc_time = Time {
            frame: Frame::Utc,
            wall: to_wall(2026, 11, 4, 22),
        };
        assert_eq!(zones.to_local(&utc_time), to_wall(2026, 11, 5, 3));
        let berlin_winter = Time {
            frame: Frame::Zone("Europe/Berlin".into()),
            wall: to_wall(2026, 11, 4, 10),
        };
        assert_eq!(zones.to_local(&berlin_winter), to_wall(2026, 11, 4, 14));
        let berlin_summer = Time {
            frame: Frame::Zone("Europe/Berlin".into()),
            wall: to_wall(2026, 7, 4, 10),
        };
        assert_eq!(zones.to_local(&berlin_summer), to_wall(2026, 7, 4, 13));
        let prefixed_zone = Time {
            frame: Frame::Zone("/freeassociation.sourceforge.net/Tzfile/Europe/Berlin".into()),
            wall: to_wall(2026, 11, 4, 10),
        };
        assert_eq!(zones.to_local(&prefixed_zone), to_wall(2026, 11, 4, 14));
        let unknown_zone = Time {
            frame: Frame::Zone("W. Europe Standard Time".into()),
            wall: to_wall(2026, 11, 4, 10),
        };
        assert_eq!(zones.to_local(&unknown_zone), to_wall(2026, 11, 4, 10));
        let date = Time {
            frame: Frame::Date,
            wall: to_wall(2026, 11, 4, 0),
        };
        assert_eq!(zones.to_local(&date), to_wall(2026, 11, 4, 0));
    }
}

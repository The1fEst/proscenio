use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        Some(Date { year, month, day })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Moment {
    pub date: Date,
    pub minute_of_day: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Temperature {
    celsius: i32,
    fahrenheit: i32,
}

impl Temperature {
    fn from_parts(celsius: Option<i32>, fahrenheit: Option<i32>) -> Option<Self> {
        let celsius = celsius.or_else(|| fahrenheit.map(fahrenheit_to_celsius))?;
        let fahrenheit = fahrenheit.unwrap_or_else(|| celsius_to_fahrenheit(celsius));
        Some(Temperature {
            celsius,
            fahrenheit,
        })
    }

    pub fn degrees(&self, uses_uscs: bool) -> i32 {
        if uses_uscs {
            self.fahrenheit
        } else {
            self.celsius
        }
    }
}

fn celsius_to_fahrenheit(celsius: i32) -> i32 {
    (celsius as f64 * 9.0 / 5.0 + 32.0).round() as i32
}

fn fahrenheit_to_celsius(fahrenheit: i32) -> i32 {
    ((fahrenheit as f64 - 32.0) * 5.0 / 9.0).round() as i32
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub start: Moment,
    pub code: String,
    pub temperature: Temperature,
    pub chance_of_rain_percent: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    pub date: Date,
    pub code: String,
    pub minimum: Temperature,
    pub maximum: Temperature,
    pub chance_of_rain_percent: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Forecast {
    pub days: Vec<Day>,
    pub slots: Vec<Slot>,
}

impl Forecast {
    pub fn select_upcoming_slots(&self, now: Moment, count: usize) -> Vec<&Slot> {
        self.slots
            .iter()
            .filter(|slot| slot.start >= now)
            .take(count)
            .collect()
    }
}

pub fn parse(response: &Value) -> Forecast {
    let mut forecast = Forecast::default();
    let Some(days) = response.get("weather").and_then(Value::as_array) else {
        return forecast;
    };
    for node in days {
        let Some(date) = node
            .get("date")
            .and_then(Value::as_str)
            .and_then(Date::parse)
        else {
            continue;
        };
        let slots: Vec<Slot> = node
            .get("hourly")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|hour| parse_slot(date, hour))
            .collect();
        forecast.days.extend(parse_day(date, node, &slots));
        forecast.slots.extend(slots);
    }
    forecast
}

fn parse_slot(date: Date, hour: &Value) -> Option<Slot> {
    let clock: u32 = read(hour, "time")?;
    let (hours, minutes) = (clock / 100, clock % 100);
    if hours >= 24 || minutes >= 60 {
        return None;
    }
    Some(Slot {
        start: Moment {
            date,
            minute_of_day: hours * 60 + minutes,
        },
        code: hour.get("weatherCode")?.as_str()?.to_owned(),
        temperature: Temperature::from_parts(read(hour, "tempC"), read(hour, "tempF"))?,
        chance_of_rain_percent: read(hour, "chanceofrain"),
    })
}

fn parse_day(date: Date, node: &Value, slots: &[Slot]) -> Option<Day> {
    let noon = slots
        .iter()
        .min_by_key(|slot| slot.start.minute_of_day.abs_diff(12 * 60))?;
    Some(Day {
        date,
        code: noon.code.clone(),
        minimum: Temperature::from_parts(read(node, "mintempC"), read(node, "mintempF"))?,
        maximum: Temperature::from_parts(read(node, "maxtempC"), read(node, "maxtempF"))?,
        chance_of_rain_percent: slots
            .iter()
            .filter_map(|slot| slot.chance_of_rain_percent)
            .max(),
    })
}

fn read<T: std::str::FromStr>(node: &Value, key: &str) -> Option<T> {
    node.get(key)?.as_str()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"weather": [
        {"date": "2026-10-31", "mintempC": "6", "mintempF": "43", "maxtempC": "13", "maxtempF": "55",
         "hourly": [
            {"time": "0", "tempC": "8", "tempF": "46", "weatherCode": "113", "chanceofrain": "0"},
            {"time": "1200", "tempC": "12", "tempF": "54", "weatherCode": "176", "chanceofrain": "14"},
            {"time": "2100", "tempC": "8", "tempF": "47", "weatherCode": "119", "chanceofrain": "40"}]},
        {"date": "2026-11-01", "mintempC": "-3", "maxtempC": "5",
         "hourly": [
            {"time": "300", "tempC": "-3", "weatherCode": "116", "chanceofrain": "5"},
            {"time": "1500", "tempC": "5", "weatherCode": "122"}]}
    ]}"#;

    fn build_sample() -> Forecast {
        parse(&serde_json::from_str(SAMPLE).unwrap())
    }

    fn make_moment(month: u32, day: u32, minute_of_day: u32) -> Moment {
        Moment {
            date: Date {
                year: 2026,
                month,
                day,
            },
            minute_of_day,
        }
    }

    fn collect_starts(slots: &[&Slot]) -> Vec<(u32, u32)> {
        slots
            .iter()
            .map(|slot| (slot.start.date.day, slot.start.minute_of_day))
            .collect()
    }

    #[test]
    fn days_and_slots_are_read_from_the_response() {
        let forecast = build_sample();
        assert_eq!(forecast.slots.len(), 5);
        assert_eq!(forecast.days.len(), 2);

        let today = &forecast.days[0];
        assert_eq!(
            today.date,
            Date {
                year: 2026,
                month: 10,
                day: 31
            }
        );
        assert_eq!(today.code, "176");
        assert_eq!(today.minimum.degrees(false), 6);
        assert_eq!(today.maximum.degrees(true), 55);
        assert_eq!(today.chance_of_rain_percent, Some(40));

        let noon = &forecast.slots[1];
        assert_eq!(noon.start, make_moment(10, 31, 720));
        assert_eq!(noon.temperature.degrees(false), 12);
        assert_eq!(noon.chance_of_rain_percent, Some(14));
    }

    #[test]
    fn a_missing_forecast_is_empty() {
        for response in [
            serde_json::json!({}),
            serde_json::json!({"weather": []}),
            serde_json::json!({"weather": "none"}),
            serde_json::json!({"weather": [{"date": "soon", "hourly": []}]}),
            serde_json::json!({"weather": [{"date": "2026-10-31", "mintempC": "1", "maxtempC": "2"}]}),
        ] {
            let forecast = parse(&response);
            assert!(forecast.days.is_empty(), "{response}");
            assert!(forecast.slots.is_empty(), "{response}");
        }
    }

    #[test]
    fn entries_with_missing_fields_are_skipped_or_left_blank() {
        let forecast = parse(&serde_json::json!({"weather": [
            {"date": "2026-10-31", "mintempC": "1", "maxtempC": "9", "hourly": [
                {"time": "600", "tempC": "4", "weatherCode": "113"},
                {"time": "900", "weatherCode": "113"},
                {"time": "1200", "tempC": "6"},
                {"time": "2500", "tempC": "6", "weatherCode": "113"},
                {"tempC": "6", "weatherCode": "113"}]},
            {"date": "2026-11-01", "mintempC": "1", "hourly": [
                {"time": "600", "tempC": "4", "weatherCode": "113"}]}]}));
        assert_eq!(forecast.slots.len(), 2);
        assert_eq!(forecast.slots[0].chance_of_rain_percent, None);
        assert_eq!(forecast.days.len(), 1);
        assert_eq!(forecast.days[0].chance_of_rain_percent, None);
    }

    #[test]
    fn the_next_slots_start_at_or_after_now() {
        let forecast = build_sample();
        assert_eq!(
            collect_starts(&forecast.select_upcoming_slots(make_moment(10, 31, 600), 2)),
            [(31, 720), (31, 1260)]
        );
        assert_eq!(
            collect_starts(&forecast.select_upcoming_slots(make_moment(10, 31, 721), 5)),
            [(31, 1260), (1, 180), (1, 900)]
        );
        assert!(
            forecast
                .select_upcoming_slots(make_moment(11, 1, 901), 5)
                .is_empty()
        );
    }

    #[test]
    fn the_next_slots_continue_past_midnight() {
        let forecast = build_sample();
        assert_eq!(
            collect_starts(&forecast.select_upcoming_slots(make_moment(10, 31, 1300), 2)),
            [(1, 180), (1, 900)]
        );
    }

    #[test]
    fn a_missing_unit_is_converted_from_the_other() {
        let forecast = build_sample();
        let night = &forecast.slots[2];
        assert_eq!(night.temperature.degrees(true), 47);

        let cold = &forecast.slots[3];
        assert_eq!(cold.temperature.degrees(false), -3);
        assert_eq!(cold.temperature.degrees(true), 27);

        let tomorrow = &forecast.days[1];
        assert_eq!(tomorrow.minimum.degrees(true), 27);
        assert_eq!(tomorrow.maximum.degrees(true), 41);

        let only_fahrenheit = Temperature::from_parts(None, Some(50)).unwrap();
        assert_eq!(only_fahrenheit.degrees(false), 10);
        assert!(Temperature::from_parts(None, None).is_none());
    }
}

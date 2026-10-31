// Formulas from the NOAA Global Monitoring Laboratory solar calculator.
const SUNRISE_ZENITH_DEGREES: f64 = 90.833;
const MINUTES_PER_DAY: f64 = 1440.0;
const J2000_JULIAN_DAY: f64 = 2_451_545.0;
const DAYS_PER_CENTURY: f64 = 36_525.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Daylight {
    Cycle {
        sunrise_minutes: i32,
        sunset_minutes: i32,
    },
    AlwaysDay,
    AlwaysNight,
}

pub fn compute_daylight(date: Date, utc_offset_minutes: i32, position: Position) -> Daylight {
    let julian_day = compute_julian_day(date);
    let refined_event = |is_sunrise: bool| {
        let first_pass_minutes = compute_event_minutes(julian_day, position, is_sunrise)?;
        compute_event_minutes(
            julian_day + first_pass_minutes / MINUTES_PER_DAY,
            position,
            is_sunrise,
        )
    };
    let local_minutes = |utc_minutes: f64| {
        ((utc_minutes + f64::from(utc_offset_minutes)).round() as i32)
            .rem_euclid(MINUTES_PER_DAY as i32)
    };
    match (refined_event(true), refined_event(false)) {
        (Ok(sunrise), Ok(sunset)) => Daylight::Cycle {
            sunrise_minutes: local_minutes(sunrise),
            sunset_minutes: local_minutes(sunset),
        },
        (Err(daylight), _) | (_, Err(daylight)) => daylight,
    }
}

fn compute_julian_day(date: Date) -> f64 {
    let (year, month) = if date.month <= 2 {
        (date.year - 1, date.month + 12)
    } else {
        (date.year, date.month)
    };
    let century = (f64::from(year) / 100.0).floor();
    let correction = 2.0 - century + (century / 4.0).floor();
    (365.25 * (f64::from(year) + 4716.0)).floor()
        + (30.6001 * (f64::from(month) + 1.0)).floor()
        + f64::from(date.day)
        + correction
        - 1524.5
}

fn compute_event_minutes(
    julian_day: f64,
    position: Position,
    is_sunrise: bool,
) -> Result<f64, Daylight> {
    let (equation_of_time_minutes, declination) =
        compute_solar_terms((julian_day - J2000_JULIAN_DAY) / DAYS_PER_CENTURY);
    let latitude = position.latitude_degrees.to_radians();
    let hour_angle_cosine = SUNRISE_ZENITH_DEGREES.to_radians().cos()
        / (latitude.cos() * declination.cos())
        - latitude.tan() * declination.tan();
    if hour_angle_cosine > 1.0 {
        return Err(Daylight::AlwaysNight);
    }
    if hour_angle_cosine < -1.0 {
        return Err(Daylight::AlwaysDay);
    }
    let hour_angle_minutes = 4.0 * hour_angle_cosine.acos().to_degrees();
    let noon_minutes = 720.0 - 4.0 * position.longitude_degrees - equation_of_time_minutes;
    Ok(if is_sunrise {
        noon_minutes - hour_angle_minutes
    } else {
        noon_minutes + hour_angle_minutes
    })
}

fn compute_solar_terms(century: f64) -> (f64, f64) {
    let mean_longitude_degrees =
        (280.46646 + century * (36000.76983 + century * 0.0003032)).rem_euclid(360.0);
    let mean_anomaly = (357.52911 + century * (35999.05029 - 0.0001537 * century)).to_radians();
    let eccentricity = 0.016708634 - century * (0.000042037 + 0.0000001267 * century);
    let center_degrees = mean_anomaly.sin()
        * (1.914602 - century * (0.004817 + 0.000014 * century))
        + (2.0 * mean_anomaly).sin() * (0.019993 - 0.000101 * century)
        + (3.0 * mean_anomaly).sin() * 0.000289;
    let node = (125.04 - 1934.136 * century).to_radians();
    let apparent_longitude =
        (mean_longitude_degrees + center_degrees - 0.00569 - 0.00478 * node.sin()).to_radians();
    let obliquity_seconds = 21.448 - century * (46.815 + century * (0.00059 - century * 0.001813));
    let mean_obliquity_degrees = 23.0 + (26.0 + obliquity_seconds / 60.0) / 60.0;
    let obliquity = (mean_obliquity_degrees + 0.00256 * node.cos()).to_radians();
    let declination = (obliquity.sin() * apparent_longitude.sin()).asin();

    let mean_longitude = mean_longitude_degrees.to_radians();
    let squared_half_tangent = (obliquity / 2.0).tan().powi(2);
    let equation_of_time_radians = squared_half_tangent * (2.0 * mean_longitude).sin()
        - 2.0 * eccentricity * mean_anomaly.sin()
        + 4.0
            * eccentricity
            * squared_half_tangent
            * mean_anomaly.sin()
            * (2.0 * mean_longitude).cos()
        - 0.5 * squared_half_tangent.powi(2) * (4.0 * mean_longitude).sin()
        - 1.25 * eccentricity.powi(2) * (2.0 * mean_anomaly).sin();
    (4.0 * equation_of_time_radians.to_degrees(), declination)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(hour: i32, minute: i32, second: i32) -> f64 {
        f64::from(hour * 60 + minute) + f64::from(second) / 60.0
    }

    #[test]
    fn the_julian_day_of_a_date_starts_at_midnight_universal_time() {
        let date = |year, month, day| compute_julian_day(Date { year, month, day });
        assert_eq!(date(2000, 1, 1), 2_451_544.5);
        assert_eq!(date(2026, 10, 31), 2_461_344.5);
        assert_eq!(date(2024, 2, 29), 2_460_369.5);
    }

    #[test]
    fn sunrise_and_sunset_match_published_values_within_two_minutes() {
        let cases = [
            (
                "Berlin, June solstice",
                (2026, 6, 21),
                (52.52, 13.405),
                120,
                (clock(4, 43, 4), clock(21, 33, 19)),
            ),
            (
                "Berlin, December solstice",
                (2026, 12, 21),
                (52.52, 13.405),
                60,
                (clock(8, 14, 46), clock(15, 54, 4)),
            ),
            (
                "Sydney, December solstice",
                (2026, 12, 21),
                (-33.8688, 151.2093),
                660,
                (clock(5, 40, 40), clock(20, 5, 22)),
            ),
            (
                "New York, March equinox",
                (2026, 3, 20),
                (40.7128, -74.006),
                -240,
                (clock(6, 58, 47), clock(19, 8, 2)),
            ),
            (
                "Tokyo, autumn",
                (2026, 10, 31),
                (35.6762, 139.6503),
                540,
                (clock(6, 2, 6), clock(16, 47, 56)),
            ),
        ];
        for (place, (year, month, day), (latitude, longitude), offset, (sunrise, sunset)) in cases {
            let position = Position {
                latitude_degrees: latitude,
                longitude_degrees: longitude,
            };
            let Daylight::Cycle {
                sunrise_minutes,
                sunset_minutes,
            } = compute_daylight(Date { year, month, day }, offset, position)
            else {
                panic!("{place}: the sun both rises and sets");
            };
            assert!(
                (f64::from(sunrise_minutes) - sunrise).abs() <= 2.0,
                "{place}: sunrise {sunrise_minutes} against {sunrise}"
            );
            assert!(
                (f64::from(sunset_minutes) - sunset).abs() <= 2.0,
                "{place}: sunset {sunset_minutes} against {sunset}"
            );
        }
    }

    #[test]
    fn near_the_pole_the_sun_stays_up_in_summer_and_down_in_winter() {
        let tromso = Position {
            latitude_degrees: 69.6492,
            longitude_degrees: 18.9553,
        };
        let daylight_on =
            |year, month, day| compute_daylight(Date { year, month, day }, 120, tromso);
        assert_eq!(daylight_on(2026, 6, 21), Daylight::AlwaysDay);
        assert_eq!(daylight_on(2026, 12, 21), Daylight::AlwaysNight);
        assert!(matches!(daylight_on(2026, 3, 20), Daylight::Cycle { .. }));
    }

    #[test]
    fn an_event_that_falls_on_the_other_side_of_local_midnight_wraps_into_the_day() {
        let honolulu = Position {
            latitude_degrees: 21.3099,
            longitude_degrees: -157.8581,
        };
        let date = Date {
            year: 2026,
            month: 6,
            day: 21,
        };
        let local_daylight = compute_daylight(date, -10 * 60, honolulu);
        assert!(matches!(local_daylight, Daylight::Cycle { .. }));
        assert_eq!(compute_daylight(date, 14 * 60, honolulu), local_daylight);
    }
}

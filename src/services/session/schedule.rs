use crate::core::config::ScheduleMode;
use crate::core::sun::Daylight;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    Between { from_minutes: i32, to_minutes: i32 },
    Always,
    Never,
}

impl Span {
    pub fn resolve(
        mode: ScheduleMode,
        fixed_from_minutes: i32,
        fixed_to_minutes: i32,
        daylight: Option<Daylight>,
    ) -> Span {
        match (mode, daylight) {
            (
                ScheduleMode::Sun,
                Some(Daylight::Cycle {
                    sunrise_minutes,
                    sunset_minutes,
                }),
            ) => Span::Between {
                from_minutes: sunset_minutes,
                to_minutes: sunrise_minutes,
            },
            (ScheduleMode::Sun, Some(Daylight::AlwaysNight)) => Span::Always,
            (ScheduleMode::Sun, Some(Daylight::AlwaysDay)) => Span::Never,
            _ => Span::Between {
                from_minutes: fixed_from_minutes,
                to_minutes: fixed_to_minutes,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_schedule_runs_from_sunset_to_sunrise_and_the_fixed_one_from_its_times() {
        let cycle = Some(Daylight::Cycle {
            sunrise_minutes: 5 * 60 + 40,
            sunset_minutes: 20 * 60 + 5,
        });
        let fixed = Span::Between {
            from_minutes: 19 * 60,
            to_minutes: 6 * 60 + 30,
        };
        let cases = [
            (
                ScheduleMode::Sun,
                cycle,
                Span::Between {
                    from_minutes: 20 * 60 + 5,
                    to_minutes: 5 * 60 + 40,
                },
            ),
            (ScheduleMode::Times, cycle, fixed),
            (ScheduleMode::Sun, None, fixed),
            (ScheduleMode::Sun, Some(Daylight::AlwaysNight), Span::Always),
            (ScheduleMode::Sun, Some(Daylight::AlwaysDay), Span::Never),
            (ScheduleMode::Times, Some(Daylight::AlwaysDay), fixed),
        ];
        for (mode, daylight, expected) in cases {
            assert_eq!(
                Span::resolve(mode, 19 * 60, 6 * 60 + 30, daylight),
                expected,
                "{mode:?} with {daylight:?}"
            );
        }
    }
}

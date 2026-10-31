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

    pub fn contains(self, now_minutes: i32) -> bool {
        match self {
            Span::Always => true,
            Span::Never => false,
            Span::Between {
                from_minutes,
                to_minutes,
            } if from_minutes <= to_minutes => (from_minutes..to_minutes).contains(&now_minutes),
            Span::Between {
                from_minutes,
                to_minutes,
            } => now_minutes >= from_minutes || now_minutes < to_minutes,
        }
    }
}

pub fn plan_dark_switch(
    previous_schedule_is_dark: Option<bool>,
    schedule_is_dark: bool,
    is_dark_now: bool,
) -> Option<bool> {
    let is_transition = previous_schedule_is_dark != Some(schedule_is_dark);
    (is_transition && is_dark_now != schedule_is_dark).then_some(schedule_is_dark)
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

    #[test]
    fn a_span_that_wraps_past_midnight_holds_from_its_start_until_its_end() {
        let evening = Span::Between {
            from_minutes: 20 * 60,
            to_minutes: 7 * 60,
        };
        let daytime = Span::Between {
            from_minutes: 7 * 60,
            to_minutes: 20 * 60,
        };
        let cases = [
            (evening, 19 * 60 + 59, false),
            (evening, 20 * 60, true),
            (evening, 0, true),
            (evening, 6 * 60 + 59, true),
            (evening, 7 * 60, false),
            (daytime, 7 * 60, true),
            (daytime, 19 * 60 + 59, true),
            (daytime, 20 * 60, false),
            (daytime, 3 * 60, false),
            (Span::Always, 12 * 60, true),
            (Span::Never, 12 * 60, false),
        ];
        for (span, now_minutes, expected) in cases {
            assert_eq!(
                span.contains(now_minutes),
                expected,
                "{span:?} at {now_minutes}"
            );
        }
    }

    #[test]
    fn dark_mode_switches_at_the_start_and_at_a_transition_when_the_state_is_wrong() {
        let cases = [
            (None, true, false, Some(true)),
            (None, true, true, None),
            (None, false, true, Some(false)),
            (None, false, false, None),
            (Some(false), true, false, Some(true)),
            (Some(true), false, true, Some(false)),
            (Some(false), true, true, None),
            (Some(true), true, false, None),
            (Some(false), false, true, None),
        ];
        for (previous_schedule_is_dark, schedule_is_dark, is_dark_now, expected) in cases {
            assert_eq!(
                plan_dark_switch(previous_schedule_is_dark, schedule_is_dark, is_dark_now),
                expected,
                "{previous_schedule_is_dark:?} scheduled {schedule_is_dark} now {is_dark_now}"
            );
        }
    }

    #[test]
    fn a_day_of_ticks_switches_only_at_the_transitions_and_a_manual_switch_holds_until_the_next() {
        let evening = Span::Between {
            from_minutes: 20 * 60,
            to_minutes: 7 * 60,
        };
        let run = |ticks: &[i32], manual_switch: Option<(i32, bool)>| {
            let (mut previous_schedule_is_dark, mut is_dark, mut switches) =
                (None, false, Vec::new());
            for &minute in ticks {
                if let Some((switch_minute, is_dark_after)) = manual_switch
                    && minute == switch_minute
                {
                    is_dark = is_dark_after;
                }
                let schedule_is_dark = evening.contains(minute);
                if let Some(target) =
                    plan_dark_switch(previous_schedule_is_dark, schedule_is_dark, is_dark)
                {
                    switches.push((minute, target));
                    is_dark = target;
                }
                previous_schedule_is_dark = Some(schedule_is_dark);
            }
            switches
        };
        let every_minute: Vec<i32> = (0..24 * 60).collect();
        assert_eq!(
            run(&every_minute, None),
            [(0, true), (7 * 60, false), (20 * 60, true)]
        );
        assert_eq!(
            run(&every_minute, Some((3 * 60, false))),
            [(0, true), (20 * 60, true)]
        );
        assert_eq!(
            run(&every_minute, Some((10 * 60, true))),
            [(0, true), (7 * 60, false)]
        );
        assert_eq!(
            run(&[12 * 60, 12 * 60 + 1, 22 * 60, 22 * 60 + 1], None),
            [(22 * 60, true)]
        );
    }
}

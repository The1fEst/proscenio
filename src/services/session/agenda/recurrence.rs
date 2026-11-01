use super::civil::{SECONDS_PER_DAY, civil_from_days, days_from_civil, days_in_month, weekday};
use super::time::Time;
use crate::core::icalendar::Property;

const MAX_PERIODS: i64 = 200_000;
const WEEKDAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, PartialEq)]
pub struct Rule {
    frequency: Option<Frequency>,
    interval: i64,
    count: Option<u64>,
    pub until: Option<Time>,
    by_day: Vec<(i64, i64)>,
    by_month_day: Vec<i64>,
    by_month: Vec<i64>,
    week_start: i64,
}

impl Rule {
    pub fn parse(value: &str) -> Rule {
        let mut rule = Rule {
            frequency: None,
            interval: 1,
            count: None,
            until: None,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            by_month: Vec::new(),
            week_start: 0,
        };
        let mut is_supported = true;
        for part in value.split(';') {
            let Some((key, text)) = part.split_once('=') else {
                continue;
            };
            let numbers = || {
                text.split(',')
                    .filter_map(|entry| entry.trim().parse::<i64>().ok())
            };
            match key.to_ascii_uppercase().as_str() {
                "FREQ" => {
                    rule.frequency = match text.to_ascii_uppercase().as_str() {
                        "DAILY" => Some(Frequency::Daily),
                        "WEEKLY" => Some(Frequency::Weekly),
                        "MONTHLY" => Some(Frequency::Monthly),
                        "YEARLY" => Some(Frequency::Yearly),
                        _ => None,
                    }
                }
                "INTERVAL" => rule.interval = text.parse().unwrap_or(1).max(1),
                "COUNT" => rule.count = text.parse().ok(),
                "UNTIL" => {
                    rule.until = Time::parse(&Property {
                        value: text.to_owned(),
                        ..Property::default()
                    })
                }
                "BYDAY" => rule.by_day = text.split(',').filter_map(parse_day).collect(),
                "BYMONTHDAY" => rule.by_month_day = numbers().collect(),
                "BYMONTH" => rule.by_month = numbers().collect(),
                "WKST" => {
                    rule.week_start = WEEKDAYS
                        .iter()
                        .position(|name| name.eq_ignore_ascii_case(text))
                        .unwrap_or(0) as i64
                }
                "BYSETPOS" | "BYWEEKNO" | "BYYEARDAY" | "BYHOUR" | "BYMINUTE" | "BYSECOND" => {
                    is_supported = false
                }
                _ => {}
            }
        }
        let is_year_by_day = rule.frequency == Some(Frequency::Yearly)
            && !rule.by_day.is_empty()
            && rule.by_month.is_empty();
        if !is_supported || is_year_by_day {
            rule.frequency = None;
        }
        rule
    }

    pub fn expand(&self, first: i64, from: i64, to: i64) -> Vec<i64> {
        let Some(frequency) = self.frequency else {
            return vec![first];
        };
        let first_day = first.div_euclid(SECONDS_PER_DAY);
        let clock = first.rem_euclid(SECONDS_PER_DAY);
        let to_day = to.div_euclid(SECONDS_PER_DAY);
        let mut period = match self.count {
            Some(_) => 0,
            None => {
                self.count_periods_before(frequency, first_day, from.div_euclid(SECONDS_PER_DAY))
            }
        };
        let mut emitted_count = 1;
        let mut starts = Vec::new();
        if first >= from && first < to {
            starts.push(first);
        }
        while period <= MAX_PERIODS
            && self.find_period_start(frequency, first_day, period) <= to_day
        {
            for day in self.list_period_days(frequency, first_day, period) {
                if day <= first_day {
                    continue;
                }
                emitted_count += 1;
                if self.count.is_some_and(|count| emitted_count > count) {
                    return starts;
                }
                let start = day * SECONDS_PER_DAY + clock;
                if start >= to {
                    return starts;
                }
                if start >= from {
                    starts.push(start);
                }
            }
            period += 1;
        }
        starts
    }

    fn count_periods_before(&self, frequency: Frequency, first_day: i64, day: i64) -> i64 {
        let (first_year, first_month, _) = civil_from_days(first_day);
        let (year, month, _) = civil_from_days(day);
        let distance = match frequency {
            Frequency::Daily => day - first_day,
            Frequency::Weekly => (day - first_day) / 7,
            Frequency::Monthly => (year * 12 + month) - (first_year * 12 + first_month),
            Frequency::Yearly => year - first_year,
        };
        (distance / self.interval - 1).max(0)
    }

    fn find_period_start(&self, frequency: Frequency, first_day: i64, period: i64) -> i64 {
        let step = period * self.interval;
        let (year, month, _) = civil_from_days(first_day);
        match frequency {
            Frequency::Daily => first_day + step,
            Frequency::Weekly => self.find_week_start(first_day) + 7 * step,
            Frequency::Monthly => {
                let (year, month) = add_months(year, month, step);
                days_from_civil(year, month, 1)
            }
            Frequency::Yearly => days_from_civil(year + step, 1, 1),
        }
    }

    fn find_week_start(&self, day: i64) -> i64 {
        day - (weekday(day) - self.week_start).rem_euclid(7)
    }

    fn list_period_days(&self, frequency: Frequency, first_day: i64, period: i64) -> Vec<i64> {
        let step = period * self.interval;
        let (first_year, first_month, first_date) = civil_from_days(first_day);
        match frequency {
            Frequency::Daily => {
                let day = first_day + step;
                let (year, month, date) = civil_from_days(day);
                let is_listed = |list: &[i64], value: i64| list.is_empty() || list.contains(&value);
                let is_match = is_listed(&self.by_month, month)
                    && (self.by_month_day.is_empty()
                        || self.by_month_day.iter().any(|&month_day| {
                            resolve_month_day(month_day, days_in_month(year, month))
                                .is_some_and(|resolved| resolved == date)
                        }))
                    && (self.by_day.is_empty()
                        || self.by_day.iter().any(|&(_, name)| name == weekday(day)));
                if is_match { vec![day] } else { Vec::new() }
            }
            Frequency::Weekly => {
                let start = self.find_week_start(first_day) + 7 * step;
                let mut names: Vec<i64> = self.by_day.iter().map(|&(_, name)| name).collect();
                if names.is_empty() {
                    names.push(weekday(first_day));
                }
                let mut days: Vec<i64> = names
                    .iter()
                    .map(|name| start + (name - self.week_start).rem_euclid(7))
                    .filter(|&day| {
                        self.by_month.is_empty() || self.by_month.contains(&civil_from_days(day).1)
                    })
                    .collect();
                days.sort_unstable();
                days.dedup();
                days
            }
            Frequency::Monthly => {
                let (year, month) = add_months(first_year, first_month, step);
                if !self.by_month.is_empty() && !self.by_month.contains(&month) {
                    return Vec::new();
                }
                self.list_month_days(year, month, first_date)
            }
            Frequency::Yearly => {
                let year = first_year + step;
                let months: Vec<i64> = if !self.by_month.is_empty() {
                    self.by_month.clone()
                } else if !self.by_month_day.is_empty() {
                    (1..=12).collect()
                } else {
                    vec![first_month]
                };
                let mut days: Vec<i64> = months
                    .iter()
                    .flat_map(|&month| self.list_month_days(year, month, first_date))
                    .collect();
                days.sort_unstable();
                days.dedup();
                days
            }
        }
    }

    fn list_month_days(&self, year: i64, month: i64, default_date: i64) -> Vec<i64> {
        let length = days_in_month(year, month);
        let first = days_from_civil(year, month, 1);
        let by_month_day: Option<Vec<i64>> = (!self.by_month_day.is_empty()).then(|| {
            self.by_month_day
                .iter()
                .filter_map(|&month_day| resolve_month_day(month_day, length))
                .map(|date| first + date - 1)
                .collect()
        });
        let by_day: Option<Vec<i64>> = (!self.by_day.is_empty()).then(|| {
            self.by_day
                .iter()
                .flat_map(|&(ordinal, name)| nth_weekdays(first, length, ordinal, name))
                .collect()
        });
        let mut days = match (by_month_day, by_day) {
            (Some(month_days), Some(weekday_days)) => month_days
                .into_iter()
                .filter(|day| weekday_days.contains(day))
                .collect(),
            (Some(days), None) | (None, Some(days)) => days,
            (None, None) if default_date <= length => vec![first + default_date - 1],
            (None, None) => Vec::new(),
        };
        days.sort_unstable();
        days.dedup();
        days
    }
}

fn parse_day(entry: &str) -> Option<(i64, i64)> {
    let entry = entry.trim();
    let split = entry.len().checked_sub(2)?;
    let name_text = entry.get(split..)?;
    let name = WEEKDAYS
        .iter()
        .position(|name| name.eq_ignore_ascii_case(name_text))? as i64;
    let prefix = &entry[..split];
    let ordinal = if prefix.is_empty() {
        0
    } else {
        prefix.trim_start_matches('+').parse().ok()?
    };
    Some((ordinal, name))
}

fn resolve_month_day(month_day: i64, length: i64) -> Option<i64> {
    let date = if month_day < 0 {
        length + 1 + month_day
    } else {
        month_day
    };
    (1..=length).contains(&date).then_some(date)
}

fn nth_weekdays(first: i64, length: i64, ordinal: i64, name: i64) -> Vec<i64> {
    let same_weekdays: Vec<i64> = (first..first + length)
        .filter(|&day| weekday(day) == name)
        .collect();
    let index = match ordinal {
        0 => return same_weekdays,
        1.. => ordinal - 1,
        _ => same_weekdays.len() as i64 + ordinal,
    };
    usize::try_from(index)
        .ok()
        .and_then(|index| same_weekdays.get(index).copied())
        .into_iter()
        .collect()
}

fn add_months(year: i64, month: i64, months: i64) -> (i64, i64) {
    let total = year * 12 + month - 1 + months;
    (total.div_euclid(12), total.rem_euclid(12) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_days(year: i64, month: i64, date: i64) -> i64 {
        days_from_civil(year, month, date)
    }

    fn expand_dates(rule: &str, first: i64, from: i64, to: i64) -> Vec<(i64, i64, i64)> {
        Rule::parse(rule)
            .expand(
                first * SECONDS_PER_DAY + 3600,
                from * SECONDS_PER_DAY,
                to * SECONDS_PER_DAY,
            )
            .iter()
            .map(|start| civil_from_days(start.div_euclid(SECONDS_PER_DAY)))
            .collect()
    }

    #[test]
    fn daily_rules_honor_interval_and_count() {
        let first = to_days(2026, 11, 1);
        assert_eq!(
            expand_dates("FREQ=DAILY;INTERVAL=3;COUNT=3", first, first, first + 100),
            [(2026, 11, 1), (2026, 11, 4), (2026, 11, 7)]
        );
        assert_eq!(
            expand_dates(
                "FREQ=DAILY;INTERVAL=2",
                first,
                to_days(2026, 11, 5),
                to_days(2026, 11, 10)
            ),
            [(2026, 11, 5), (2026, 11, 7), (2026, 11, 9)]
        );
    }

    #[test]
    fn a_count_includes_the_occurrences_before_the_window() {
        let first = to_days(2026, 11, 1);
        assert_eq!(
            expand_dates(
                "FREQ=DAILY;COUNT=5",
                first,
                to_days(2026, 11, 4),
                to_days(2026, 11, 30)
            ),
            [(2026, 11, 4), (2026, 11, 5)]
        );
    }

    #[test]
    fn skipping_ahead_finds_the_same_days_as_counting_from_the_start() {
        let first = to_days(2020, 1, 30);
        for rule in [
            "FREQ=DAILY;INTERVAL=7",
            "FREQ=WEEKLY;INTERVAL=3;BYDAY=FR,MO",
            "FREQ=MONTHLY;INTERVAL=2",
            "FREQ=YEARLY;INTERVAL=2;BYMONTH=11,12",
        ] {
            let from = to_days(2026, 11, 1);
            let to = to_days(2026, 12, 13);
            let skipped_dates = expand_dates(rule, first, from, to);
            let counted_dates = expand_dates(&format!("{rule};COUNT=100000"), first, from, to);
            assert_eq!(skipped_dates, counted_dates, "{rule}");
            assert!(!skipped_dates.is_empty(), "{rule}");
        }
    }

    #[test]
    fn weekly_rules_list_the_named_weekdays_of_every_interval_week() {
        let first = to_days(2026, 11, 3);
        assert_eq!(
            expand_dates(
                "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,TH",
                first,
                first,
                to_days(2026, 12, 1)
            ),
            [(2026, 11, 3), (2026, 11, 5), (2026, 11, 17), (2026, 11, 19)]
        );
        assert_eq!(
            expand_dates("FREQ=WEEKLY", first, first, to_days(2026, 11, 25)),
            [
                (2026, 11, 3),
                (2026, 11, 10),
                (2026, 11, 17),
                (2026, 11, 24)
            ]
        );
    }

    #[test]
    fn a_week_start_changes_which_days_share_an_interval_week() {
        let first = to_days(2026, 11, 8);
        let monday_weeks = expand_dates(
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,SU",
            first,
            first,
            to_days(2026, 12, 1),
        );
        let sunday_weeks = expand_dates(
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,SU;WKST=SU",
            first,
            first,
            to_days(2026, 12, 1),
        );
        assert_eq!(
            monday_weeks,
            [
                (2026, 11, 8),
                (2026, 11, 16),
                (2026, 11, 22),
                (2026, 11, 30)
            ]
        );
        assert_eq!(
            sunday_weeks,
            [(2026, 11, 8), (2026, 11, 9), (2026, 11, 22), (2026, 11, 23)]
        );
    }

    #[test]
    fn monthly_rules_resolve_ordinal_weekdays_and_negative_month_days() {
        let first = to_days(2026, 11, 10);
        let to = to_days(2027, 2, 1);
        assert_eq!(
            expand_dates("FREQ=MONTHLY;BYDAY=2TU", first, first, to),
            [(2026, 11, 10), (2026, 12, 8), (2027, 1, 12)]
        );
        assert_eq!(
            expand_dates("FREQ=MONTHLY;BYDAY=-1FR", first, first, to),
            [
                (2026, 11, 10),
                (2026, 11, 27),
                (2026, 12, 25),
                (2027, 1, 29)
            ]
        );
        assert_eq!(
            expand_dates("FREQ=MONTHLY;BYMONTHDAY=-1", first, first, to),
            [
                (2026, 11, 10),
                (2026, 11, 30),
                (2026, 12, 31),
                (2027, 1, 31)
            ]
        );
        assert_eq!(
            expand_dates(
                "FREQ=MONTHLY;BYDAY=MO,TU;BYMONTHDAY=1,2,3",
                first,
                first,
                to
            ),
            [(2026, 11, 10), (2026, 12, 1)]
        );
    }

    #[test]
    fn the_start_is_always_the_first_instance_and_counts_toward_the_count() {
        let first = to_days(2026, 11, 10);
        let to = first + 100;
        assert_eq!(
            expand_dates("FREQ=WEEKLY;BYDAY=FR;COUNT=3", first, first, to),
            [(2026, 11, 10), (2026, 11, 13), (2026, 11, 20)]
        );
        assert_eq!(
            expand_dates("FREQ=WEEKLY;BYDAY=TU;COUNT=3", first, first, to),
            [(2026, 11, 10), (2026, 11, 17), (2026, 11, 24)]
        );
        assert_eq!(
            expand_dates("FREQ=WEEKLY;BYDAY=FR;COUNT=1", first, first, to),
            [(2026, 11, 10)]
        );
        assert_eq!(
            expand_dates("FREQ=WEEKLY;BYDAY=FR;COUNT=3", first, first + 1, to),
            [(2026, 11, 13), (2026, 11, 20)]
        );
    }

    #[test]
    fn monthly_rules_skip_months_without_the_start_date() {
        let first = to_days(2026, 1, 31);
        assert_eq!(
            expand_dates("FREQ=MONTHLY;COUNT=4", first, first, to_days(2027, 1, 1)),
            [(2026, 1, 31), (2026, 3, 31), (2026, 5, 31), (2026, 7, 31)]
        );
    }

    #[test]
    fn yearly_rules_repeat_the_start_date_and_skip_missing_leap_days() {
        let first = to_days(2024, 2, 29);
        assert_eq!(
            expand_dates(
                "FREQ=YEARLY",
                first,
                to_days(2024, 1, 1),
                to_days(2029, 1, 1)
            ),
            [(2024, 2, 29), (2028, 2, 29)]
        );
        let birthday = to_days(1990, 11, 5);
        assert_eq!(
            expand_dates(
                "FREQ=YEARLY",
                birthday,
                to_days(2026, 11, 1),
                to_days(2026, 11, 30)
            ),
            [(2026, 11, 5)]
        );
        assert_eq!(
            expand_dates(
                "FREQ=YEARLY;BYMONTH=11;BYDAY=4TH",
                birthday,
                to_days(2026, 11, 1),
                to_days(2026, 11, 30)
            ),
            [(2026, 11, 26)]
        );
    }

    #[test]
    fn rules_that_are_not_supported_give_only_the_first_start() {
        let first = to_days(2026, 11, 3);
        for rule in [
            "FREQ=HOURLY",
            "FREQ=MONTHLY;BYDAY=MO;BYSETPOS=-1",
            "FREQ=YEARLY;BYDAY=20MO",
            "FREQ=DAILY;BYHOUR=9,17",
        ] {
            assert_eq!(
                expand_dates(rule, first, first, first + 90),
                [(2026, 11, 3)],
                "{rule}"
            );
        }
    }

    #[test]
    fn the_until_value_is_kept_for_the_caller() {
        let rule = Rule::parse("FREQ=DAILY;UNTIL=20261231T215959Z");
        assert_eq!(
            rule.until.unwrap().wall,
            to_days(2026, 12, 31) * SECONDS_PER_DAY + 21 * 3600 + 59 * 60 + 59
        );
    }
}

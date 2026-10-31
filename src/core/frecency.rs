use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 200;
const SAMPLE_LIMIT: u32 = 10;
const DAY_SECS: u64 = 86_400;
const RECENCY_WEIGHTS: [(u64, u32); 4] = [(4, 100), (14, 70), (31, 50), (90, 30)];
const OLDEST_WEIGHT: u32 = 10;
const MAX_WEIGHT: u32 = 100;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Usage {
    count: u32,
    last_used_secs: u64,
}

#[derive(Default)]
pub struct UsageHistory {
    entries: HashMap<String, Usage>,
}

impl UsageHistory {
    pub fn load() -> Self {
        Self::read_from(&path())
    }

    pub fn save(&self) {
        self.write_to(&path());
    }

    fn read_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&text) else {
            return Self::default();
        };
        let entries = object
            .into_iter()
            .filter_map(|(id, value)| {
                let usage = Usage {
                    count: value.get("count")?.as_u64()?.try_into().ok()?,
                    last_used_secs: value.get("lastUsed")?.as_u64()?,
                };
                Some((id, usage))
            })
            .collect();
        UsageHistory { entries }
    }

    fn write_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let object: serde_json::Map<String, Value> = self
            .entries
            .iter()
            .map(|(id, usage)| {
                let value = json!({ "count": usage.count, "lastUsed": usage.last_used_secs });
                (id.clone(), value)
            })
            .collect();
        let _ = std::fs::write(path, Value::Object(object).to_string());
    }

    pub fn record(&mut self, id: &str, now_secs: u64) {
        let usage = self.entries.entry(id.to_owned()).or_insert(Usage {
            count: 0,
            last_used_secs: now_secs,
        });
        usage.count = usage.count.saturating_add(1);
        usage.last_used_secs = now_secs;
        self.trim(id, now_secs);
    }

    pub fn score(&self, id: &str, now_secs: u64) -> f64 {
        self.entries
            .get(id)
            .map_or(0.0, |usage| usage_score(usage, now_secs))
    }

    fn trim(&mut self, recorded_id: &str, now_secs: u64) {
        let excess = self.entries.len().saturating_sub(MAX_ENTRIES);
        if excess == 0 {
            return;
        }
        let mut candidates: Vec<(f64, u64, String)> = self
            .entries
            .iter()
            .filter(|(id, _)| id.as_str() != recorded_id)
            .map(|(id, usage)| {
                (
                    usage_score(usage, now_secs),
                    usage.last_used_secs,
                    id.clone(),
                )
            })
            .collect();
        candidates
            .sort_by(|first, second| first.0.total_cmp(&second.0).then(first.1.cmp(&second.1)));
        for (_, _, id) in candidates.into_iter().take(excess) {
            self.entries.remove(&id);
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn path() -> PathBuf {
    crate::core::paths::state().join("frecency.json")
}

fn recency_weight(age_secs: u64) -> u32 {
    RECENCY_WEIGHTS
        .iter()
        .find(|(max_days, _)| age_secs <= max_days * DAY_SECS)
        .map_or(OLDEST_WEIGHT, |(_, weight)| *weight)
}

fn usage_score(usage: &Usage, now_secs: u64) -> f64 {
    let weight = recency_weight(now_secs.saturating_sub(usage.last_used_secs));
    let sample = usage.count.min(SAMPLE_LIMIT);
    f64::from(weight * sample) / f64::from(MAX_WEIGHT * SAMPLE_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_SECS: u64 = 1_800_000_000;

    fn build_history(entries: &[(&str, u32, u64)]) -> UsageHistory {
        UsageHistory {
            entries: entries
                .iter()
                .map(|(id, count, age_days)| {
                    let usage = Usage {
                        count: *count,
                        last_used_secs: NOW_SECS - age_days * DAY_SECS,
                    };
                    (id.to_string(), usage)
                })
                .collect(),
        }
    }

    #[test]
    fn the_recency_bucket_sets_the_weight() {
        let cases = [
            (0, 100),
            (4, 100),
            (5, 70),
            (14, 70),
            (15, 50),
            (31, 50),
            (32, 30),
            (90, 30),
            (91, 10),
            (4000, 10),
        ];
        for (age_days, weight) in cases {
            assert_eq!(recency_weight(age_days * DAY_SECS), weight, "{age_days}");
        }
        assert_eq!(recency_weight(4 * DAY_SECS + 1), 70);
    }

    #[test]
    fn the_score_grows_with_the_count_up_to_the_sample_limit() {
        let history = build_history(&[("one", 1, 0), ("ten", 10, 0), ("fifty", 50, 0)]);
        assert!((history.score("one", NOW_SECS) - 0.1).abs() < 1e-9);
        assert!((history.score("ten", NOW_SECS) - 1.0).abs() < 1e-9);
        assert_eq!(
            history.score("fifty", NOW_SECS),
            history.score("ten", NOW_SECS)
        );
        assert_eq!(history.score("unknown", NOW_SECS), 0.0);
    }

    #[test]
    fn a_recent_launch_outscores_older_ones_with_the_same_count() {
        let history = build_history(&[
            ("today", 5, 0),
            ("week", 5, 10),
            ("month", 5, 20),
            ("quarter", 5, 60),
            ("year", 5, 300),
        ]);
        let scores: Vec<f64> = ["today", "week", "month", "quarter", "year"]
            .iter()
            .map(|id| history.score(id, NOW_SECS))
            .collect();
        assert!(
            scores.windows(2).all(|pair| pair[0] > pair[1]),
            "{scores:?}"
        );
        assert!(scores[4] > 0.0);
    }

    #[test]
    fn a_launch_in_the_future_counts_as_just_now() {
        let mut history = UsageHistory::default();
        history.record("clock", NOW_SECS + 1000);
        assert_eq!(history.score("clock", NOW_SECS), 0.1);
    }

    #[test]
    fn recording_counts_and_stamps_each_launch() {
        let mut history = UsageHistory::default();
        history.record("app", NOW_SECS - 10 * DAY_SECS);
        history.record("app", NOW_SECS);
        assert_eq!(
            history.entries["app"],
            Usage {
                count: 2,
                last_used_secs: NOW_SECS
            }
        );
    }

    #[test]
    fn the_history_keeps_the_most_valuable_entries_at_the_cap() {
        let mut history = UsageHistory::default();
        for index in 0..MAX_ENTRIES {
            history.record(&format!("app-{index}"), NOW_SECS - index as u64);
        }
        history.record("app-0", NOW_SECS);
        history.record("app-0", NOW_SECS);
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        history.record("newcomer", NOW_SECS);
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert!(history.entries.contains_key("newcomer"));
        assert!(history.entries.contains_key("app-0"));
        assert!(!history.entries.contains_key("app-199"));
        assert!(history.entries.contains_key("app-198"));
    }

    #[test]
    fn a_first_launch_survives_the_cap_among_well_used_apps() {
        let mut history = UsageHistory::default();
        for index in 0..MAX_ENTRIES {
            for _ in 0..3 {
                history.record(&format!("app-{index}"), NOW_SECS);
            }
        }
        history.record("newcomer", NOW_SECS);
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert!(history.entries.contains_key("newcomer"));
    }

    #[test]
    fn the_cap_drops_old_entries_before_recent_ones() {
        let mut history = build_history(&[("old", 1, 200), ("recent", 1, 1)]);
        for index in 0..MAX_ENTRIES - 1 {
            history.record(&format!("filler-{index}"), NOW_SECS);
        }
        assert!(!history.entries.contains_key("old"));
        assert!(history.entries.contains_key("recent"));
    }

    #[test]
    fn the_history_survives_a_round_trip_through_its_file() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-frecency-{}", std::process::id()));
        let file = folder.join("nested/frecency.json");
        let mut original = UsageHistory::default();
        original.record("firefox.desktop", NOW_SECS - 5);
        original.record("firefox.desktop", NOW_SECS);
        original.record("kitty.desktop", NOW_SECS);
        original.write_to(&file);
        let copy = UsageHistory::read_from(&file);
        assert_eq!(copy.entries, original.entries);
        std::fs::write(
            &file,
            r#"{"ok.desktop": {"count": 3, "lastUsed": 7}, "bad.desktop": {"count": "x"}}"#,
        )
        .unwrap();
        assert_eq!(UsageHistory::read_from(&file).entries.len(), 1);
        std::fs::write(&file, "not json").unwrap();
        assert!(UsageHistory::read_from(&file).entries.is_empty());
        assert!(
            UsageHistory::read_from(&folder.join("missing.json"))
                .entries
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}

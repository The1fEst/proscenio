use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const SHARING_GRACE: Duration = Duration::from_secs(3);
pub const SHARING_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Monitor,
    Window,
    Region,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Target {
    pub kind: Kind,
    pub name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Session {
    pub is_active: bool,
    pub target: Target,
}

pub fn parse_session(data: &str) -> Option<Session> {
    let mut fields = data.splitn(3, ',');
    let is_active = match fields.next()? {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    let kind = match fields.next()? {
        "monitor" => Kind::Monitor,
        "window" => Kind::Window,
        "region" => Kind::Region,
        _ => return None,
    };
    let name = fields.next().unwrap_or_default().to_owned();
    Some(Session {
        is_active,
        target: Target { kind, name },
    })
}

#[derive(Default)]
pub struct Sessions {
    counts: HashMap<Target, SessionCounts>,
}

#[derive(Default)]
struct SessionCounts {
    own_count: u32,
    external_count: u32,
    external_since: Option<Instant>,
}

impl Sessions {
    pub fn start(&mut self, target: Target, is_own: bool, now: Instant) {
        let counts = self.counts.entry(target).or_default();
        if is_own {
            counts.own_count += 1;
        } else {
            counts.external_count += 1;
            counts.external_since.get_or_insert(now);
        }
    }

    pub fn own_count(&self, target: &Target) -> usize {
        self.counts
            .get(target)
            .map_or(0, |counts| counts.own_count as usize)
    }

    // A stop event names no session. It is charged to the shell's own sessions first, which end
    // within a second, and a window name that matches no start (the title changed mid-share) falls
    // back to the only window session there is.
    pub fn stop(&mut self, target: &Target) {
        let key = if self.counts.contains_key(target) {
            target.clone()
        } else {
            let mut windows = self.counts.keys().filter(|key| key.kind == Kind::Window);
            match (target.kind, windows.next(), windows.next()) {
                (Kind::Window, Some(only), None) => only.clone(),
                _ => return,
            }
        };
        let Some(counts) = self.counts.get_mut(&key) else {
            return;
        };
        if counts.own_count > 0 {
            counts.own_count -= 1;
        } else {
            counts.external_count = counts.external_count.saturating_sub(1);
        }
        if counts.external_count == 0 {
            counts.external_since = None;
        }
        if counts.own_count == 0 && counts.external_count == 0 {
            self.counts.remove(&key);
        }
    }

    pub fn confirmed_targets(
        &self,
        now: Instant,
        min_age: Duration,
        exempt_targets: &[Target],
    ) -> Vec<Target> {
        let mut targets: Vec<Target> = self
            .counts
            .iter()
            .filter(|(target, counts)| {
                counts.external_count > 0
                    && (exempt_targets.contains(target) || age_of(counts, now) >= min_age)
            })
            .map(|(target, _)| target.clone())
            .collect();
        targets.sort();
        targets
    }

    pub fn next_confirmation(
        &self,
        now: Instant,
        min_age: Duration,
        exempt_targets: &[Target],
    ) -> Option<Duration> {
        self.counts
            .iter()
            .filter(|(target, counts)| {
                counts.external_count > 0 && !exempt_targets.contains(target)
            })
            .map(|(_, counts)| min_age.saturating_sub(age_of(counts, now)))
            .filter(|wait| !wait.is_zero())
            .min()
    }
}

fn age_of(counts: &SessionCounts, now: Instant) -> Duration {
    counts
        .external_since
        .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: Kind, name: &str) -> Target {
        Target {
            kind,
            name: name.to_owned(),
        }
    }

    fn external_targets(sessions: &Sessions) -> Vec<Target> {
        sessions.confirmed_targets(Instant::now(), Duration::ZERO, &[])
    }

    #[test]
    fn a_session_event_names_its_kind_and_target() {
        let cases = [
            ("1,monitor,DP-2", Some((true, Kind::Monitor, "DP-2"))),
            (
                "0,window,Title, with commas",
                Some((false, Kind::Window, "Title, with commas")),
            ),
            ("1,region,HDMI-A-1", Some((true, Kind::Region, "HDMI-A-1"))),
            ("1,window", Some((true, Kind::Window, ""))),
            ("1,0,DP-2", None),
            ("2,monitor,DP-2", None),
            ("monitor", None),
            ("", None),
        ];
        for (data, expected) in cases {
            let expected = expected.map(|(is_active, kind, name)| Session {
                is_active,
                target: target(kind, name),
            });
            assert_eq!(parse_session(data), expected, "{data}");
        }
    }

    #[test]
    fn only_sessions_the_shell_did_not_start_count_as_external() {
        let now = Instant::now();
        let window = target(Kind::Window, "Title");
        let monitor = target(Kind::Monitor, "DP-2");
        let mut sessions = Sessions::default();
        sessions.start(window.clone(), true, now);
        assert_eq!(external_targets(&sessions), []);
        sessions.start(monitor.clone(), false, now);
        sessions.start(window.clone(), false, now);
        assert_eq!(
            external_targets(&sessions),
            [monitor.clone(), window.clone()]
        );
        sessions.stop(&window);
        assert_eq!(
            external_targets(&sessions),
            [monitor.clone(), window.clone()]
        );
        sessions.stop(&window);
        assert_eq!(external_targets(&sessions), [monitor.clone()]);
        sessions.stop(&monitor);
        sessions.stop(&monitor);
        assert_eq!(external_targets(&sessions), []);
    }

    #[test]
    fn a_stop_under_a_changed_window_title_ends_a_window_session() {
        let now = Instant::now();
        let monitor = target(Kind::Monitor, "DP-2");
        let mut sessions = Sessions::default();
        sessions.start(target(Kind::Window, "Tab A"), false, now);
        sessions.start(monitor.clone(), false, now);
        sessions.stop(&target(Kind::Window, "Tab B"));
        assert_eq!(external_targets(&sessions), [monitor.clone()]);
        sessions.stop(&target(Kind::Window, "Tab B"));
        assert_eq!(external_targets(&sessions), [monitor]);
    }

    #[test]
    fn a_stop_under_an_unknown_window_title_ends_nothing_among_several_windows() {
        let now = Instant::now();
        let windows = [target(Kind::Window, "Tab A"), target(Kind::Window, "Chat")];
        let mut sessions = Sessions::default();
        for window in &windows {
            sessions.start(window.clone(), false, now);
        }
        sessions.stop(&target(Kind::Window, "Tab B"));
        assert_eq!(
            external_targets(&sessions),
            [windows[1].clone(), windows[0].clone()]
        );
    }

    #[test]
    fn a_stop_under_another_monitor_name_ends_nothing() {
        let monitor = target(Kind::Monitor, "DP-2");
        let mut sessions = Sessions::default();
        sessions.start(monitor.clone(), false, Instant::now());
        sessions.stop(&target(Kind::Monitor, "HDMI-A-1"));
        assert_eq!(external_targets(&sessions), [monitor]);
    }

    #[test]
    fn a_target_counts_the_sessions_the_shell_started() {
        let now = Instant::now();
        let window = target(Kind::Window, "Title");
        let mut sessions = Sessions::default();
        assert_eq!(sessions.own_count(&window), 0);
        sessions.start(window.clone(), true, now);
        sessions.start(window.clone(), true, now);
        sessions.start(window.clone(), false, now);
        assert_eq!(sessions.own_count(&window), 2);
        sessions.stop(&window);
        assert_eq!(sessions.own_count(&window), 1);
    }

    #[test]
    fn a_session_counts_as_sharing_only_once_it_has_lasted_the_minimum_age() {
        let start = Instant::now();
        let min_age = Duration::from_secs(1);
        let screenshot = target(Kind::Monitor, "DP-2");
        let share = target(Kind::Region, "DP-2");
        let mut sessions = Sessions::default();
        sessions.start(screenshot.clone(), false, start);
        sessions.start(share.clone(), false, start - Duration::from_millis(1500));
        assert_eq!(
            sessions.confirmed_targets(start, min_age, &[]),
            [share.clone()]
        );
        assert_eq!(
            sessions.next_confirmation(start, min_age, &[]),
            Some(min_age)
        );
        let later = start + Duration::from_millis(400);
        assert_eq!(
            sessions.next_confirmation(later, min_age, &[]),
            Some(Duration::from_millis(600))
        );
        let confirmed = start + min_age;
        assert_eq!(
            sessions.confirmed_targets(confirmed, min_age, &[]),
            [screenshot.clone(), share.clone()]
        );
        assert_eq!(sessions.next_confirmation(confirmed, min_age, &[]), None);

        sessions.stop(&screenshot);
        assert_eq!(sessions.confirmed_targets(confirmed, min_age, &[]), [share]);
        assert_eq!(sessions.next_confirmation(confirmed, min_age, &[]), None);
    }

    #[test]
    fn a_session_restarting_under_a_target_already_shown_counts_at_once() {
        let now = Instant::now();
        let min_age = Duration::from_secs(1);
        let monitor = target(Kind::Monitor, "DP-2");
        let mut sessions = Sessions::default();
        sessions.start(monitor.clone(), false, now);
        assert_eq!(sessions.confirmed_targets(now, min_age, &[]), []);
        assert_eq!(
            sessions.confirmed_targets(now, min_age, &[monitor.clone()]),
            [monitor.clone()]
        );
        assert_eq!(
            sessions.next_confirmation(now, min_age, &[monitor.clone()]),
            None
        );
        sessions.stop(&monitor);
        sessions.start(monitor.clone(), false, now + Duration::from_millis(900));
        assert_eq!(
            sessions.confirmed_targets(now + Duration::from_millis(1500), min_age, &[]),
            []
        );
    }
}

use std::time::{Duration, Instant};

pub struct Throttle {
    settle_delay: Duration,
    interval: Duration,
    last_flush: Option<Instant>,
    is_pending: bool,
}

impl Throttle {
    pub fn new(settle_delay: Duration, interval: Duration) -> Self {
        Throttle {
            settle_delay,
            interval,
            last_flush: None,
            is_pending: false,
        }
    }

    pub fn request(&mut self, now: Instant) -> Option<Duration> {
        if self.is_pending {
            return None;
        }
        self.is_pending = true;
        let remaining = self.last_flush.map_or(Duration::ZERO, |last_flush| {
            self.interval
                .saturating_sub(now.saturating_duration_since(last_flush))
        });
        Some(remaining.max(self.settle_delay))
    }

    pub fn finish(&mut self, now: Instant) {
        self.is_pending = false;
        self.last_flush = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTLE: Duration = Duration::from_millis(50);
    const INTERVAL: Duration = Duration::from_millis(250);

    #[test]
    fn a_burst_of_changes_flushes_once() {
        let start = Instant::now();
        let mut throttle = Throttle::new(SETTLE, INTERVAL);
        assert_eq!(throttle.request(start), Some(SETTLE));
        for offset in [1, 10, 40] {
            assert_eq!(
                throttle.request(start + Duration::from_millis(offset)),
                None
            );
        }
        throttle.finish(start + SETTLE);
        assert_eq!(
            throttle.request(start + SETTLE + Duration::from_millis(10)),
            Some(Duration::from_millis(240))
        );
    }

    #[test]
    fn a_change_after_a_quiet_spell_waits_only_to_settle() {
        let start = Instant::now();
        let mut throttle = Throttle::new(SETTLE, INTERVAL);
        throttle.request(start);
        throttle.finish(start);
        assert_eq!(
            throttle.request(start + Duration::from_secs(2)),
            Some(SETTLE)
        );
        throttle.finish(start + Duration::from_secs(2));
        assert_eq!(
            throttle.request(start + Duration::from_millis(2230)),
            Some(SETTLE)
        );
    }
}

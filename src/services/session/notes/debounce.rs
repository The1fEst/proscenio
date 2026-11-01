#[derive(Clone, Copy)]
pub struct Debouncer {
    quiet_millis: u64,
    longest_millis: u64,
}

impl Debouncer {
    pub const fn new(quiet_millis: u64, longest_millis: u64) -> Self {
        Debouncer {
            quiet_millis,
            longest_millis,
        }
    }

    pub fn delay_millis(&self, first_change_millis: i64, now_millis: i64) -> u64 {
        let waited_millis = now_millis.saturating_sub(first_change_millis).max(0) as u64;
        self.quiet_millis
            .min(self.longest_millis.saturating_sub(waited_millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_waits_for_a_quiet_moment_but_never_past_the_longest_wait() {
        let debounce = Debouncer::new(500, 5000);
        let cases = [
            (1000, 1000, 500),
            (1000, 3000, 500),
            (1000, 5600, 400),
            (1000, 6000, 0),
            (1000, 9000, 0),
            (1000, 500, 500),
        ];
        for (first_change, now, delay) in cases {
            assert_eq!(debounce.delay_millis(first_change, now), delay, "{now}");
        }
    }
}

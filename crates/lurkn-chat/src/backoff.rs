use std::time::Duration;

/// Exponential backoff with full jitter, 1s doubling to 30s.
///
/// Full jitter (a uniform pick in `[0, delay]`) rather than plain exponential so
/// that many clients dropped by the same Twitch edge do not return in lockstep.
#[derive(Debug, Clone)]
pub struct Backoff {
    attempt: u32,
    base: Duration,
    cap: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            attempt: 0,
            base: Duration::from_secs(1),
            cap: Duration::from_secs(30),
        }
    }
}

impl Backoff {
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// The un-jittered ceiling for the next attempt.
    pub fn ceiling(&self) -> Duration {
        let shift = self.attempt.min(20);
        self.base.saturating_mul(1u32 << shift).min(self.cap)
    }

    /// Next delay, and advance.
    pub fn next_delay(&mut self) -> Duration {
        let ceiling = self.ceiling();
        self.attempt = self.attempt.saturating_add(1);

        // A jitter source without an rng dependency for one number.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let span = ceiling.as_millis().max(1) as u64;
        Duration::from_millis(nanos % span)
    }

    /// Call after a successful connection so the next failure starts over.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ceiling_doubles_then_stops_at_the_cap() {
        let mut b = Backoff::default();
        let mut seen = Vec::new();
        for _ in 0..8 {
            seen.push(b.ceiling());
            b.next_delay();
        }
        assert_eq!(seen[0], Duration::from_secs(1));
        assert_eq!(seen[1], Duration::from_secs(2));
        assert_eq!(seen[2], Duration::from_secs(4));
        assert_eq!(seen[5], Duration::from_secs(30), "capped");
        assert_eq!(seen[7], Duration::from_secs(30), "stays capped");
    }

    #[test]
    fn jitter_never_exceeds_the_ceiling() {
        let mut b = Backoff::default();
        for _ in 0..50 {
            let ceiling = b.ceiling();
            assert!(b.next_delay() <= ceiling);
        }
    }

    #[test]
    fn resetting_starts_over_after_a_good_connection() {
        let mut b = Backoff::default();
        for _ in 0..5 {
            b.next_delay();
        }
        assert!(b.attempt() > 0);
        b.reset();
        assert_eq!(b.attempt(), 0);
        assert_eq!(b.ceiling(), Duration::from_secs(1));
    }

    #[test]
    fn a_long_outage_does_not_overflow() {
        let mut b = Backoff::default();
        for _ in 0..10_000 {
            b.next_delay();
        }
        assert_eq!(b.ceiling(), Duration::from_secs(30));
    }
}

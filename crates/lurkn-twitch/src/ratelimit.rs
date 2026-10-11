use std::collections::VecDeque;

use time::{Duration, OffsetDateTime};

/// A sliding-window limiter.
///
/// Twitch **silently drops** chat messages sent over the limit, so exceeding it
/// looks like a Lurkn bug to the user. Enforced client-side per
/// `docs/architecture.md` §5.4.
#[derive(Debug)]
pub struct SlidingWindow {
    capacity: usize,
    window: Duration,
    hits: VecDeque<OffsetDateTime>,
}

impl SlidingWindow {
    /// Non-moderator chat limit: 20 messages per 30 seconds per channel.
    pub fn chat_default() -> Self {
        Self::new(20, Duration::seconds(30))
    }

    pub fn new(capacity: usize, window: Duration) -> Self {
        Self {
            capacity,
            window,
            hits: VecDeque::with_capacity(capacity),
        }
    }

    fn evict(&mut self, now: OffsetDateTime) {
        while self.hits.front().is_some_and(|t| now - *t >= self.window) {
            self.hits.pop_front();
        }
    }

    pub fn try_acquire(&mut self, now: OffsetDateTime) -> bool {
        self.evict(now);
        if self.hits.len() < self.capacity {
            self.hits.push_back(now);
            true
        } else {
            false
        }
    }

    /// When the next slot frees up, or `None` if one is free right now.
    pub fn retry_after(&mut self, now: OffsetDateTime) -> Option<Duration> {
        self.evict(now);
        if self.hits.len() < self.capacity {
            return None;
        }
        self.hits
            .front()
            .map(|oldest| self.window - (now - *oldest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const T0: OffsetDateTime = datetime!(2026-09-20 12:00:00 UTC);

    #[test]
    fn allows_up_to_capacity_then_blocks() {
        let mut w = SlidingWindow::chat_default();
        for i in 0..20 {
            assert!(
                w.try_acquire(T0 + Duration::milliseconds(i)),
                "message {i} should pass"
            );
        }
        assert!(
            !w.try_acquire(T0 + Duration::seconds(1)),
            "21st message must be held back"
        );
    }

    #[test]
    fn slot_frees_once_the_window_slides_past() {
        let mut w = SlidingWindow::chat_default();
        for _ in 0..20 {
            assert!(w.try_acquire(T0));
        }
        assert!(!w.try_acquire(T0 + Duration::seconds(29)));
        assert!(w.try_acquire(T0 + Duration::seconds(30)));
    }

    #[test]
    fn retry_after_reports_the_wait() {
        let mut w = SlidingWindow::new(1, Duration::seconds(30));
        assert_eq!(w.retry_after(T0), None);
        assert!(w.try_acquire(T0));
        assert_eq!(
            w.retry_after(T0 + Duration::seconds(10)),
            Some(Duration::seconds(20))
        );
    }
}

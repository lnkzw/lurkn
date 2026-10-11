use std::collections::VecDeque;

use indexmap::IndexSet;
use lurkn_core::MessageId;

/// Default history depth. Twitch redelivers within a short window, so this only
/// has to outlast a reconnect, not the session.
const DEFAULT_CAPACITY: usize = 4096;

/// Bounded seen-set for EventSub `message_id`s.
///
/// EventSub may redeliver a notification, notably across the `session_reconnect`
/// socket swap. Without this, duplicate chat lines appear on a flaky network.
/// See `docs/architecture.md` §5.2.
#[derive(Debug)]
pub struct MessageDedupe {
    capacity: usize,
    seen: IndexSet<MessageId>,
    order: VecDeque<MessageId>,
}

impl Default for MessageDedupe {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl MessageDedupe {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "dedupe capacity must be non-zero");
        Self {
            capacity,
            seen: IndexSet::with_capacity(capacity),
            order: VecDeque::new(),
        }
    }

    /// Returns `true` if this id is new (and records it), `false` if it repeats.
    pub fn accept(&mut self, id: &MessageId) -> bool {
        if self.seen.contains(id) {
            return false;
        }
        if self.order.len() == self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.shift_remove(&oldest);
            }
        }
        self.seen.insert(id.clone());
        self.order.push_back(id.clone());
        true
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> MessageId {
        MessageId(s.to_owned())
    }

    #[test]
    fn first_sighting_accepted_repeat_rejected() {
        let mut d = MessageDedupe::default();
        assert!(d.accept(&id("a")));
        assert!(!d.accept(&id("a")));
        assert!(d.accept(&id("b")));
    }

    #[test]
    fn stays_bounded_and_evicts_oldest_first() {
        let mut d = MessageDedupe::with_capacity(3);
        for m in ["a", "b", "c"] {
            assert!(d.accept(&id(m)));
        }
        assert_eq!(d.len(), 3);

        // "d" evicts "a".
        assert!(d.accept(&id("d")));
        assert_eq!(d.len(), 3, "capacity must not grow");
        assert!(!d.accept(&id("c")), "recent id still remembered");
        assert!(d.accept(&id("a")), "evicted id is forgotten");
    }

    #[test]
    fn survives_a_redelivery_burst_without_growing() {
        let mut d = MessageDedupe::with_capacity(8);
        for _ in 0..1000 {
            d.accept(&id("same"));
        }
        assert_eq!(d.len(), 1);
    }
}

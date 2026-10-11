use time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ttl {
    pub name: &'static str,
    pub duration: Duration,
}

/// Cache lifetimes from `docs/architecture.md` §10. Expiry marks data *stale*,
/// not unusable — stale rows are still served when the network fails.
pub const CACHE_TTLS: &[Ttl] = &[
    Ttl {
        name: "follows",
        duration: Duration::minutes(15),
    },
    Ttl {
        name: "channels",
        duration: Duration::hours(1),
    },
    Ttl {
        name: "emotes_channel",
        duration: Duration::hours(1),
    },
    Ttl {
        name: "emotes_global",
        duration: Duration::hours(24),
    },
    Ttl {
        name: "badges",
        duration: Duration::hours(24),
    },
];

pub fn ttl(name: &str) -> Option<Duration> {
    CACHE_TTLS
        .iter()
        .find(|t| t.name == name)
        .map(|t| t.duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ttls_are_documented_values() {
        assert_eq!(ttl("follows"), Some(Duration::minutes(15)));
        assert_eq!(ttl("emotes_global"), Some(Duration::hours(24)));
        assert_eq!(ttl("unknown"), None);
    }

    #[test]
    fn global_emote_sets_outlive_channel_sets() {
        assert!(ttl("emotes_global").unwrap() > ttl("emotes_channel").unwrap());
    }

    #[test]
    fn every_ttl_is_positive() {
        for t in CACHE_TTLS {
            assert!(
                t.duration > Duration::ZERO,
                "{} has a non-positive TTL",
                t.name
            );
        }
    }
}

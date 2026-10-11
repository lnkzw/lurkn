//! Normative lookup order for third-party emote names.
//!
//! `ProviderId` and `EmoteScope` live in `lurkn-core` because chat tokens carry
//! emotes; this module only owns the ordering.

pub use lurkn_core::emote::{EmoteScope, ProviderId};

/// Approved at review, mirrored in `docs/emote-precedence.md`. Earlier wins.
///
/// Channel-specific always beats global — that is what the broadcaster chose —
/// and 7TV > BTTV > FFZ matches what users expect from existing clients.
pub const PRECEDENCE: &[(ProviderId, EmoteScope)] = &[
    (ProviderId::SevenTv, EmoteScope::Channel),
    (ProviderId::Bttv, EmoteScope::Channel),
    (ProviderId::Ffz, EmoteScope::Channel),
    (ProviderId::SevenTv, EmoteScope::Global),
    (ProviderId::Bttv, EmoteScope::Global),
    (ProviderId::Ffz, EmoteScope::Global),
];

/// Rank of a (provider, scope) pair. Lower wins.
pub fn rank(provider: ProviderId, scope: &EmoteScope) -> Option<usize> {
    PRECEDENCE
        .iter()
        .position(|(p, s)| *p == provider && s == scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_sets_always_beat_global_sets() {
        for provider in [ProviderId::SevenTv, ProviderId::Bttv, ProviderId::Ffz] {
            let channel = rank(provider, &EmoteScope::Channel).unwrap();
            let global = rank(provider, &EmoteScope::Global).unwrap();
            assert!(
                channel < global,
                "{provider:?}: channel must outrank global"
            );
        }
    }

    #[test]
    fn provider_order_is_seventv_then_bttv_then_ffz() {
        for scope in [EmoteScope::Channel, EmoteScope::Global] {
            let seven = rank(ProviderId::SevenTv, &scope).unwrap();
            let bttv = rank(ProviderId::Bttv, &scope).unwrap();
            let ffz = rank(ProviderId::Ffz, &scope).unwrap();
            assert!(seven < bttv && bttv < ffz, "{scope:?}: order violated");
        }
    }

    /// Twitch emotes arrive as authoritative EventSub fragments and are never
    /// name-matched, so they must not appear in the name-resolution table.
    #[test]
    fn twitch_is_not_name_resolved() {
        assert_eq!(rank(ProviderId::Twitch, &EmoteScope::Channel), None);
        assert_eq!(rank(ProviderId::Twitch, &EmoteScope::Global), None);
    }

    #[test]
    fn every_entry_is_unique() {
        let mut seen = PRECEDENCE.to_vec();
        let before = seen.len();
        seen.sort_by_key(|(p, s)| (format!("{p:?}"), format!("{s:?}")));
        seen.dedup();
        assert_eq!(seen.len(), before, "duplicate entry in PRECEDENCE");
    }
}

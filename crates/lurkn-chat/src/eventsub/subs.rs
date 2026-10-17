//! Subscription reconciliation.
//!
//! Under EventSub, joining a channel is an HTTP `POST` per subscription type and
//! leaving is a `DELETE`. See `docs/architecture.md` §5.2.

use std::collections::HashMap;

use lurkn_core::UserId;

/// Without this there is no chat at all, so a failure here is fatal.
pub const CHAT_MESSAGE_TYPE: &str = "channel.chat.message";

pub const CHAT_CLEAR_TYPE: &str = "channel.chat.clear";
pub const CHAT_CLEAR_USER_TYPE: &str = "channel.chat.clear_user_messages";
pub const CHAT_DELETE_TYPE: &str = "channel.chat.message_delete";
/// Note the underscore: Twitch names this `chat_settings`, unlike its siblings.
pub const CHAT_SETTINGS_TYPE: &str = "channel.chat_settings.update";
/// Subs, gifts, raids and announcements. The only route to them for a viewer
/// client: the dedicated topics (`channel.subscribe`, `channel.cheer`,
/// `channel.hype_train.*`) all require the broadcaster's own token, while this
/// one rides the `user:read:chat` scope chat already has.
pub const CHAT_NOTIFICATION_TYPE: &str = "channel.chat.notification";
/// The channel sending its viewers to another one, so Lurkn can go along.
///
/// Public, like a channel's live state: no scope, and a condition that names
/// the raiding channel and nothing else — Twitch has no `user_id` here and
/// refuses a `to_broadcaster_user_id` beside it. Unlike the chat types it
/// costs 1 against the socket's `max_total_cost` of 10, because the channel
/// has not authorised Lurkn; one per joined channel stays far inside that.
pub const CHANNEL_RAID_TYPE: &str = "channel.raid";
pub const SUBSCRIPTION_VERSION: &str = "1";

/// Moderation and room state. Each improves chat but none is required to read
/// it, so a failure is logged and skipped rather than taking the channel down.
pub const OPTIONAL_SUBSCRIPTIONS: &[&str] = &[
    CHAT_CLEAR_TYPE,
    CHAT_CLEAR_USER_TYPE,
    CHAT_DELETE_TYPE,
    CHAT_SETTINGS_TYPE,
    CHAT_NOTIFICATION_TYPE,
    CHANNEL_RAID_TYPE,
];

/// Every subscription a joined channel asks for, all version 1 and all
/// satisfied by the `user:read:chat` scope — the raid by no scope at all.
pub const CHANNEL_SUBSCRIPTIONS: &[&str] = &[
    CHAT_MESSAGE_TYPE,
    CHAT_CLEAR_TYPE,
    CHAT_CLEAR_USER_TYPE,
    CHAT_DELETE_TYPE,
    CHAT_SETTINGS_TYPE,
    CHAT_NOTIFICATION_TYPE,
    CHANNEL_RAID_TYPE,
];

/// Whether losing this subscription costs the user chat itself.
pub fn is_required(subscription_type: &str) -> bool {
    subscription_type == CHAT_MESSAGE_TYPE
}

/// One subscription: a channel and an event type.
pub type SubscriptionKey = (UserId, &'static str);

/// What must change to get from the current subscriptions to the desired ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciliation {
    pub to_create: Vec<SubscriptionKey>,
    /// Subscription ids to delete.
    pub to_delete: Vec<String>,
}

impl Reconciliation {
    pub fn is_empty(&self) -> bool {
        self.to_create.is_empty() && self.to_delete.is_empty()
    }
}

/// Diff current subscriptions against the channels we want.
///
/// `current` maps each (channel, type) to the subscription id Twitch gave us.
pub fn reconcile(current: &HashMap<SubscriptionKey, String>, desired: &[UserId]) -> Reconciliation {
    let wanted: Vec<SubscriptionKey> = desired
        .iter()
        .flat_map(|id| CHANNEL_SUBSCRIPTIONS.iter().map(move |t| (id.clone(), *t)))
        .collect();

    Reconciliation {
        to_create: wanted
            .iter()
            .filter(|key| !current.contains_key(key))
            .cloned()
            .collect(),
        to_delete: current
            .iter()
            .filter(|(key, _)| !wanted.contains(key))
            .map(|(_, sub_id)| sub_id.clone())
            .collect(),
    }
}

/// Whispers sent to the authorising user.
///
/// Not part of [`CHANNEL_SUBSCRIPTIONS`]: it belongs to the account, not to a
/// channel, so it is created once per socket session and left out of the
/// per-channel reconcile, which would otherwise delete it on the first switch.
pub const WHISPER_TYPE: &str = "user.whisper.message";

/// The reader's own stream starting and stopping.
///
/// Needs no scope at all — Twitch treats "is this channel live" as public — so
/// unlike the rest of broadcaster mode it costs nothing and is always on. It is
/// also the honest answer to "am I streaming": a local OBS process or a screen
/// share says whether an app is running, not whether Twitch has a stream.
pub const STREAM_ONLINE_TYPE: &str = "stream.online";
pub const STREAM_OFFLINE_TYPE: &str = "stream.offline";

/// Both halves of the reader's own live state, subscribed together.
pub const OWN_STREAM_SUBSCRIPTIONS: &[&str] = &[STREAM_ONLINE_TYPE, STREAM_OFFLINE_TYPE];

/// The body for a subscription conditioned on one broadcaster and nothing else.
pub fn broadcaster_body(
    subscription_type: &str,
    broadcaster_id: &str,
    session_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": subscription_type,
        "version": SUBSCRIPTION_VERSION,
        "condition": { "broadcaster_user_id": broadcaster_id },
        "transport": { "method": "websocket", "session_id": session_id },
    })
}

/// The body for the whisper subscription, whose condition is the user alone.
pub fn whisper_body(user_id: &str, session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": WHISPER_TYPE,
        "version": SUBSCRIPTION_VERSION,
        "condition": { "user_id": user_id },
        "transport": { "method": "websocket", "session_id": session_id },
    })
}

/// The body for one of a joined channel's subscriptions: the raid's condition
/// is its own, every other type takes [`create_body`]'s.
pub fn channel_body(
    subscription_type: &str,
    broadcaster_id: &str,
    user_id: &str,
    session_id: &str,
) -> serde_json::Value {
    if subscription_type == CHANNEL_RAID_TYPE {
        return serde_json::json!({
            "type": CHANNEL_RAID_TYPE,
            "version": SUBSCRIPTION_VERSION,
            "condition": { "from_broadcaster_user_id": broadcaster_id },
            "transport": { "method": "websocket", "session_id": session_id },
        });
    }
    create_body(subscription_type, broadcaster_id, user_id, session_id)
}

/// The body for `POST helix/eventsub/subscriptions`.
pub fn create_body(
    subscription_type: &str,
    broadcaster_id: &str,
    user_id: &str,
    session_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": subscription_type,
        "version": SUBSCRIPTION_VERSION,
        "condition": {
            "broadcaster_user_id": broadcaster_id,
            // Must be the authorising user, which is what makes the cost 0.
            "user_id": user_id,
        },
        "transport": { "method": "websocket", "session_id": session_id },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whisper_condition_names_only_the_user() {
        let body = whisper_body("42", "S1");
        assert_eq!(body["type"], "user.whisper.message");
        assert_eq!(body["condition"]["user_id"], "42");
        // A broadcaster here would be rejected: whispers are not a channel event.
        assert!(body["condition"].get("broadcaster_user_id").is_none());
        assert_eq!(body["transport"]["session_id"], "S1");
    }

    /// It must never ride the per-channel reconcile, which would delete it.
    #[test]
    fn the_own_stream_condition_names_only_the_broadcaster() {
        let body = broadcaster_body(STREAM_ONLINE_TYPE, "42", "S1");
        assert_eq!(body["type"], "stream.online");
        assert_eq!(body["condition"]["broadcaster_user_id"], "42");
        // A user_id here would be rejected: this one is not a chat subscription.
        assert!(body["condition"].get("user_id").is_none());
    }

    /// The reader's own stream is theirs, not the open channel's, so it must
    /// survive every channel switch — the per-channel reconcile would drop it.
    #[test]
    fn the_own_stream_is_not_a_channel_subscription() {
        for name in OWN_STREAM_SUBSCRIPTIONS {
            assert!(!CHANNEL_SUBSCRIPTIONS.contains(name));
            assert!(!OPTIONAL_SUBSCRIPTIONS.contains(name));
        }
    }

    #[test]
    fn whispers_are_not_a_channel_subscription() {
        assert!(!CHANNEL_SUBSCRIPTIONS.contains(&WHISPER_TYPE));
        assert!(!OPTIONAL_SUBSCRIPTIONS.contains(&WHISPER_TYPE));
    }

    fn id(s: &str) -> UserId {
        UserId(s.to_owned())
    }

    #[test]
    fn joining_a_channel_creates_every_subscription_type() {
        let r = reconcile(&HashMap::new(), &[id("1")]);
        assert_eq!(r.to_create.len(), CHANNEL_SUBSCRIPTIONS.len());
        assert!(r.to_create.iter().all(|(channel, _)| channel == &id("1")));
        assert!(r.to_delete.is_empty());

        let types: Vec<&str> = r.to_create.iter().map(|(_, t)| *t).collect();
        assert!(types.contains(&CHAT_MESSAGE_TYPE));
        assert!(types.contains(&CHAT_SETTINGS_TYPE));
        assert!(types.contains(&CHAT_DELETE_TYPE));
    }

    #[test]
    fn leaving_a_channel_deletes_every_one_of_its_subscriptions() {
        let current: HashMap<SubscriptionKey, String> = CHANNEL_SUBSCRIPTIONS
            .iter()
            .enumerate()
            .map(|(i, t)| ((id("1"), *t), format!("sub-{i}")))
            .collect();

        let r = reconcile(&current, &[]);
        assert!(r.to_create.is_empty());
        assert_eq!(r.to_delete.len(), CHANNEL_SUBSCRIPTIONS.len());
    }

    #[test]
    fn an_unchanged_set_needs_no_work() {
        let current: HashMap<SubscriptionKey, String> = CHANNEL_SUBSCRIPTIONS
            .iter()
            .map(|t| ((id("1"), *t), format!("sub-{t}")))
            .collect();
        assert!(reconcile(&current, &[id("1")]).is_empty());
        assert!(reconcile(&HashMap::new(), &[]).is_empty());
    }

    /// Switching channels replaces the whole set, not just the message feed.
    #[test]
    fn switching_channels_swaps_every_subscription() {
        let current: HashMap<SubscriptionKey, String> = CHANNEL_SUBSCRIPTIONS
            .iter()
            .map(|t| ((id("1"), *t), format!("sub-{t}")))
            .collect();

        let r = reconcile(&current, &[id("2")]);
        assert_eq!(r.to_create.len(), CHANNEL_SUBSCRIPTIONS.len());
        assert_eq!(r.to_delete.len(), CHANNEL_SUBSCRIPTIONS.len());
        assert!(r.to_create.iter().all(|(channel, _)| channel == &id("2")));
    }

    /// A partially-subscribed channel only fills the gaps, e.g. after one POST
    /// failed and the rest succeeded.
    #[test]
    fn only_the_missing_subscriptions_are_created() {
        let mut current = HashMap::new();
        current.insert((id("1"), CHAT_MESSAGE_TYPE), "sub-msg".to_owned());

        let r = reconcile(&current, &[id("1")]);
        assert_eq!(r.to_create.len(), CHANNEL_SUBSCRIPTIONS.len() - 1);
        assert!(!r.to_create.iter().any(|(_, t)| *t == CHAT_MESSAGE_TYPE));
        assert!(r.to_delete.is_empty());
    }

    /// Only the message feed is load-bearing. A typo or outage in any of the
    /// others must not cost the user chat — which is exactly what happened when
    /// `channel.chat_settings.update` was first spelt with a dot.
    #[test]
    fn only_the_message_subscription_is_required() {
        assert!(is_required(CHAT_MESSAGE_TYPE));
        for optional in OPTIONAL_SUBSCRIPTIONS {
            assert!(!is_required(optional), "{optional} should be optional");
        }
    }

    #[test]
    fn the_optional_and_required_types_together_are_the_full_set() {
        let mut combined = vec![CHAT_MESSAGE_TYPE];
        combined.extend_from_slice(OPTIONAL_SUBSCRIPTIONS);
        combined.sort_unstable();

        let mut all = CHANNEL_SUBSCRIPTIONS.to_vec();
        all.sort_unstable();
        assert_eq!(combined, all);
    }

    /// Twitch spells this one with an underscore, unlike every sibling.
    #[test]
    fn the_settings_type_uses_twitchs_underscore_spelling() {
        assert_eq!(CHAT_SETTINGS_TYPE, "channel.chat_settings.update");
        assert_ne!(CHAT_SETTINGS_TYPE, "channel.chat.settings.update");
    }

    #[test]
    fn the_create_body_matches_twitchs_schema() {
        let body = create_body(CHAT_MESSAGE_TYPE, "11111", "22222", "SESSION");
        assert_eq!(body["type"], CHAT_MESSAGE_TYPE);
        assert_eq!(body["version"], "1");
        assert_eq!(body["condition"]["broadcaster_user_id"], "11111");
        assert_eq!(body["condition"]["user_id"], "22222");
        assert_eq!(body["transport"]["method"], "websocket");
        assert_eq!(body["transport"]["session_id"], "SESSION");
    }

    /// A raid is followed from the raiding side, and Twitch takes exactly one
    /// of `from_` and `to_broadcaster_user_id`, with no user.
    #[test]
    fn the_raid_condition_names_only_the_raiding_channel() {
        let body = channel_body(CHANNEL_RAID_TYPE, "11111", "22222", "S");
        assert_eq!(body["type"], "channel.raid");
        assert_eq!(body["version"], "1");
        assert_eq!(body["condition"]["from_broadcaster_user_id"], "11111");
        let condition = body["condition"].as_object().expect("a condition");
        assert_eq!(condition.len(), 1, "{condition:?}");
        assert_eq!(body["transport"]["session_id"], "S");
    }

    #[test]
    fn every_other_channel_type_keeps_the_chat_condition() {
        for name in CHANNEL_SUBSCRIPTIONS {
            if *name == CHANNEL_RAID_TYPE {
                continue;
            }
            assert_eq!(
                channel_body(name, "11111", "22222", "S"),
                create_body(name, "11111", "22222", "S")
            );
        }
    }

    /// The `user_id` condition must be the authorising user — that is what makes
    /// the subscription cost 0 and keeps us clear of the max_total_cost ceiling.
    #[test]
    fn the_condition_user_is_the_viewer_not_the_broadcaster() {
        let body = create_body(CHAT_SETTINGS_TYPE, "11111", "22222", "S");
        assert_ne!(
            body["condition"]["user_id"],
            body["condition"]["broadcaster_user_id"]
        );
    }

    /// A handful of subscriptions per channel against a 300-per-socket ceiling.
    #[test]
    fn the_per_channel_cost_stays_well_inside_the_socket_limit() {
        assert!(CHANNEL_SUBSCRIPTIONS.len() * 10 < crate::MAX_SUBSCRIPTIONS_PER_SOCKET);
    }
}

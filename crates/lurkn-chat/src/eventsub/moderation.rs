//! Moderation and room-state events.
//!
//! These are what IRC used to deliver as `CLEARCHAT`, `CLEARMSG` and
//! `ROOMSTATE`; under EventSub each is its own subscription
//! (`docs/architecture.md` §5.3).

use lurkn_core::RoomState;
use serde::Deserialize;

use crate::eventsub::subs;
use crate::transport::ChatEvent;

#[derive(Debug, Deserialize)]
struct ClearEvent {
    broadcaster_user_id: String,
}

#[derive(Debug, Deserialize)]
struct ClearUserEvent {
    broadcaster_user_id: String,
    target_user_id: String,
    #[serde(default)]
    target_user_login: String,
}

#[derive(Debug, Deserialize)]
struct DeleteEvent {
    broadcaster_user_id: String,
    message_id: String,
    #[serde(default)]
    target_user_login: String,
}

#[derive(Debug, Deserialize)]
struct SettingsEvent {
    broadcaster_user_id: String,
    #[serde(default)]
    emote_mode: bool,
    #[serde(default)]
    follower_mode: bool,
    #[serde(default)]
    follower_mode_duration_minutes: Option<u32>,
    #[serde(default)]
    slow_mode: bool,
    #[serde(default)]
    slow_mode_wait_time_seconds: Option<u32>,
    #[serde(default)]
    subscriber_mode: bool,
    #[serde(default)]
    unique_chat_mode: bool,
}

impl SettingsEvent {
    fn to_room_state(&self) -> RoomState {
        RoomState {
            emote_only: self.emote_mode,
            subscriber_only: self.subscriber_mode,
            unique_only: self.unique_chat_mode,
            // Twitch sends the duration only while the mode is on.
            follower_only_minutes: self
                .follower_mode
                .then(|| self.follower_mode_duration_minutes.unwrap_or(0)),
            slow_mode_seconds: self
                .slow_mode
                .then(|| self.slow_mode_wait_time_seconds.unwrap_or(30)),
        }
    }
}

/// Map a non-message notification onto a [`ChatEvent`].
///
/// Returns `None` for a type this build does not handle, which is not an error:
/// Twitch may deliver subscriptions created by an older version.
pub fn to_event(subscription_type: &str, payload: &serde_json::Value) -> Option<ChatEvent> {
    match subscription_type {
        subs::CHAT_CLEAR_TYPE => {
            let event: ClearEvent = serde_json::from_value(payload.clone()).ok()?;
            Some(ChatEvent::Cleared {
                channel_id: event.broadcaster_user_id,
            })
        }

        subs::CHAT_CLEAR_USER_TYPE => {
            let event: ClearUserEvent = serde_json::from_value(payload.clone()).ok()?;
            Some(ChatEvent::UserCleared {
                channel_id: event.broadcaster_user_id,
                user_id: event.target_user_id,
                user_login: event.target_user_login,
            })
        }

        subs::CHAT_DELETE_TYPE => {
            let event: DeleteEvent = serde_json::from_value(payload.clone()).ok()?;
            Some(ChatEvent::Deleted {
                channel_id: event.broadcaster_user_id,
                message_id: event.message_id,
                user_login: event.target_user_login,
            })
        }

        subs::CHAT_SETTINGS_TYPE => {
            let event: SettingsEvent = serde_json::from_value(payload.clone()).ok()?;
            Some(ChatEvent::RoomStateChanged {
                channel_id: event.broadcaster_user_id.clone(),
                state: event.to_room_state(),
            })
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(raw: &str) -> serde_json::Value {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn a_full_clear_is_reported() {
        let event = to_event(
            subs::CHAT_CLEAR_TYPE,
            &json(r#"{"broadcaster_user_id":"11111","broadcaster_user_login":"forsen"}"#),
        )
        .unwrap();
        assert!(matches!(event, ChatEvent::Cleared { channel_id } if channel_id == "11111"));
    }

    #[test]
    fn a_timeout_clears_one_users_messages() {
        let event = to_event(
            subs::CHAT_CLEAR_USER_TYPE,
            &json(
                r#"{"broadcaster_user_id":"11111","target_user_id":"22222",
                    "target_user_login":"spammer"}"#,
            ),
        )
        .unwrap();
        match event {
            ChatEvent::UserCleared {
                user_id,
                user_login,
                ..
            } => {
                assert_eq!(user_id, "22222");
                assert_eq!(user_login, "spammer");
            }
            other => panic!("expected UserCleared, got {other:?}"),
        }
    }

    #[test]
    fn a_single_deletion_names_the_message() {
        let event = to_event(
            subs::CHAT_DELETE_TYPE,
            &json(
                r#"{"broadcaster_user_id":"11111","target_user_id":"2",
                    "target_user_login":"someone","message_id":"abc-123"}"#,
            ),
        )
        .unwrap();
        match event {
            ChatEvent::Deleted {
                message_id,
                user_login,
                ..
            } => {
                assert_eq!(message_id, "abc-123");
                assert_eq!(user_login, "someone");
            }
            other => panic!("expected Deleted, got {other:?}"),
        }
    }

    #[test]
    fn room_settings_map_onto_room_state() {
        let event = to_event(
            subs::CHAT_SETTINGS_TYPE,
            &json(
                r#"{"broadcaster_user_id":"11111","emote_mode":true,
                    "follower_mode":true,"follower_mode_duration_minutes":30,
                    "slow_mode":true,"slow_mode_wait_time_seconds":10,
                    "subscriber_mode":false,"unique_chat_mode":true}"#,
            ),
        )
        .unwrap();

        match event {
            ChatEvent::RoomStateChanged { state, .. } => {
                assert!(state.emote_only);
                assert!(!state.subscriber_only);
                assert!(state.unique_only);
                assert_eq!(state.follower_only_minutes, Some(30));
                assert_eq!(state.slow_mode_seconds, Some(10));
            }
            other => panic!("expected RoomStateChanged, got {other:?}"),
        }
    }

    /// Twitch omits the duration when the mode is off; it must not read as "on".
    #[test]
    fn a_mode_that_is_off_has_no_duration() {
        let event = to_event(
            subs::CHAT_SETTINGS_TYPE,
            &json(
                r#"{"broadcaster_user_id":"1","follower_mode":false,
                    "follower_mode_duration_minutes":null,"slow_mode":false}"#,
            ),
        )
        .unwrap();
        match event {
            ChatEvent::RoomStateChanged { state, .. } => {
                assert_eq!(state.follower_only_minutes, None);
                assert_eq!(state.slow_mode_seconds, None);
                assert!(state.is_unrestricted());
            }
            other => panic!("expected RoomStateChanged, got {other:?}"),
        }
    }

    /// Followers-only with no duration means "any follower", not "off".
    #[test]
    fn follower_mode_without_a_duration_means_any_follower() {
        let event = to_event(
            subs::CHAT_SETTINGS_TYPE,
            &json(r#"{"broadcaster_user_id":"1","follower_mode":true}"#),
        )
        .unwrap();
        match event {
            ChatEvent::RoomStateChanged { state, .. } => {
                assert_eq!(state.follower_only_minutes, Some(0));
                assert_eq!(state.labels(), ["Followers only"]);
            }
            other => panic!("expected RoomStateChanged, got {other:?}"),
        }
    }

    #[test]
    fn an_unhandled_type_is_ignored_rather_than_failing() {
        assert!(to_event("channel.chat.notification", &json("{}")).is_none());
        assert!(to_event("channel.follow", &json("{}")).is_none());
    }

    #[test]
    fn an_unreadable_payload_is_ignored_rather_than_panicking() {
        assert!(to_event(subs::CHAT_DELETE_TYPE, &json(r#"{"nothing":"useful"}"#)).is_none());
    }
}

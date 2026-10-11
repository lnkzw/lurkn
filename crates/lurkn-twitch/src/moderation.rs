//! Acting as a moderator: timeouts, bans, deletions and room settings.
//!
//! Every call carries `moderator_id` as well as `broadcaster_id`, and Twitch
//! requires it to be the user the token belongs to. Every one also needs the
//! caller to actually hold the moderator role in that channel, which is why
//! [`ModerationService::moderated_channels`] exists: offering a timeout button
//! where it would fail is worse than not offering it.
//!
//! These are destructive and public. The wording of a failure matters as much
//! as the call — "you are not a moderator here" and "that user is already
//! banned" are different problems with different answers.

use lurkn_core::UserId;

use crate::client::HelixClient;
use crate::endpoints;
use crate::error::{HelixError, Result};

#[derive(Debug, Clone)]
pub struct ModerationService {
    client: HelixClient,
    /// The acting moderator: the user the token belongs to.
    moderator_id: String,
}

/// A channel the reader may moderate.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModeratedChannelDto {
    pub broadcaster_id: String,
    pub broadcaster_login: String,
    pub broadcaster_name: String,
}

/// What to change about a room. `None` leaves a setting alone.
///
/// Every field optional because Twitch treats the request as a patch, and
/// sending a full state would clobber whatever another moderator just set.
#[derive(Debug, Clone, Default)]
pub struct RoomChange {
    pub emote_only: Option<bool>,
    pub subscriber_only: Option<bool>,
    pub unique_only: Option<bool>,
    pub follower_only: Option<bool>,
    pub follower_minutes: Option<u32>,
    pub slow: Option<bool>,
    pub slow_seconds: Option<u32>,
}

impl ModerationService {
    pub fn new(client: HelixClient, moderator_id: String) -> Self {
        Self {
            client,
            moderator_id,
        }
    }

    /// The channels this user moderates, so the UI knows where to offer itself.
    pub async fn moderated_channels(&self) -> Result<Vec<ModeratedChannelDto>> {
        // More than anyone moderates in practice; the list is not paged through.
        const LIMIT: u32 = 100;

        let page: crate::client::Page<ModeratedChannelDto> = self
            .client
            .get(&endpoints::moderated_channels(&self.moderator_id, LIMIT))
            .await?;

        Ok(page.data)
    }

    /// Time a user out, or ban them when `seconds` is `None`.
    ///
    /// One call for both because Twitch makes them one call: the only
    /// difference is whether a duration is sent.
    pub async fn ban(
        &self,
        broadcaster: &UserId,
        user: &UserId,
        seconds: Option<u32>,
        reason: Option<&str>,
    ) -> Result<()> {
        let mut data = serde_json::json!({ "user_id": user.as_str() });
        if let Some(seconds) = seconds {
            data["duration"] = serde_json::json!(seconds);
        }
        if let Some(reason) = reason.filter(|r| !r.trim().is_empty()) {
            data["reason"] = serde_json::json!(reason);
        }

        self.client
            .post_empty(
                &endpoints::bans(broadcaster.as_str(), &self.moderator_id),
                &serde_json::json!({ "data": data }),
            )
            .await
    }

    pub async fn unban(&self, broadcaster: &UserId, user: &UserId) -> Result<()> {
        self.client
            .delete(&endpoints::unban(
                broadcaster.as_str(),
                &self.moderator_id,
                user.as_str(),
            ))
            .await
    }

    /// Delete one message, or clear the room when `message` is `None`.
    pub async fn delete_message(&self, broadcaster: &UserId, message: Option<&str>) -> Result<()> {
        self.client
            .delete(&endpoints::delete_chat(
                broadcaster.as_str(),
                &self.moderator_id,
                message,
            ))
            .await
    }

    /// Change the room's settings, leaving alone whatever is not named.
    pub async fn update_room(&self, broadcaster: &UserId, change: &RoomChange) -> Result<()> {
        let mut body = serde_json::Map::new();
        let mut set = |key: &str, value: serde_json::Value| {
            body.insert(key.to_owned(), value);
        };

        if let Some(on) = change.emote_only {
            set("emote_mode", on.into());
        }
        if let Some(on) = change.subscriber_only {
            set("subscriber_mode", on.into());
        }
        if let Some(on) = change.unique_only {
            set("unique_chat_mode", on.into());
        }
        if let Some(on) = change.follower_only {
            set("follower_mode", on.into());
        }
        if let Some(minutes) = change.follower_minutes {
            set("follower_mode_duration", minutes.into());
        }
        if let Some(on) = change.slow {
            set("slow_mode", on.into());
        }
        if let Some(seconds) = change.slow_seconds {
            set("slow_mode_wait_time", seconds.into());
        }

        if body.is_empty() {
            // Twitch rejects an empty patch, and there is nothing to ask for.
            return Ok(());
        }

        self.client
            .patch_empty(
                &endpoints::update_chat_settings(broadcaster.as_str(), &self.moderator_id),
                &serde_json::Value::Object(body),
            )
            .await
    }

    /// Post an announcement — the coloured line `/announce` produces.
    pub async fn announce(
        &self,
        broadcaster: &UserId,
        message: &str,
        color: Option<&str>,
    ) -> Result<()> {
        let mut body = serde_json::json!({ "message": message });
        if let Some(color) = color {
            body["color"] = serde_json::json!(color);
        }

        self.client
            .post_empty(
                &endpoints::announcements(broadcaster.as_str(), &self.moderator_id),
                &body,
            )
            .await
    }
}

/// What to tell someone when a moderation call fails.
///
/// These are the failures a moderator actually hits, and they are not the same
/// problem: a 401 is a permission never granted, a 403 is a role they do not
/// hold in this channel, and a 400 is usually the action already being true.
/// Twitch's own message is carried through where it has one, because it
/// distinguishes cases a status code cannot.
pub fn describe(error: &HelixError, action: &str) -> String {
    match error {
        HelixError::Unauthorized => {
            "Lurkn is not allowed to moderate for this account. Grant the permission again."
                .to_owned()
        }
        HelixError::RateLimited { retry_after_secs } => {
            format!("Twitch is rate-limiting moderation. Try again in {retry_after_secs}s.")
        }
        HelixError::Twitch { status: 403, .. } => {
            "You are not a moderator in this channel.".to_owned()
        }
        HelixError::Twitch { status, message } => {
            format!("Could not {action}. Twitch said: {status} {message}")
        }
        HelixError::Transport(_) => "Could not reach Twitch. Check your connection.".to_owned(),
        other => format!("Could not {action}: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_role_is_not_a_missing_permission() {
        let forbidden = HelixError::Twitch {
            status: 403,
            message: "user is not a moderator".into(),
        };
        assert_eq!(
            describe(&forbidden, "ban"),
            "You are not a moderator in this channel."
        );

        // A scope never granted is the user's to fix, and says how.
        assert!(describe(&HelixError::Unauthorized, "ban").contains("Grant the permission"));
    }

    /// Twitch's own wording distinguishes cases the status code cannot, such as
    /// a user who is already banned.
    #[test]
    fn twitchs_own_message_is_carried_through() {
        let bad = HelixError::Twitch {
            status: 400,
            message: "user is already banned".into(),
        };
        let said = describe(&bad, "ban that user");
        assert!(said.contains("already banned"), "{said}");
        assert!(said.contains("ban that user"), "{said}");
    }

    #[test]
    fn an_empty_room_change_asks_for_nothing() {
        let change = RoomChange::default();
        assert!(change.slow.is_none() && change.emote_only.is_none());
    }
}

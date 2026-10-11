//! Moderation commands: timeouts, bans, deletions and room settings.
//!
//! Every one is destructive and public, so each returns a message the UI can
//! show rather than a silent failure. The permission is opt-in, granted as one
//! bundle — a moderator who wants the timeout button wants the delete button
//! too, and splitting them would mean a consent screen per action.

use lurkn_core::UserId;
use lurkn_twitch::moderation::{describe, ModerationService, RoomChange};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::state::AppState;

/// A channel the reader may moderate.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModeratedChannel {
    pub id: String,
    pub login: String,
    /// UNTRUSTED.
    pub name: String,
}

/// Build a service, or say why there is none.
async fn service(state: &AppState) -> Result<ModerationService, String> {
    let viewer_id = state
        .current_user_id()
        .await
        .ok_or_else(|| "Log in to moderate.".to_owned())?;

    if !state.has_moderation().await {
        return Err("Lurkn has not been given permission to moderate.".to_owned());
    }

    Ok(ModerationService::new(state.helix(), viewer_id))
}

/// The channels this account moderates.
///
/// Fetched once per session and cached: the list changes when someone is modded
/// or unmodded, which is rare, and the answer decides whether moderation
/// controls appear at all.
#[tauri::command]
#[specta::specta]
pub async fn moderated_channels(
    state: State<'_, AppState>,
) -> Result<Vec<ModeratedChannel>, String> {
    if let Some(cached) = state.moderated.read().await.as_ref() {
        return Ok(cached.clone());
    }

    let channels = service(&state)
        .await?
        .moderated_channels()
        .await
        .map_err(|error| describe(&error, "read your moderated channels"))?
        .into_iter()
        .map(|c| ModeratedChannel {
            id: c.broadcaster_id,
            login: c.broadcaster_login,
            name: c.broadcaster_name,
        })
        .collect::<Vec<_>>();

    *state.moderated.write().await = Some(channels.clone());
    Ok(channels)
}

/// Time a user out, or ban them when `seconds` is absent.
#[tauri::command]
#[specta::specta]
pub async fn moderate_ban(
    state: State<'_, AppState>,
    broadcaster_id: String,
    user_id: String,
    seconds: Option<u32>,
    reason: Option<String>,
) -> Result<(), String> {
    let what = if seconds.is_some() {
        "time that user out"
    } else {
        "ban that user"
    };

    service(&state)
        .await?
        .ban(
            &UserId(broadcaster_id),
            &UserId(user_id),
            seconds,
            reason.as_deref(),
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, "a moderation action failed");
            describe(&error, what)
        })
}

#[tauri::command]
#[specta::specta]
pub async fn moderate_unban(
    state: State<'_, AppState>,
    broadcaster_id: String,
    user_id: String,
) -> Result<(), String> {
    service(&state)
        .await?
        .unban(&UserId(broadcaster_id), &UserId(user_id))
        .await
        .map_err(|error| describe(&error, "lift that ban"))
}

/// Delete one message, or clear the room when `message_id` is absent.
#[tauri::command]
#[specta::specta]
pub async fn moderate_delete(
    state: State<'_, AppState>,
    broadcaster_id: String,
    message_id: Option<String>,
) -> Result<(), String> {
    let what = if message_id.is_some() {
        "delete that message"
    } else {
        "clear the chat"
    };

    service(&state)
        .await?
        .delete_message(&UserId(broadcaster_id), message_id.as_deref())
        .await
        .map_err(|error| describe(&error, what))
}

/// Change the room's settings, leaving alone whatever is not named.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn moderate_room(
    state: State<'_, AppState>,
    broadcaster_id: String,
    emote_only: Option<bool>,
    subscriber_only: Option<bool>,
    unique_only: Option<bool>,
    follower_only: Option<bool>,
    follower_minutes: Option<u32>,
    slow: Option<bool>,
    slow_seconds: Option<u32>,
) -> Result<(), String> {
    let change = RoomChange {
        emote_only,
        subscriber_only,
        unique_only,
        follower_only,
        follower_minutes,
        slow,
        slow_seconds,
    };

    service(&state)
        .await?
        .update_room(&UserId(broadcaster_id), &change)
        .await
        .map_err(|error| describe(&error, "change the chat settings"))
}

/// Post an announcement — the coloured line `/announce` produces.
#[tauri::command]
#[specta::specta]
pub async fn moderate_announce(
    state: State<'_, AppState>,
    broadcaster_id: String,
    message: String,
) -> Result<(), String> {
    let message = message.trim().to_owned();
    if message.is_empty() {
        return Err("Nothing to announce.".to_owned());
    }

    service(&state)
        .await?
        .announce(&UserId(broadcaster_id), &message, None)
        .await
        .map_err(|error| describe(&error, "post that announcement"))
}

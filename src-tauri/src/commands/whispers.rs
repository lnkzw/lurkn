//! Whisper commands. See `lurkn_core::whisper` and `docs/architecture.md` §5.2.
//!
//! Receiving, storing, showing and sending. Sending is gated twice by Twitch:
//! the account must have a verified phone number, and there is a cap of 40
//! unique recipients a day that cannot be raised.

use lurkn_chat::api::ChatApi;
use lurkn_chat::transport::ChatError;
use tauri::State;

use crate::state::AppState;

/// Twitch's own cap for a first message to someone; later ones may be longer,
/// but a single limit is one fewer surprise than two.
const MAX_WHISPER_LENGTH: usize = 500;

/// Every conversation, most recently active first.
#[tauri::command]
#[specta::specta]
pub async fn whisper_threads(
    state: State<'_, AppState>,
) -> Result<Vec<lurkn_core::WhisperThread>, String> {
    state.store.whisper_threads().map_err(|error| {
        tracing::error!(%error, "could not read the whisper threads");
        "Could not read your whispers.".to_owned()
    })
}

/// One conversation, oldest first.
#[tauri::command]
#[specta::specta]
pub async fn whisper_thread(
    state: State<'_, AppState>,
    peer_id: String,
) -> Result<Vec<lurkn_core::Whisper>, String> {
    // Enough to scroll back through a real conversation without turning the
    // panel into a second chat log.
    const LIMIT: u32 = 200;

    state
        .store
        .whisper_thread(&peer_id, LIMIT)
        .map_err(|error| {
            tracing::error!(%error, "could not read a whisper thread");
            "Could not read that conversation.".to_owned()
        })
}

/// Mark a conversation read, which is what opening it means. Returns the new
/// unread total so the badge does not need a second call.
#[tauri::command]
#[specta::specta]
pub async fn whispers_mark_read(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    peer_id: String,
) -> Result<u32, String> {
    state.store.mark_whispers_read(&peer_id).map_err(|error| {
        tracing::error!(%error, "could not mark a whisper thread read");
        "Could not update that conversation.".to_owned()
    })?;

    let unread = state.store.unread_whispers().unwrap_or(0);
    crate::tray::refresh(&app).await;
    Ok(unread)
}

/// How many received whispers are unread, for the badge at startup.
#[tauri::command]
#[specta::specta]
pub async fn whispers_unread(state: State<'_, AppState>) -> Result<u32, ()> {
    Ok(state.store.unread_whispers().unwrap_or(0))
}

/// Send a whisper, and record our own copy of it.
///
/// Twitch never notifies the sender, so the outgoing half of a conversation
/// exists only because this writes it down. The id is made here for the same
/// reason: a successful send answers 204 with no body, so there is none to take.
#[tauri::command]
#[specta::specta]
pub async fn whisper_send(
    state: State<'_, AppState>,
    peer_id: String,
    peer_login: String,
    peer_name: String,
    text: String,
) -> Result<lurkn_core::Whisper, String> {
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("Nothing to send.".to_owned());
    }
    if text.chars().count() > MAX_WHISPER_LENGTH {
        return Err(format!(
            "Whispers are limited to {MAX_WHISPER_LENGTH} characters."
        ));
    }

    let login = lurkn_core::Login::parse(&peer_login)
        .map_err(|_| "That account name is not usable.".to_owned())?;

    let viewer_id = state
        .current_user_id()
        .await
        .ok_or_else(|| "Log in to send a whisper.".to_owned())?;

    state
        .chat_api(viewer_id)
        .send_whisper(&peer_id, &text)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "could not send a whisper");
            whisper_message(&error)
        })?;

    let sent = lurkn_core::Whisper {
        // Distinct from a Twitch whisper id, which is a plain string: the prefix
        // keeps a locally made id from ever colliding with a delivered one.
        id: format!(
            "sent:{}:{}",
            peer_id,
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ),
        peer_id: lurkn_core::UserId(peer_id),
        peer_login: login,
        peer_name,
        outgoing: true,
        body: text,
        sent_at: time::OffsetDateTime::now_utc(),
        // Your own message is never unread.
        read: true,
    };

    if let Err(error) = state.store.save_whisper(&sent) {
        // The whisper did reach Twitch, so this is not a failed send — but the
        // local record is the only one there is, and losing it silently would
        // make the thread lie.
        tracing::error!(%error, "sent a whisper but could not record it");
    }

    Ok(sent)
}

/// What to tell the user when a whisper does not go out.
///
/// Every cause here is something they can act on, and they are not the same
/// thing, so the message says which one it was. Twitch's own wording is kept
/// where it has any: its 400s distinguish a blocked recipient from a daily cap
/// better than a status code can.
fn whisper_message(error: &ChatError) -> String {
    match error {
        ChatError::Unauthorized => {
            "Lurkn may not send whispers for this account. Grant the permission again.".to_owned()
        }
        ChatError::RateLimited(_) => {
            "Twitch is rate-limiting your whispers. You may whisper 40 new people a day.".to_owned()
        }
        ChatError::Rejected(detail) if detail.contains("403") => {
            "Twitch refused: sending whispers needs a verified phone number on your account."
                .to_owned()
        }
        ChatError::Rejected(detail) => format!("Twitch refused the whisper. {detail}"),
        ChatError::Transport(_) => "Could not reach Twitch. Check your connection.".to_owned(),
        _ => "Could not send that whisper.".to_owned(),
    }
}

/// Who to whisper, found by the name you know them by.
///
/// Needed because every other way into a conversation starts from someone who
/// has spoken in an open channel; this is the way to reach anyone else.
#[tauri::command]
#[specta::specta]
pub async fn whisper_find_user(
    state: State<'_, AppState>,
    login: String,
) -> Result<lurkn_core::User, String> {
    // Twitch logins are lower case; accepting the display-name spelling and
    // folding it here saves a lookup that would fail for no good reason.
    let trimmed = login.trim().trim_start_matches('@').to_lowercase();

    let parsed = lurkn_core::Login::parse(&trimmed)
        .map_err(|_| "That is not a Twitch account name.".to_owned())?;

    let service = state
        .follows_service()
        .await
        .ok_or_else(|| "Log in to look up an account.".to_owned())?;

    let found = service.user_by_login(&parsed).await.map_err(|error| {
        tracing::warn!(%error, "could not look up an account");
        "Twitch did not answer. Try again in a moment.".to_owned()
    })?;

    found.ok_or_else(|| format!("Twitch has no account called {trimmed}."))
}

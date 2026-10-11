//! Backfilling a channel's recent chat.
//!
//! Twitch publishes no chat history, so the only source is
//! `recent-messages.robotty.de`, a community service. Opting in tells
//! that service which channels are being opened, which is why it is off by
//! default and why the panel says where the messages came from.
//!
//! Failure is never surfaced as an error: the backfill is a convenience on top
//! of a chat that works without it, and a third-party outage must not look like
//! Lurkn being broken.

use lurkn_core::Login;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::state::AppState;

/// How many messages to ask for. Enough to see what a conversation was about,
/// well short of the 500-line buffer the live log keeps.
const LIMIT: u32 = 80;

/// Fetch and publish the backfill for a channel, if the user asked for it.
pub async fn load(app: &AppHandle, login: &Login, channel_id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if !state.settings.read().await.chat.history_on_join {
        return;
    }

    let url = format!(
        "https://recent-messages.robotty.de/api/v2/recent-messages/{}?limit={LIMIT}",
        login.as_str()
    );

    let response = match state.http().get(&url).send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(%error, "could not reach the chat history service");
            return;
        }
    };

    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), "the chat history service declined");
        return;
    }

    let body: lurkn_chat::history::RecentMessages = match response.json().await {
        Ok(body) => body,
        Err(error) => {
            tracing::debug!(%error, "could not read the chat history answer");
            return;
        }
    };

    if let Some(error) = &body.error {
        tracing::debug!(error, "the chat history service reported an error");
        return;
    }

    let parsed = lurkn_chat::history::parse_batch(&body.messages, channel_id);

    // The same ignores and highlights as live chat, minus the notifications:
    // backfill that shows an ignored user would undo the ignore on every join.
    let mut messages = Vec::with_capacity(parsed.len());
    for mut message in parsed {
        if crate::tasks::chat::screen(app, &mut message, false).await {
            messages.push(message);
        }
    }
    if messages.is_empty() {
        return;
    }

    // The same resolution live messages get, so backfill does not render as a
    // plainer copy of the real thing.
    crate::tasks::chat::resolve(app, &mut messages).await;

    // A channel switch during the fetch must not backfill the wrong log.
    let still_current = state
        .current_channel
        .read()
        .await
        .as_ref()
        .is_some_and(|(_, id)| id.as_str() == channel_id);
    if !still_current {
        tracing::debug!("discarding history for a channel we left");
        return;
    }

    tracing::debug!(
        count = messages.len(),
        channel = login.as_str(),
        "backfilled chat"
    );
    crate::watch::history(app, &messages).await;

    if let Err(error) = (crate::events::ChatHistory {
        channel_id: channel_id.to_owned(),
        messages,
    })
    .emit(app)
    {
        tracing::warn!(%error, "could not publish the chat history");
    }
}

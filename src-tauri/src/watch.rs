//! The synced window's chat: what reaches it, and the font steps it sends
//! back. The server and the page live in `lurkn-watch`.
//!
//! The column shows what Lurkn's own log shows — the same ignores, highlights,
//! emotes and badges, decided once in the chat pump — so nothing here filters
//! or styles anything. It only forwards, and only while a window server runs.

use lurkn_core::settings::{MAX_FONT_SIZE, MIN_FONT_SIZE};
use lurkn_core::ChatMessage;
use lurkn_watch::{Step, WatchMessage, WatchServer};
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast;

use crate::state::AppState;

/// The server, if a synced window has ever been opened this session.
pub async fn running(state: &AppState) -> Option<WatchServer> {
    state.watch.read().await.clone()
}

/// Bring a server that has just started up to date: the look, and the
/// channel chat is on. Its replay starts empty — Rust keeps no log of its own
/// — and fills from the next line on.
pub async fn prime(app: &AppHandle, server: &WatchServer) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Some(look) = state.watch_look.read().await.clone() {
        server.set_look(look).await;
    }
    let channel = state
        .current_channel
        .read()
        .await
        .as_ref()
        .map(|(login, _)| login.clone());
    server.set_chat_channel(channel.as_ref()).await;

    tauri::async_runtime::spawn(font_steps(app.clone(), server.requests()));
}

/// Live lines, after the pump has screened and resolved them.
pub async fn messages(app: &AppHandle, messages: &[ChatMessage]) {
    if let Some((server, lines)) = reduce(app, messages).await {
        server.messages(lines).await;
    }
}

/// Backfill from before Lurkn joined, oldest first.
pub async fn history(app: &AppHandle, messages: &[ChatMessage]) {
    if let Some((server, lines)) = reduce(app, messages).await {
        server.history(lines).await;
    }
}

/// The lines for the channel chat is on now. A batch can straddle a switch,
/// and the window must not show the old channel's tail under the new name.
async fn reduce(
    app: &AppHandle,
    messages: &[ChatMessage],
) -> Option<(WatchServer, Vec<WatchMessage>)> {
    let state = app.try_state::<AppState>()?;
    let server = running(&state).await?;
    let current = state
        .current_channel
        .read()
        .await
        .as_ref()
        .map(|(_, id)| id.clone())?;

    let lines: Vec<WatchMessage> = messages
        .iter()
        .filter(|m| m.channel_id == current)
        .filter_map(WatchMessage::from_chat)
        .collect();
    (!lines.is_empty()).then_some((server, lines))
}

/// Ctrl and `+` or `-` in the window: one step of Lurkn's chat font, saved
/// like any other settings change. The window gets the new size back through
/// the look the frontend sends once it has re-rendered.
async fn font_steps(app: AppHandle, mut requests: broadcast::Receiver<Step>) {
    loop {
        match requests.recv().await {
            Ok(step) => step_font(&app, step).await,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn step_font(app: &AppHandle, step: Step) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut settings = state.settings.read().await.clone();
    let size = settings.chat.font_size;
    let next = match step {
        Step::Bigger => size.saturating_add(1),
        Step::Smaller => size.saturating_sub(1),
    }
    .clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
    if next == size {
        return;
    }

    settings.chat.font_size = next;
    if let Err(error) = crate::commands::settings::apply(app, &state, settings).await {
        tracing::warn!(%error, "could not change the chat font from the synced window");
    }
}

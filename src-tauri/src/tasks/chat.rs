//! Forwards chat from the supervisor to the frontend.
//!
//! Messages are coalesced into batches on a short tick rather than emitted one
//! by one: a busy channel produces hundreds a second and one IPC round trip per
//! message saturates the bridge. See `docs/architecture.md` §8.1.

use std::time::Duration;

use lurkn_chat::transport::{ChatConnState, ChatEvent};
use lurkn_core::ChatMessage;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::events::{ChatBatch, ChatStateChanged, ModerationEvent};
use crate::state::AppState;

/// How long messages accumulate before being sent up. Short enough to feel live,
/// long enough that a fast channel becomes a handful of batches per second.
const BATCH_INTERVAL: Duration = Duration::from_millis(80);
/// A hard ceiling so one tick cannot ship an unbounded payload.
const MAX_BATCH: usize = 200;

pub async fn run_pump(app: AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut events = state.chat_events.subscribe();
    let mut pending: Vec<ChatMessage> = Vec::new();
    let mut ticker = tokio::time::interval(BATCH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(ChatEvent::Message(message)) => {
                    let mut message = *message;
                    let audience = crate::overlay::audience(&state, message.channel_id.as_str()).await;
                    if audience.overlay {
                        crate::overlay::offer(&app, message.clone()).await;
                    }
                    // An ignored message never leaves Rust: it costs no IPC,
                    // and it cannot raise a notification.
                    if audience.view && screen(&app, &mut message, true).await {
                        pending.push(message);
                        if pending.len() >= MAX_BATCH {
                            flush(&app, &mut pending).await;
                        }
                    }
                }
                Ok(ChatEvent::State(state_change)) => {
                    // Order matters: deliver what arrived before the state moved.
                    flush(&app, &mut pending).await;
                    publish_state(&app, state_change).await;
                }
                // Moderation reaches the overlay too: a line removed from chat
                // must not stay up on the stream.
                Ok(ChatEvent::Cleared { channel_id }) => {
                    let audience = crate::overlay::audience(&state, &channel_id).await;
                    if audience.overlay {
                        if let Some(server) = crate::overlay::server(&app).await {
                            server.clear().await;
                        }
                    }
                    if audience.view {
                        // Deliver what arrived first, so the clear cannot land
                        // ahead of messages it was meant to remove.
                        flush(&app, &mut pending).await;
                        if let Some(server) = crate::watch::running(&state).await {
                            server.clear().await;
                        }
                        emit_moderation(&app, ModerationEvent::Cleared { channel_id });
                    }
                }
                Ok(ChatEvent::UserCleared { channel_id, user_id, user_login }) => {
                    let audience = crate::overlay::audience(&state, &channel_id).await;
                    if audience.overlay {
                        if let Some(server) = crate::overlay::server(&app).await {
                            server.clear_user(&user_login).await;
                        }
                    }
                    if audience.view {
                        flush(&app, &mut pending).await;
                        if let Some(server) = crate::watch::running(&state).await {
                            server.clear_user(&user_login).await;
                        }
                        emit_moderation(
                            &app,
                            ModerationEvent::UserCleared { user_id, user_login },
                        );
                    }
                }
                Ok(ChatEvent::Deleted { channel_id, message_id, user_login }) => {
                    let audience = crate::overlay::audience(&state, &channel_id).await;
                    if audience.overlay {
                        if let Some(server) = crate::overlay::server(&app).await {
                            server.delete(&message_id).await;
                        }
                    }
                    if audience.view {
                        flush(&app, &mut pending).await;
                        if let Some(server) = crate::watch::running(&state).await {
                            server.delete(&message_id).await;
                        }
                        emit_moderation(
                            &app,
                            ModerationEvent::MessageDeleted { message_id, user_login },
                        );
                    }
                }
                Ok(ChatEvent::OwnStream { live }) => {
                    if let Some(app_state) = app.try_state::<AppState>() {
                        *app_state.own_stream_live.write().await = live;

                        let settings = app_state.settings.read().await.broadcast.clone();
                        if settings.notify_own_stream {
                            crate::notify::own_stream(&app, live);
                        }

                        use tauri_specta::Event;
                        if let Err(error) = crate::events::OwnStreamChanged(live).emit(&app) {
                            tracing::warn!(%error, "could not publish your stream state");
                        }
                    }
                }
                Ok(ChatEvent::Whisper(received)) => {
                    // Written before it is announced: Twitch serves no whisper
                    // history, so a whisper that reaches the UI without reaching
                    // the store is gone the moment the window closes.
                    if let Some(app_state) = app.try_state::<AppState>() {
                        if let Err(error) = app_state.store.save_whisper(&received) {
                            tracing::error!(%error, "could not record a whisper");
                        }
                        let unread = app_state.store.unread_whispers().unwrap_or(0);
                        // The tray is the only surface a hidden window leaves.
                        crate::tray::refresh(&app).await;

                        if app_state.settings.read().await.notifications.whispers {
                            crate::notify::whisper(&app, &received.peer_name, &received.body);
                        }

                        use tauri_specta::Event;
                        if let Err(error) = (crate::events::WhisperReceived {
                            whisper: *received,
                            unread,
                        })
                        .emit(&app)
                        {
                            tracing::warn!(%error, "could not publish a whisper");
                        }
                    }
                }
                Ok(ChatEvent::RoomStateChanged { channel_id, state: room }) => {
                    // The own channel's slow mode is not the open channel's.
                    if crate::overlay::audience(&state, &channel_id).await.view {
                        *state.room_state.write().await = room.clone();
                        emit_moderation(&app, ModerationEvent::RoomState { state: room });
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    // The UI fell behind a very fast channel. Dropping old lines
                    // is the right trade; say so rather than pretending.
                    tracing::warn!(skipped, "chat consumer lagged; dropped messages");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            },

            _ = ticker.tick() => flush(&app, &mut pending).await,
        }
    }
}

/// Fill in third-party emotes and badge images.
///
/// Shared with the history backfill, so a message from before Lurkn joined
/// renders exactly like one that arrived live rather than as a plainer copy.
///
/// Twitch emotes already arrived as fragments on the live path and as tag
/// offsets on the history one; this resolves 7TV/BTTV/FFZ names inside the
/// remaining text (`docs/architecture.md` §6.2).
pub async fn resolve(app: &AppHandle, messages: &mut [ChatMessage]) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let registry = state.emotes.read().await;
    if !registry.is_empty() {
        for message in messages.iter_mut() {
            message.tokens = registry.apply(std::mem::take(&mut message.tokens));
        }
    }

    let (channel_badges, global_badges) = (
        state.channel_badges.read().await,
        state.global_badges.read().await,
    );
    if !channel_badges.is_empty() || !global_badges.is_empty() {
        for message in messages.iter_mut() {
            lurkn_twitch::badges::resolve(
                &mut message.author.badges,
                &channel_badges,
                &global_badges,
            );
        }
    }
}

async fn flush(app: &AppHandle, pending: &mut Vec<ChatMessage>) {
    if pending.is_empty() {
        return;
    }

    let mut messages = std::mem::take(pending);
    resolve(app, &mut messages).await;
    crate::watch::messages(app, &messages).await;

    if let Err(error) = (ChatBatch { messages }).emit(app) {
        tracing::error!(%error, "could not emit a chat batch");
    }
}

async fn publish_state(app: &AppHandle, state_change: ChatConnState) {
    if let Some(state) = app.try_state::<AppState>() {
        *state.chat_state.write().await = state_change;
        let status = state.chat_status().await;
        if let Err(error) = ChatStateChanged(status).emit(app) {
            tracing::error!(%error, "could not emit the chat state");
        }
    }
}

fn emit_moderation(app: &AppHandle, event: ModerationEvent) {
    if let Err(error) = event.emit(app) {
        tracing::error!(%error, "could not emit a moderation event");
    }
}

/// Ask Twitch what restrictions a channel already has.
///
/// `channel.chat_settings.update` only fires on a *change*, so without this a
/// channel that was already in followers-only looks unrestricted until a
/// moderator happens to toggle something.
pub async fn prime_room_state(app: &AppHandle, channel: &lurkn_core::UserId) {
    use lurkn_chat::api::ChatApi;

    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(viewer_id) = state.current_user_id().await else {
        return;
    };

    let room = match state
        .chat_api(viewer_id)
        .chat_settings(channel.as_str())
        .await
    {
        Ok(room) => room,
        Err(error) => {
            // Not knowing the mode is a cosmetic gap, not a reason to fail the join.
            tracing::warn!(%error, "could not read the channel's chat settings");
            return;
        }
    };

    // A switch during the request must not apply the wrong channel's state.
    let still_current = state
        .current_channel
        .read()
        .await
        .as_ref()
        .is_some_and(|(_, id)| id == channel);
    if !still_current {
        return;
    }

    if !room.is_unrestricted() {
        tracing::debug!(channel = channel.as_str(), labels = ?room.labels(), "chat is restricted");
    }
    *state.room_state.write().await = room.clone();
    emit_moderation(app, ModerationEvent::RoomState { state: room });
}

/// Apply the reader's ignores and highlights to one message. `false` means
/// drop it.
///
/// Done here rather than in the renderer because the rules need the reader's
/// own name and their settings; deciding once keeps the notification and the
/// marked row in agreement. Shared with the history backfill, which passes
/// `announce: false` — a notification for something said before Lurkn joined
/// is news about the past.
///
/// The reader's own messages are exempt from both: they know what they said,
/// and a message of theirs vanishing because it matched an ignore reads as the
/// send having failed.
pub async fn screen(
    app: &AppHandle,
    message: &mut lurkn_core::ChatMessage,
    announce: bool,
) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return true;
    };

    let login = match &*state.auth.read().await {
        crate::commands::auth::AuthStatus::Authenticated { login, .. } => Some(login.clone()),
        _ => None,
    };

    if login
        .as_deref()
        .is_some_and(|login| message.author.login.as_str().eq_ignore_ascii_case(login))
    {
        return true;
    }

    let rules = std::sync::Arc::clone(&*state.rules.read().await);
    if !rules.screen(message) {
        return false;
    }

    let settings = state.settings.read().await;

    // Being named comes first: it is the highlight nobody had to write.
    let named = login.as_deref().is_some_and(|login| {
        settings.chat.highlight_mentions
            && !rules.is_excluded(message.author.login.as_str())
            && lurkn_core::chat::mentions(&message.raw_text, login, login, &[])
    });

    let mark = if named {
        Some(lurkn_core::rules::Mark {
            color: None,
            notify: settings.notifications.highlights,
        })
    } else {
        rules.highlight(message)
    };

    let Some(mark) = mark else {
        return true;
    };
    message.highlighted = true;
    message.highlight_color = mark.color;

    if announce && mark.notify {
        let channel = state
            .current_channel
            .read()
            .await
            .as_ref()
            .map(|(login, _)| login.to_string())
            .unwrap_or_default();

        crate::notify::highlight(
            app,
            &channel,
            &message.author.display_name,
            &message.raw_text,
        );
    }

    true
}

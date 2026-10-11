//! The chat overlay for OBS: when it runs, which channel it shows, and what
//! reaches it. The server and the page live in `lurkn-watch::overlay`.
//!
//! By default it shows the reader's own channel, whatever is open in Lurkn. A
//! streamer who clicks over to the channel they are about to raid must not take
//! their overlay with them, so the own channel is subscribed alongside the open
//! one — a second subscription set, which costs nothing against Twitch's limits
//! (`docs/architecture.md` §5.1) — and its messages are routed here rather than
//! into the log. `OverlaySource::Open` shows the open channel instead, for
//! watch-alongs, and follows every switch.

use std::sync::Arc;

use lurkn_core::{ChatMessage, OverlaySource, UserId};
use lurkn_watch::overlay::{wire, OverlayLook, OverlayMessage, OverlayServer};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Bring the overlay in line with the settings, the login and the open
/// channel: start, stop, move to another port, or restyle in place, and point
/// it at the channel it should show.
///
/// Called on every settings change and channel switch; with nothing to do it
/// costs a few lock reads.
pub async fn sync(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let _one_at_a_time = state.overlay_sync.lock().await;

    let settings = state.settings.read().await.overlay.clone();
    let look = OverlayLook::from_settings(&settings);

    {
        let mut slot = state.overlay.write().await;
        let keep = slot
            .as_ref()
            .is_some_and(|server| settings.enabled && server.port() == settings.port);

        if keep {
            if let Some(server) = slot.as_ref() {
                server.set_look(&look).await;
            }
        } else {
            if let Some(old) = slot.take() {
                old.shutdown().await;
            }
            *state.overlay_error.write().await = None;

            if settings.enabled {
                match OverlayServer::start(settings.port, &look).await {
                    Ok(server) => *slot = Some(server),
                    Err(error) => {
                        tracing::warn!(%error, port = settings.port, "could not start the chat overlay");
                        *state.overlay_error.write().await =
                            Some(bind_error(settings.port, &error));
                    }
                }
            }
        }
    }

    let running = state.overlay.read().await.is_some();
    let target = match (running, settings.source) {
        (false, _) => None,
        (true, OverlaySource::Own) => state.current_user_id().await.map(UserId),
        (true, OverlaySource::Open) => state
            .current_channel
            .read()
            .await
            .as_ref()
            .map(|(_, id)| id.clone()),
    };
    if *state.overlay_channel.read().await == target {
        return;
    }

    // What is on screen belongs to the channel being left; a stream showing
    // one channel's name beside another's lines would be worse than a blank.
    if let Some(server) = state.overlay.read().await.clone() {
        server.clear().await;
    }

    // Emotes before the subscription, so the first message already has them.
    // Both come from the same caches the open channel uses.
    match &target {
        Some(channel) => {
            let registry =
                crate::tasks::emotes::registry_for(&state.emote_fetcher, &state.store, channel)
                    .await;
            *state.overlay_emotes.write().await = registry;
            load_badges(&state, channel).await;
        }
        None => {
            *state.overlay_emotes.write().await = Default::default();
            state.overlay_badges.write().await.clear();
        }
    }

    *state.overlay_channel.write().await = target.clone();

    // Logged out there is no chat to subscribe through, and nothing to drop.
    if target.is_some() || state.chat.read().await.is_some() {
        if let Err(error) = state.resync_channels().await {
            tracing::warn!(%error, "could not subscribe the overlay to its channel");
        }
    }
    tracing::debug!(
        channel = target.as_ref().map(UserId::as_str),
        source = ?settings.source,
        "the overlay switched channel"
    );
}

fn bind_error(port: u16, error: &lurkn_watch::WatchError) -> String {
    match error {
        lurkn_watch::WatchError::Bind(io) if io.kind() == std::io::ErrorKind::AddrInUse => {
            format!("Port {port} is already in use by another program. Pick a different port.")
        }
        _ => format!("Could not open port {port} for the overlay."),
    }
}

async fn load_badges(state: &AppState, channel: &UserId) {
    let service = state.badges();

    if state.global_badges.read().await.is_empty() {
        match service.global().await {
            Ok(map) => *state.global_badges.write().await = map,
            Err(error) => tracing::warn!(%error, "could not load global chat badges"),
        }
    }
    match service.channel(channel.as_str()).await {
        Ok(map) => *state.overlay_badges.write().await = map,
        Err(error) => tracing::warn!(%error, "could not load your channel's badges"),
    }
}

/// Who an event from `channel_id` is for: the open chat, the overlay, or both.
///
/// Everything goes to the open chat as before, except what arrives from the
/// own channel only because the overlay asked for it — that would otherwise
/// land in the log of whatever channel is open.
pub async fn audience(state: &AppState, channel_id: &str) -> Audience {
    let overlay = state
        .overlay_channel
        .read()
        .await
        .as_ref()
        .is_some_and(|own| own.as_str() == channel_id);
    if !overlay {
        return Audience {
            view: true,
            overlay: false,
        };
    }

    let open = state
        .current_channel
        .read()
        .await
        .as_ref()
        .is_some_and(|(_, id)| id.as_str() == channel_id);
    Audience {
        view: open,
        overlay: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Audience {
    pub view: bool,
    pub overlay: bool,
}

/// Put a message on the overlay, if it belongs there.
pub async fn offer(app: &AppHandle, mut message: ChatMessage) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(server) = state.overlay.read().await.clone() else {
        return;
    };

    // Ignores hold on stream too — the reader set them up so as not to see
    // something — but, as in Lurkn, never against the reader's own lines.
    let own = state.current_user_id().await;
    if own.as_deref() != Some(message.author.id.as_str()) {
        let rules = Arc::clone(&*state.rules.read().await);
        if !rules.screen(&mut message) {
            return;
        }
    }

    if !wire::admits(&message, &state.settings.read().await.overlay) {
        return;
    }

    {
        let registry = state.overlay_emotes.read().await;
        if !registry.is_empty() {
            message.tokens = registry.apply(std::mem::take(&mut message.tokens));
        }
    }
    {
        let (channel, global) = (
            state.overlay_badges.read().await,
            state.global_badges.read().await,
        );
        lurkn_twitch::badges::resolve(&mut message.author.badges, &channel, &global);
    }

    server.message(&OverlayMessage::from_chat(&message)).await;
}

/// The server, if the overlay is running.
pub async fn server(app: &AppHandle) -> Option<OverlayServer> {
    let state = app.try_state::<AppState>()?;
    let server = state.overlay.read().await.clone();
    server
}

/// A line that looks like chat, for checking the overlay without waiting for
/// someone to say something. It never goes to Twitch.
pub fn test_message(name: &str) -> OverlayMessage {
    let now = time::OffsetDateTime::now_utc();
    OverlayMessage {
        id: format!("lurkn-test-{}", now.unix_timestamp_nanos()),
        login: name.to_ascii_lowercase(),
        name: name.to_owned(),
        color: Some("#9146ff".to_owned()),
        badges: Vec::new(),
        action: false,
        parts: vec![
            wire::Part::Text {
                text: "This is a test message from Lurkn ".to_owned(),
            },
            wire::Part::Emote {
                name: "Kappa".to_owned(),
                url: "https://static-cdn.jtvnw.net/emoticons/v2/25/default/dark/3.0".to_owned(),
                aspect: Some((28, 28)),
                zero_width: false,
            },
        ],
        at: (now.unix_timestamp_nanos() / 1_000_000) as i64,
    }
}

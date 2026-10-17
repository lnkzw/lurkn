//! The EventSub WebSocket supervisor.
//!
//! Owns the socket, the keepalive watchdog, the reconnect handover and the
//! subscription set. See `docs/architecture.md` §5.2.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use lurkn_core::UserId;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::api::ChatApi;
use crate::backoff::Backoff;
use crate::dedupe::MessageDedupe;
use crate::eventsub::chat::{to_message, ChatMessageEvent};
use crate::eventsub::envelope::{parse, Envelope};
use crate::eventsub::moderation;
use crate::eventsub::notification;
use crate::eventsub::raid;
use crate::eventsub::subs::{self, reconcile, SubscriptionKey};
use crate::eventsub::whisper;
use crate::transport::{ChatConnState, ChatError, ChatEvent};
use crate::{EVENTSUB_WS_URL, SUBSCRIBE_DEADLINE_SECS};

/// Grace added to Twitch's keepalive timeout before declaring the socket dead.
/// A little slack avoids reconnecting over ordinary network jitter.
const KEEPALIVE_GRACE: Duration = Duration::from_secs(5);

/// What the app asks the supervisor to do.
#[derive(Debug)]
enum Command {
    SetChannels(Vec<UserId>),
    Send {
        broadcaster_id: String,
        text: String,
        /// The message this one replies to, if any.
        reply_to: Option<String>,
        ack: mpsc::Sender<Result<(), ChatError>>,
    },
    Shutdown,
}

/// Handle to a running supervisor.
#[derive(Debug, Clone)]
pub struct ChatHandle {
    tx: mpsc::Sender<Command>,
}

impl ChatHandle {
    /// Set the channels to receive chat for. Replaces the previous set.
    pub async fn set_channels(&self, channels: Vec<UserId>) -> Result<(), ChatError> {
        self.tx
            .send(Command::SetChannels(channels))
            .await
            .map_err(|_| ChatError::NotConnected)
    }

    pub async fn send(
        &self,
        broadcaster_id: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<(), ChatError> {
        let (ack_tx, mut ack_rx) = mpsc::channel(1);
        self.tx
            .send(Command::Send {
                broadcaster_id: broadcaster_id.to_owned(),
                text: text.to_owned(),
                reply_to: reply_to.map(str::to_owned),
                ack: ack_tx,
            })
            .await
            .map_err(|_| ChatError::NotConnected)?;

        ack_rx.recv().await.unwrap_or(Err(ChatError::NotConnected))
    }

    pub async fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown).await;
    }
}

/// Start the supervisor against Twitch. Runs until [`ChatHandle::shutdown`].
pub fn spawn(api: Arc<dyn ChatApi>, events: broadcast::Sender<ChatEvent>) -> ChatHandle {
    spawn_with_url(api, events, EVENTSUB_WS_URL.to_owned())
}

/// Same, against a specific endpoint. Exists so tests can point the supervisor at
/// a local server without an ambient override.
pub fn spawn_with_url(
    api: Arc<dyn ChatApi>,
    events: broadcast::Sender<ChatEvent>,
    ws_url: String,
) -> ChatHandle {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(run(api, events, rx, ws_url));
    ChatHandle { tx }
}

struct Session {
    id: String,
    keepalive: Duration,
    /// (channel, event type) -> subscription id.
    subscriptions: HashMap<SubscriptionKey, String>,
}

async fn run(
    api: Arc<dyn ChatApi>,
    events: broadcast::Sender<ChatEvent>,
    commands: mpsc::Receiver<Command>,
    ws_url: String,
) {
    let commands = Arc::new(Mutex::new(commands));
    let desired: Arc<Mutex<Vec<UserId>>> = Arc::new(Mutex::new(Vec::new()));
    let mut backoff = Backoff::default();

    loop {
        let state = if backoff.attempt() == 0 {
            ChatConnState::Connecting
        } else {
            ChatConnState::Reconnecting {
                attempt: backoff.attempt(),
            }
        };
        let _ = events.send(ChatEvent::State(state));

        let mut connected = false;
        let outcome =
            connect_and_serve(&api, &events, &commands, &desired, &ws_url, &mut connected).await;

        // A socket that reached Connected earns a fresh backoff budget, so a
        // long-lived connection dropping once does not inherit an old delay.
        if connected {
            backoff.reset();
        }

        match outcome {
            Outcome::ShutdownRequested => {
                let _ = events.send(ChatEvent::State(ChatConnState::Disconnected));
                return;
            }
            Outcome::Fatal(reason) => {
                // A revoked subscription or a dead token: retrying would spin.
                tracing::warn!(%reason, "chat stopped and will not retry");
                let _ = events.send(ChatEvent::State(ChatConnState::Failed { reason }));
                return;
            }
            Outcome::Retry(reason) => {
                let delay = backoff.next_delay();
                tracing::info!(%reason, ?delay, attempt = backoff.attempt(), "chat reconnecting");
                tokio::time::sleep(delay).await;
            }
        }
    }
}

enum Outcome {
    ShutdownRequested,
    Fatal(String),
    Retry(String),
}

async fn connect_and_serve(
    api: &Arc<dyn ChatApi>,
    events: &broadcast::Sender<ChatEvent>,
    commands: &Arc<Mutex<mpsc::Receiver<Command>>>,
    desired: &Arc<Mutex<Vec<UserId>>>,
    ws_url: &str,
    connected: &mut bool,
) -> Outcome {
    let mut session: Option<Session> = None;
    let mut dedupe = MessageDedupe::default();
    // Reported back so a socket that worked for a while restarts its backoff.

    let (stream, _) = match tokio_tungstenite::connect_async(ws_url).await {
        Ok(pair) => pair,
        Err(error) => return Outcome::Retry(format!("could not connect: {error}")),
    };
    let (mut sink, mut source) = stream.split();

    // Until the welcome arrives, Twitch's documented deadline is the only clock.
    let mut read_timeout = Duration::from_secs(SUBSCRIBE_DEADLINE_SECS + 5);

    loop {
        let mut commands_guard = commands.lock().await;

        tokio::select! {
            biased;

            command = commands_guard.recv() => {
                drop(commands_guard);
                match command {
                    None | Some(Command::Shutdown) => {
                        let _ = sink.send(WsMessage::Close(None)).await;
                        return Outcome::ShutdownRequested;
                    }
                    Some(Command::SetChannels(channels)) => {
                        *desired.lock().await = channels;
                        if let Some(session) = session.as_mut() {
                            if let Err(error) =
                                sync_subscriptions(api, session, &desired.lock().await).await
                            {
                                if error.requires_reauth() {
                                    return Outcome::Fatal(error.to_string());
                                }
                                tracing::warn!(%error, "could not update chat subscriptions");
                                let _ = events.send(ChatEvent::State(ChatConnState::Failed {
                                    reason: "Could not join that channel's chat.".to_owned(),
                                }));
                            }
                        }
                    }
                    Some(Command::Send { broadcaster_id, text, reply_to, ack }) => {
                        let result = api
                            .send_chat_message(&broadcaster_id, &text, reply_to.as_deref())
                            .await;
                        let _ = ack.send(result).await;
                    }
                }
            }

            frame = tokio::time::timeout(read_timeout, source.next()) => {
                drop(commands_guard);
                let frame = match frame {
                    // No event and no keepalive within the window: assume dead.
                    Err(_) => return Outcome::Retry("keepalive timeout".to_owned()),
                    Ok(None) => return Outcome::Retry("socket closed".to_owned()),
                    Ok(Some(Err(error))) => {
                        return Outcome::Retry(format!("socket error: {error}"))
                    }
                    Ok(Some(Ok(frame))) => frame,
                };

                let text = match frame {
                    WsMessage::Text(text) => text,
                    WsMessage::Close(frame) => {
                        return Outcome::Retry(format!("closed by Twitch: {frame:?}"))
                    }
                    // Ping/pong are handled by tungstenite; binary is not used.
                    _ => continue,
                };

                match parse(&text) {
                    Ok(Envelope::Welcome { session_id, keepalive_timeout_secs }) => {
                        tracing::debug!(keepalive_timeout_secs, "chat socket connected");
                        read_timeout =
                            Duration::from_secs(keepalive_timeout_secs) + KEEPALIVE_GRACE;

                        let mut new_session = Session {
                            id: session_id,
                            keepalive: read_timeout,
                            subscriptions: HashMap::new(),
                        };

                        let want = desired.lock().await.clone();
                        if let Err(error) = sync_subscriptions(api, &mut new_session, &want).await {
                            if error.requires_reauth() {
                                return Outcome::Fatal(error.to_string());
                            }
                            return Outcome::Retry(error.to_string());
                        }

                        // Free of any scope, so this is expected to work; it is
                        // still not fatal, because chat is the point and this is
                        // only the reader's own live state.
                        match api.create_own_stream_subscriptions(&new_session.id).await {
                            Ok(()) => tracing::debug!("subscribed to your own stream"),
                            Err(error) => {
                                tracing::warn!(%error, "no live state for your own channel")
                            }
                        }

                        // Whispers ride an optional scope, so a refusal here is
                        // a user who has not opted in — noted once, never fatal,
                        // and chat carries on regardless.
                        match api.create_whisper_subscription(&new_session.id).await {
                            Ok(_) => tracing::debug!("subscribed to whispers"),
                            Err(error) => tracing::debug!(
                                %error,
                                "whispers are off; grant the permission in settings to receive them"
                            ),
                        }

                        session = Some(new_session);
                        *connected = true;
                        let _ = events.send(ChatEvent::State(ChatConnState::Connected));
                    }

                    Ok(Envelope::Keepalive) => {}

                    Ok(Envelope::Notification { message_id, subscription_type, event }) => {
                        // EventSub may redeliver, notably across a handover.
                        if !dedupe.accept(&lurkn_core::MessageId(message_id)) {
                            continue;
                        }

                        match subscription_type.as_str() {
                            subs::CHAT_MESSAGE_TYPE => {
                                match serde_json::from_value::<ChatMessageEvent>(event) {
                                    Ok(parsed) => {
                                        if let Some(message) =
                                            to_message(parsed, time::OffsetDateTime::now_utc())
                                        {
                                            let _ = events
                                                .send(ChatEvent::Message(Box::new(message)));
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(
                                            %error,
                                            "skipping an unreadable chat message"
                                        );
                                    }
                                }
                            }
                            subs::STREAM_ONLINE_TYPE | subs::STREAM_OFFLINE_TYPE => {
                                let live = subscription_type == subs::STREAM_ONLINE_TYPE;
                                tracing::debug!(live, "your own stream changed state");
                                let _ = events.send(ChatEvent::OwnStream { live });
                            }

                            subs::WHISPER_TYPE => {
                                match serde_json::from_value::<whisper::WhisperEvent>(event) {
                                    Ok(parsed) => {
                                        if let Some(received) = whisper::to_whisper(
                                            parsed,
                                            time::OffsetDateTime::now_utc(),
                                        ) {
                                            let _ = events
                                                .send(ChatEvent::Whisper(Box::new(received)));
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, "skipping an unreadable whisper");
                                    }
                                }
                            }

                            subs::CHAT_NOTIFICATION_TYPE => {
                                match serde_json::from_value::<notification::NotificationEvent>(
                                    event,
                                ) {
                                    Ok(parsed) => {
                                        if let Some(message) = notification::to_message(
                                            parsed,
                                            time::OffsetDateTime::now_utc(),
                                        ) {
                                            let _ = events
                                                .send(ChatEvent::Message(Box::new(message)));
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(
                                            %error,
                                            "skipping an unreadable channel event"
                                        );
                                    }
                                }
                            }
                            subs::CHANNEL_RAID_TYPE => {
                                match serde_json::from_value::<raid::RaidEvent>(event) {
                                    Ok(parsed) => {
                                        if let Some(raid) = raid::to_raid(parsed) {
                                            tracing::debug!(
                                                from = raid.from_login.as_str(),
                                                to = raid.to_login.as_str(),
                                                "a channel raided another"
                                            );
                                            let _ = events.send(ChatEvent::Raided(Box::new(raid)));
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, "skipping an unreadable raid");
                                    }
                                }
                            }
                            other => {
                                if let Some(moderation_event) =
                                    moderation::to_event(other, &event)
                                {
                                    let _ = events.send(moderation_event);
                                } else {
                                    tracing::debug!(
                                        subscription_type = other,
                                        "ignoring an unhandled subscription type"
                                    );
                                }
                            }
                        }
                    }

                    Ok(Envelope::Reconnect { reconnect_url }) => {
                        tracing::info!("Twitch asked us to move to another edge");

                        // The old socket keeps delivering until the new one
                        // greets us; closing early loses whatever arrives in the
                        // gap. Twitch transfers the subscriptions itself, so
                        // there is nothing to resubscribe.
                        let (new_stream, _) =
                            match tokio_tungstenite::connect_async(&reconnect_url).await {
                                Ok(pair) => pair,
                                Err(error) => {
                                    return Outcome::Retry(format!(
                                        "could not follow the reconnect: {error}"
                                    ))
                                }
                            };
                        let (new_sink, mut new_source) = new_stream.split();

                        match await_welcome(&mut new_source, read_timeout).await {
                            Ok((session_id, keepalive_secs)) => {
                                // Only now is it safe to drop the old socket.
                                let _ = sink.send(WsMessage::Close(None)).await;
                                sink = new_sink;
                                source = new_source;
                                read_timeout = Duration::from_secs(keepalive_secs)
                                    + KEEPALIVE_GRACE;

                                if let Some(session) = session.as_mut() {
                                    session.id = session_id;
                                    session.keepalive = read_timeout;
                                } else {
                                    session = Some(Session {
                                        id: session_id,
                                        keepalive: read_timeout,
                                        subscriptions: HashMap::new(),
                                    });
                                }

                                tracing::debug!("chat socket handover complete");
                                let _ = events.send(ChatEvent::State(ChatConnState::Connected));
                            }
                            Err(error) => {
                                return Outcome::Retry(format!(
                                    "the replacement socket never greeted us: {error}"
                                ))
                            }
                        }
                    }

                    Ok(Envelope::Revocation { subscription_type, status }) => {
                        return Outcome::Fatal(format!(
                            "Twitch revoked the {subscription_type} subscription ({status})."
                        ));
                    }

                    Ok(Envelope::Unknown { message_type }) => {
                        tracing::debug!(%message_type, "ignoring an unknown EventSub message");
                    }

                    Err(error) => {
                        // A frame we cannot read is not a reason to drop the socket.
                        tracing::warn!(%error, "ignoring an unreadable EventSub frame");
                    }
                }
            }
        }

        if let Some(session) = session.as_ref() {
            debug_assert!(session.keepalive > Duration::ZERO);
        }
    }
}

/// Bring `session.subscriptions` in line with `want`.
async fn sync_subscriptions(
    api: &Arc<dyn ChatApi>,
    session: &mut Session,
    want: &[UserId],
) -> Result<(), ChatError> {
    let plan = reconcile(&session.subscriptions, want);
    if plan.is_empty() {
        return Ok(());
    }

    for subscription_id in &plan.to_delete {
        if let Err(error) = api.delete_subscription(subscription_id).await {
            // A subscription we cannot delete is Twitch's to expire; not fatal.
            tracing::warn!(%error, "could not delete a chat subscription");
        }
        session.subscriptions.retain(|_, id| id != subscription_id);
    }

    for key in &plan.to_create {
        let (broadcaster, subscription_type) = key;
        let created = api
            .create_chat_subscription(subscription_type, broadcaster.as_str(), &session.id)
            .await;

        match created {
            Ok(id) => {
                session.subscriptions.insert(key.clone(), id);
            }
            // Losing the message feed means no chat, so that one is fatal.
            Err(error) if subs::is_required(subscription_type) => return Err(error),
            // Moderation and room state are enhancements: note it and carry on,
            // rather than leaving the user with no chat at all.
            Err(error) => {
                tracing::warn!(
                    %error,
                    subscription_type,
                    "chat will work without this subscription"
                );
            }
        }
    }

    Ok(())
}

/// Wait for `session_welcome` on a freshly opened socket.
///
/// Returns the new session id and its keepalive timeout. Any other frame is
/// ignored: the replacement socket sends nothing else before greeting us.
async fn await_welcome<S>(source: &mut S, timeout: Duration) -> Result<(String, u64), ChatError>
where
    S: futures_util::Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(ChatError::Transport(
                "timed out waiting for the welcome".into(),
            ));
        }

        match tokio::time::timeout(remaining, source.next()).await {
            Err(_) => {
                return Err(ChatError::Transport(
                    "timed out waiting for the welcome".into(),
                ))
            }
            Ok(None) => {
                return Err(ChatError::Transport(
                    "the socket closed before greeting us".into(),
                ))
            }
            Ok(Some(Err(error))) => return Err(ChatError::Transport(error.to_string())),
            Ok(Some(Ok(WsMessage::Text(text)))) => {
                if let Ok(Envelope::Welcome {
                    session_id,
                    keepalive_timeout_secs,
                }) = parse(&text)
                {
                    return Ok((session_id, keepalive_timeout_secs));
                }
            }
            Ok(Some(Ok(_))) => {}
        }
    }
}

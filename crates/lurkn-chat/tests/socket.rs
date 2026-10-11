#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The EventSub socket against a fake Twitch: handshake, keepalive starvation,
//! the reconnect handover, revocation and redelivery.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use lurkn_chat::api::ChatApi;
use lurkn_chat::transport::{ChatConnState, ChatError, ChatEvent};
use lurkn_core::{RoomState, UserId};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// Records what the socket asked Helix to do.
#[derive(Debug, Default)]
struct FakeApi {
    /// (subscription type, broadcaster id, session id)
    created: Mutex<Vec<(String, String, String)>>,
    deleted: Mutex<Vec<String>>,
    /// `(broadcaster, text, reply_parent)` for each send.
    sent: Mutex<Vec<(String, String, Option<String>)>>,
    /// How many times the session-level whisper subscription was created.
    whisper_subs: Mutex<u32>,
    /// How many times the reader's own stream subscriptions were created.
    own_stream_subs: Mutex<u32>,
    next_id: AtomicUsize,
}

#[async_trait]
impl ChatApi for FakeApi {
    fn viewer_id(&self) -> String {
        "22222".into()
    }

    async fn create_chat_subscription(
        &self,
        subscription_type: &str,
        broadcaster_id: &str,
        session_id: &str,
    ) -> Result<String, ChatError> {
        self.created.lock().unwrap().push((
            subscription_type.to_owned(),
            broadcaster_id.to_owned(),
            session_id.to_owned(),
        ));
        let n = self.next_id.fetch_add(1, Ordering::SeqCst);
        Ok(format!("sub-{n}"))
    }

    async fn chat_settings(&self, _broadcaster_id: &str) -> Result<RoomState, ChatError> {
        Ok(RoomState::default())
    }

    async fn delete_subscription(&self, subscription_id: &str) -> Result<(), ChatError> {
        self.deleted
            .lock()
            .unwrap()
            .push(subscription_id.to_owned());
        Ok(())
    }

    async fn create_own_stream_subscriptions(&self, _session_id: &str) -> Result<(), ChatError> {
        *self.own_stream_subs.lock().unwrap() += 1;
        Ok(())
    }

    async fn create_whisper_subscription(&self, _session_id: &str) -> Result<String, ChatError> {
        *self.whisper_subs.lock().unwrap() += 1;
        Ok("whisper-sub".into())
    }

    async fn send_whisper(&self, _to: &str, _text: &str) -> Result<(), ChatError> {
        Ok(())
    }

    async fn send_chat_message(
        &self,
        broadcaster_id: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<(), ChatError> {
        self.sent.lock().unwrap().push((
            broadcaster_id.to_owned(),
            text.to_owned(),
            reply_to.map(str::to_owned),
        ));
        Ok(())
    }
}

fn welcome(session_id: &str, keepalive: u64) -> String {
    format!(
        r#"{{"metadata":{{"message_id":"w-{session_id}","message_type":"session_welcome",
             "message_timestamp":"2026-09-20T12:00:00Z"}},
           "payload":{{"session":{{"id":"{session_id}","status":"connected",
             "keepalive_timeout_seconds":{keepalive}}}}}}}"#
    )
}

fn chat_notification(message_id: &str, text: &str) -> String {
    format!(
        r##"{{"metadata":{{"message_id":"{message_id}","message_type":"notification",
             "subscription_type":"channel.chat.message","message_timestamp":"2026-09-20T12:00:00Z"}},
           "payload":{{"subscription":{{"type":"channel.chat.message"}},
             "event":{{"broadcaster_user_id":"11111","chatter_user_id":"22222",
               "chatter_user_login":"forsen","chatter_user_name":"Forsen","color":"#1E90FF",
               "badges":[],"message_id":"{message_id}","message_type":"text",
               "message":{{"text":"{text}","fragments":[{{"type":"text","text":"{text}"}}]}}}}}}}}"##
    )
}

const REVOCATION: &str = r#"{"metadata":{"message_id":"r1","message_type":"revocation"},
  "payload":{"subscription":{"type":"channel.chat.message","status":"authorization_revoked"}}}"#;

/// A scripted EventSub server: hands each connection the next script in the list.
async fn fake_server(scripts: Vec<Vec<String>>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("ws://127.0.0.1:{port}/ws");

    let handle = tokio::spawn(async move {
        for script in scripts {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                continue;
            };
            let (mut sink, mut source) = ws.split();

            tokio::spawn(async move { while source.next().await.is_some() {} });

            for frame in script {
                if frame == "__HANG__" {
                    // Send nothing: starve the client's keepalive watchdog.
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                }
                if sink.send(WsMessage::Text(frame.into())).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    });

    (url, handle)
}

fn start(url: &str, api: Arc<FakeApi>) -> (lurkn_chat::ChatHandle, broadcast::Receiver<ChatEvent>) {
    let (tx, rx) = broadcast::channel(64);
    (
        lurkn_chat::eventsub::socket::spawn_with_url(api, tx, url.to_owned()),
        rx,
    )
}

async fn next_event(
    rx: &mut broadcast::Receiver<ChatEvent>,
    within: Duration,
) -> Option<ChatEvent> {
    tokio::time::timeout(within, rx.recv())
        .await
        .ok()
        .and_then(Result::ok)
}

async fn wait_for_message(
    rx: &mut broadcast::Receiver<ChatEvent>,
    within: Duration,
) -> Option<lurkn_core::ChatMessage> {
    let deadline = tokio::time::Instant::now() + within;
    while tokio::time::Instant::now() < deadline {
        match next_event(rx, deadline - tokio::time::Instant::now()).await {
            Some(ChatEvent::Message(m)) => return Some(*m),
            Some(_) => continue,
            None => return None,
        }
    }
    None
}

#[tokio::test]
async fn subscribes_after_the_welcome_and_delivers_messages() {
    let (url, _server) = fake_server(vec![vec![
        welcome("S1", 10),
        chat_notification("m1", "hello"),
    ]])
    .await;

    let api = Arc::new(FakeApi::default());
    let (handle, mut rx) = start(&url, Arc::clone(&api));
    handle
        .set_channels(vec![UserId("11111".into())])
        .await
        .unwrap();

    let message = wait_for_message(&mut rx, Duration::from_secs(5))
        .await
        .expect("a chat message should arrive");
    assert_eq!(message.raw_text, "hello");
    assert_eq!(message.author.login.as_str(), "forsen");

    let created = api.created.lock().unwrap().clone();
    assert_eq!(
        created.len(),
        lurkn_chat::eventsub::subs::CHANNEL_SUBSCRIPTIONS.len(),
        "a joined channel needs every chat subscription, not just messages"
    );
    assert!(created
        .iter()
        .all(|(_, broadcaster, session)| broadcaster == "11111" && session == "S1"));
    // Referencing the constants means a renamed type cannot silently pass here.
    for expected in lurkn_chat::eventsub::subs::CHANNEL_SUBSCRIPTIONS {
        assert!(
            created.iter().any(|(kind, _, _)| kind == expected),
            "{expected} was never subscribed to"
        );
    }

    handle.shutdown().await;
}

/// EventSub may redeliver; the same message_id must surface once.
#[tokio::test]
async fn a_redelivered_message_is_shown_once() {
    let (url, _server) = fake_server(vec![vec![
        welcome("S1", 10),
        chat_notification("dup", "twice"),
        chat_notification("dup", "twice"),
        chat_notification("other", "second"),
    ]])
    .await;

    let api = Arc::new(FakeApi::default());
    let (handle, mut rx) = start(&url, api);
    handle
        .set_channels(vec![UserId("11111".into())])
        .await
        .unwrap();

    let first = wait_for_message(&mut rx, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(first.raw_text, "twice");

    let second = wait_for_message(&mut rx, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(second.raw_text, "second", "the duplicate must not appear");

    handle.shutdown().await;
}

/// A revoked subscription is terminal: it must not be retried in a loop.
#[tokio::test]
async fn a_revocation_stops_the_socket_for_good() {
    let (url, _server) = fake_server(vec![vec![welcome("S1", 10), REVOCATION.to_owned()]]).await;

    let api = Arc::new(FakeApi::default());
    let (handle, mut rx) = start(&url, api);
    handle
        .set_channels(vec![UserId("11111".into())])
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut failed = None;
    while tokio::time::Instant::now() < deadline {
        match next_event(&mut rx, deadline - tokio::time::Instant::now()).await {
            Some(ChatEvent::State(ChatConnState::Failed { reason })) => {
                failed = Some(reason);
                break;
            }
            Some(_) => continue,
            None => break,
        }
    }

    let reason = failed.expect("a revocation should surface as Failed");
    assert!(reason.contains("revoked"), "got {reason}");

    handle.shutdown().await;
}

/// Sending goes over Helix, not the socket.
#[tokio::test]
async fn sending_a_message_goes_through_the_api() {
    let (url, _server) = fake_server(vec![vec![welcome("S1", 10)]]).await;

    let api = Arc::new(FakeApi::default());
    let (handle, _rx) = start(&url, Arc::clone(&api));
    handle
        .set_channels(vec![UserId("11111".into())])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    handle.send("11111", "hello chat", None).await.unwrap();
    assert_eq!(
        api.sent.lock().unwrap().clone(),
        vec![("11111".to_owned(), "hello chat".to_owned(), None)]
    );

    // A reply is the same send with the parent threaded through.
    handle.send("11111", "and this", Some("M1")).await.unwrap();
    assert_eq!(
        api.sent.lock().unwrap()[1],
        (
            "11111".to_owned(),
            "and this".to_owned(),
            Some("M1".to_owned())
        )
    );

    handle.shutdown().await;
}

/// Switching channels is one delete and one create, not a socket teardown.
/// Whispers belong to the account, so the subscription is made once per socket
/// session and must survive every channel switch — the per-channel reconcile
/// would delete it if it were listed there.
#[tokio::test]
async fn whispers_are_subscribed_once_per_session_and_survive_a_switch() {
    let (url, _server) = fake_server(vec![vec![welcome("S1", 10), "__HANG__".to_owned()]]).await;

    let api = Arc::new(FakeApi::default());
    let (handle, _rx) = start(&url, Arc::clone(&api));

    handle
        .set_channels(vec![UserId("11111".into())])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    handle
        .set_channels(vec![UserId("99999".into())])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(*api.whisper_subs.lock().unwrap(), 1);
    assert_eq!(
        *api.own_stream_subs.lock().unwrap(),
        1,
        "the reader's own live state is subscribed once per session too"
    );
    assert!(
        !api.deleted
            .lock()
            .unwrap()
            .contains(&"whisper-sub".to_owned()),
        "a channel switch must not drop the whisper subscription"
    );

    handle.shutdown().await;
}

#[tokio::test]
async fn switching_channels_swaps_one_subscription() {
    let (url, _server) = fake_server(vec![vec![welcome("S1", 10), "__HANG__".to_owned()]]).await;

    let api = Arc::new(FakeApi::default());
    let (handle, _rx) = start(&url, Arc::clone(&api));

    handle
        .set_channels(vec![UserId("11111".into())])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    handle
        .set_channels(vec![UserId("99999".into())])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let per_channel = lurkn_chat::eventsub::subs::CHANNEL_SUBSCRIPTIONS.len();
    let created = api.created.lock().unwrap().clone();

    assert_eq!(
        created.len(),
        per_channel * 2,
        "both channels subscribed in full"
    );
    assert!(created[..per_channel].iter().all(|(_, b, _)| b == "11111"));
    assert!(created[per_channel..].iter().all(|(_, b, _)| b == "99999"));
    assert_eq!(
        api.deleted.lock().unwrap().len(),
        per_channel,
        "leaving a channel drops all of its subscriptions"
    );

    handle.shutdown().await;
}

/// A subscription type Twitch rejects must not cost the user chat.
///
/// Regression: `channel.chat_settings.update` was first spelt
/// `channel.chat.settings.update`, Twitch answered 400, and the socket
/// reconnected forever without ever delivering a message.
#[tokio::test]
async fn a_rejected_optional_subscription_still_leaves_chat_working() {
    #[derive(Debug, Default)]
    struct PickyApi {
        created: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ChatApi for PickyApi {
        fn viewer_id(&self) -> String {
            "22222".into()
        }

        async fn create_chat_subscription(
            &self,
            subscription_type: &str,
            _broadcaster_id: &str,
            _session_id: &str,
        ) -> Result<String, ChatError> {
            // Stand in for Twitch refusing everything but the message feed.
            if subscription_type != lurkn_chat::eventsub::subs::CHAT_MESSAGE_TYPE {
                return Err(ChatError::Rejected(
                    "400 Bad Request: invalid subscription type and version".into(),
                ));
            }
            self.created
                .lock()
                .unwrap()
                .push(subscription_type.to_owned());
            Ok("sub-msg".to_owned())
        }

        async fn delete_subscription(&self, _id: &str) -> Result<(), ChatError> {
            Ok(())
        }

        async fn chat_settings(&self, _broadcaster_id: &str) -> Result<RoomState, ChatError> {
            Ok(RoomState::default())
        }

        async fn create_own_stream_subscriptions(&self, _s: &str) -> Result<(), ChatError> {
            Err(ChatError::Unauthorized)
        }

        /// Whispers need an optional scope, so a refusal here is the common
        /// case and must never take chat down with it.
        async fn create_whisper_subscription(&self, _s: &str) -> Result<String, ChatError> {
            Err(ChatError::Unauthorized)
        }

        async fn send_whisper(&self, _to: &str, _text: &str) -> Result<(), ChatError> {
            Err(ChatError::Unauthorized)
        }

        async fn send_chat_message(
            &self,
            _b: &str,
            _t: &str,
            _r: Option<&str>,
        ) -> Result<(), ChatError> {
            Ok(())
        }
    }

    let (url, _server) = fake_server(vec![vec![
        welcome("S1", 10),
        chat_notification("m1", "still working"),
    ]])
    .await;

    let api = Arc::new(PickyApi::default());
    let (tx, mut rx) = broadcast::channel(64);
    let handle = lurkn_chat::eventsub::socket::spawn_with_url(
        Arc::clone(&api) as Arc<dyn ChatApi>,
        tx,
        url.to_owned(),
    );
    handle
        .set_channels(vec![UserId("11111".into())])
        .await
        .unwrap();

    let message = wait_for_message(&mut rx, Duration::from_secs(5))
        .await
        .expect("messages must still arrive when only the optional subscriptions fail");
    assert_eq!(message.raw_text, "still working");
    assert_eq!(
        api.created.lock().unwrap().len(),
        1,
        "only the message feed subscribed"
    );

    handle.shutdown().await;
}

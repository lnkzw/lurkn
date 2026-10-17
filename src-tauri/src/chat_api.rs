//! Helix behind the [`ChatApi`] trait.
//!
//! Lives here rather than in `lurkn-chat` so that crate stays free of Helix, and
//! rather than in `lurkn-twitch` so there is no cycle. See `docs/architecture.md` §5.

use std::sync::Arc;

use async_trait::async_trait;
use lurkn_auth::Session;
use lurkn_chat::api::ChatApi;
use lurkn_chat::transport::ChatError;
use lurkn_twitch::endpoints;

#[derive(Debug, Clone)]
pub struct HelixChatApi {
    http: reqwest::Client,
    session: Arc<Session>,
    client_id: String,
    viewer_id: String,
}

impl HelixChatApi {
    pub fn new(
        http: reqwest::Client,
        session: Arc<Session>,
        client_id: String,
        viewer_id: String,
    ) -> Self {
        Self {
            http,
            session,
            client_id,
            viewer_id,
        }
    }

    async fn token(&self) -> Result<String, ChatError> {
        self.session.access_token().await.map_err(|error| {
            if error.requires_reauth() {
                ChatError::Unauthorized
            } else {
                ChatError::Transport(error.to_string())
            }
        })
    }

    /// Turn a Helix response into a `ChatError`, refreshing once on a 401 is the
    /// caller's job — chat requests are infrequent enough to surface it instead.
    async fn check(response: reqwest::Response) -> Result<serde_json::Value, ChatError> {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            return Ok(serde_json::from_str(&body).unwrap_or(serde_json::Value::Null));
        }

        Err(match status.as_u16() {
            401 => ChatError::Unauthorized,
            429 => ChatError::RateLimited(30),
            _ => {
                let message = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_owned))
                    .unwrap_or(body);
                ChatError::Rejected(format!("{status}: {message}"))
            }
        })
    }
}

#[async_trait]
impl ChatApi for HelixChatApi {
    fn viewer_id(&self) -> String {
        self.viewer_id.clone()
    }

    async fn create_chat_subscription(
        &self,
        subscription_type: &str,
        broadcaster_id: &str,
        session_id: &str,
    ) -> Result<String, ChatError> {
        let token = self.token().await?;
        let body = lurkn_chat::eventsub::subs::channel_body(
            subscription_type,
            broadcaster_id,
            &self.viewer_id,
            session_id,
        );

        let response = self
            .http
            .post(endpoints::eventsub_subscriptions())
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| ChatError::Transport(e.to_string()))?;

        let json = Self::check(response).await?;
        json.get("data")
            .and_then(|d| d.get(0))
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| ChatError::Rejected("Twitch did not return a subscription id".into()))
    }

    async fn send_whisper(&self, to_user_id: &str, text: &str) -> Result<(), ChatError> {
        let token = self.token().await?;

        let response = self
            .http
            .post(endpoints::send_whisper(&self.viewer_id, to_user_id))
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({ "message": text }))
            .send()
            .await
            .map_err(|e| ChatError::Transport(e.to_string()))?;

        // 204 No Content on success, so there is nothing to parse — only a
        // status to check. `check` expects a body, hence the separate path.
        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_owned))
            .unwrap_or(body);

        Err(match status.as_u16() {
            401 => ChatError::Unauthorized,
            429 => ChatError::RateLimited(60),
            // Twitch's own wording is carried through: the causes here are an
            // unverified phone, a recipient who blocks whispers, and a daily
            // cap, and its message distinguishes them better than a guess from
            // the status code would.
            _ => ChatError::Rejected(format!("{status}: {message}")),
        })
    }

    async fn create_own_stream_subscriptions(&self, session_id: &str) -> Result<(), ChatError> {
        let token = self.token().await?;

        for kind in lurkn_chat::eventsub::subs::OWN_STREAM_SUBSCRIPTIONS {
            let body =
                lurkn_chat::eventsub::subs::broadcaster_body(kind, &self.viewer_id, session_id);

            let response = self
                .http
                .post(endpoints::eventsub_subscriptions())
                .header("Client-Id", &self.client_id)
                .header("Authorization", format!("Bearer {token}"))
                .json(&body)
                .send()
                .await
                .map_err(|e| ChatError::Transport(e.to_string()))?;

            Self::check(response).await?;
        }

        Ok(())
    }

    async fn create_whisper_subscription(&self, session_id: &str) -> Result<String, ChatError> {
        let token = self.token().await?;
        let body = lurkn_chat::eventsub::subs::whisper_body(&self.viewer_id, session_id);

        let response = self
            .http
            .post(endpoints::eventsub_subscriptions())
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| ChatError::Transport(e.to_string()))?;

        let json = Self::check(response).await?;
        json.get("data")
            .and_then(|d| d.get(0))
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| ChatError::Rejected("Twitch did not return a subscription id".into()))
    }

    async fn delete_subscription(&self, subscription_id: &str) -> Result<(), ChatError> {
        let token = self.token().await?;
        let mut url = endpoints::eventsub_subscriptions();
        url.query_pairs_mut().append_pair("id", subscription_id);

        let response = self
            .http
            .delete(url)
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(|e| ChatError::Transport(e.to_string()))?;

        Self::check(response).await.map(|_| ())
    }

    async fn chat_settings(
        &self,
        broadcaster_id: &str,
    ) -> Result<lurkn_core::RoomState, ChatError> {
        let token = self.token().await?;

        let response = self
            .http
            .get(endpoints::chat_settings(broadcaster_id))
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(|e| ChatError::Transport(e.to_string()))?;

        let json = Self::check(response).await?;
        let entry = json
            .get("data")
            .and_then(|d| d.get(0))
            .cloned()
            .unwrap_or_default();

        let flag = |key: &str| {
            entry
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        let number = |key: &str| {
            entry
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32)
        };

        Ok(lurkn_core::RoomState {
            emote_only: flag("emote_mode"),
            subscriber_only: flag("subscriber_mode"),
            unique_only: flag("unique_chat_mode"),
            // The duration is only meaningful while the mode is on.
            follower_only_minutes: flag("follower_mode")
                .then(|| number("follower_mode_duration").unwrap_or(0)),
            slow_mode_seconds: flag("slow_mode")
                .then(|| number("slow_mode_wait_time").unwrap_or(30)),
        })
    }

    async fn send_chat_message(
        &self,
        broadcaster_id: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<(), ChatError> {
        let token = self.token().await?;

        let mut body = serde_json::json!({
            "broadcaster_id": broadcaster_id,
            "sender_id": self.viewer_id,
            "message": text,
        });

        // Omitted entirely rather than sent as null: Twitch rejects an empty
        // parent id, and most messages are not replies.
        if let Some(parent) = reply_to {
            body["reply_parent_message_id"] = serde_json::Value::String(parent.to_owned());
        }

        let response = self
            .http
            .post(endpoints::send_chat_message())
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| ChatError::Transport(e.to_string()))?;

        let json = Self::check(response).await?;

        // Twitch answers 200 even when it drops the message, e.g. for a channel
        // in followers-only mode. The real verdict is inside the payload.
        let entry = json.get("data").and_then(|d| d.get(0));
        let sent = entry
            .and_then(|e| e.get("is_sent"))
            .and_then(serde_json::Value::as_bool);

        if sent == Some(false) {
            let reason = entry
                .and_then(|e| e.get("drop_reason"))
                .and_then(|d| d.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Twitch did not deliver the message");
            return Err(ChatError::Rejected(reason.to_owned()));
        }

        Ok(())
    }
}

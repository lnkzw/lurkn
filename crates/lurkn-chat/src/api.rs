//! What the chat socket needs from Helix, as a trait.
//!
//! Declared here rather than depending on `lurkn-twitch` so the socket can be
//! driven by a fake in tests, and so no crate cycle is needed. The real
//! implementation lives in the Tauri layer, which already owns the Helix client.

use async_trait::async_trait;

use crate::transport::ChatError;

#[async_trait]
pub trait ChatApi: Send + Sync + std::fmt::Debug {
    /// The authorising user. Required as the `user_id` condition, and it is what
    /// makes a `channel.chat.message` subscription cost 0.
    fn viewer_id(&self) -> String;

    /// `POST helix/eventsub/subscriptions`. Returns the new subscription id.
    async fn create_chat_subscription(
        &self,
        subscription_type: &str,
        broadcaster_id: &str,
        session_id: &str,
    ) -> Result<String, ChatError>;

    /// `GET helix/chat/settings`. EventSub only reports *changes*, so the state
    /// at join time has to be asked for.
    async fn chat_settings(&self, broadcaster_id: &str)
        -> Result<lurkn_core::RoomState, ChatError>;

    /// `POST helix/whispers`. Answers 204 with no body on success.
    ///
    /// Fails for reasons the user can act on and which differ from each other —
    /// an unverified phone number, a recipient who has turned whispers off, a
    /// daily cap — so callers map the error rather than printing one message.
    async fn send_whisper(&self, to_user_id: &str, text: &str) -> Result<(), ChatError>;

    /// Subscribe to the authorising user's own stream going on and off air.
    ///
    /// Needs no scope, so unlike the whisper subscription a failure here is a
    /// real problem rather than a user who has not opted in — it is still not
    /// fatal, because chat works without it.
    async fn create_own_stream_subscriptions(&self, session_id: &str) -> Result<(), ChatError>;

    /// Subscribe to whispers for the authorising user.
    ///
    /// Separate from the chat subscription call because the condition differs
    /// and because it is allowed to fail: the scope is optional, so a user who
    /// has not opted in gets a 401 here and full chat everywhere else.
    async fn create_whisper_subscription(&self, session_id: &str) -> Result<String, ChatError>;

    /// `DELETE helix/eventsub/subscriptions?id=`
    async fn delete_subscription(&self, subscription_id: &str) -> Result<(), ChatError>;

    /// `POST helix/chat/messages`
    ///
    /// `reply_to` threads the message under another, which Twitch supports on
    /// the same `user:write:chat` scope as an ordinary send.
    async fn send_chat_message(
        &self,
        broadcaster_id: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> Result<(), ChatError>;
}

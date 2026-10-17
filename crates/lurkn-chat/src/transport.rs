use async_trait::async_trait;
use lurkn_core::{ChatMessage, Login};

/// The one seam between chat plumbing and everything above it.
///
/// `join` / `part` describe intent, not wire protocol: under EventSub they are a
/// subscription create and delete. Nothing above this trait sees an envelope.
#[async_trait]
pub trait ChatTransport: Send + Sync {
    async fn connect(&mut self) -> Result<(), ChatError>;
    async fn join(&mut self, channel: &Login) -> Result<(), ChatError>;
    async fn part(&mut self, channel: &Login) -> Result<(), ChatError>;
    async fn send(&mut self, channel: &Login, text: &str) -> Result<(), ChatError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatConnState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting {
        attempt: u32,
    },
    /// Terminal for this session: the subscription was revoked, or the token is
    /// no longer valid. Surfaced as an auth error, never silently retried.
    Failed {
        reason: String,
    },
}

/// What the rest of the app sees. Transport-agnostic by construction.
#[derive(Debug, Clone)]
pub enum ChatEvent {
    State(ChatConnState),
    Message(Box<ChatMessage>),
    /// A moderator cleared the whole chat.
    Cleared {
        channel_id: String,
    },
    /// One user's messages were removed, e.g. a timeout or ban.
    UserCleared {
        channel_id: String,
        user_id: String,
        user_login: String,
    },
    /// A single message was removed.
    Deleted {
        channel_id: String,
        message_id: String,
        user_login: String,
    },
    /// The reader's own stream started or stopped.
    OwnStream {
        live: bool,
    },
    /// A whisper arrived. Not tied to a channel: it belongs to the account.
    Whisper(Box<lurkn_core::Whisper>),
    /// A channel raided another. Only ever one Lurkn has joined, as the raider.
    Raided(Box<crate::eventsub::raid::Raid>),
    /// Followers-only, slow mode and friends changed.
    RoomStateChanged {
        channel_id: String,
        state: lurkn_core::RoomState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatError {
    #[error("chat transport not connected")]
    NotConnected,
    #[error("subscription revoked: {0}")]
    Revoked(String),
    #[error("rate limited, retry in {0}s")]
    RateLimited(u64),
    #[error("the Twitch session is no longer valid")]
    Unauthorized,
    #[error("could not reach Twitch: {0}")]
    Transport(String),
    #[error("Twitch rejected the request: {0}")]
    Rejected(String),
}

impl ChatError {
    /// Whether the user has to log in again.
    pub fn requires_reauth(&self) -> bool {
        matches!(self, Self::Unauthorized | Self::Revoked(_))
    }

    /// Whether reconnecting stands a chance.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport(_) | Self::RateLimited(_) | Self::NotConnected
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AppState` holds a `Box<dyn ChatTransport>`, so the trait must stay
    /// object-safe for the transport to be swappable.
    #[test]
    fn transport_is_object_safe() {
        fn assert_object_safe(_: Option<&dyn ChatTransport>) {}
        assert_object_safe(None);
    }

    #[test]
    fn failed_state_carries_a_reason_for_the_ui() {
        let state = ChatConnState::Failed {
            reason: "token expired".into(),
        };
        assert_ne!(state, ChatConnState::Disconnected);
    }

    #[test]
    fn revocation_and_auth_failures_send_the_user_back_to_login() {
        assert!(ChatError::Unauthorized.requires_reauth());
        assert!(ChatError::Revoked("authorization_revoked".into()).requires_reauth());
        assert!(!ChatError::Transport("dns".into()).requires_reauth());
    }

    /// A revoked subscription must never be retried in a loop.
    #[test]
    fn only_transient_failures_are_retryable() {
        assert!(ChatError::Transport("reset".into()).is_retryable());
        assert!(ChatError::RateLimited(5).is_retryable());
        assert!(!ChatError::Revoked("x".into()).is_retryable());
        assert!(!ChatError::Unauthorized.is_retryable());
    }
}

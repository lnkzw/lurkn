//! Whispers: private messages, threaded by the other person.
//!
//! Twitch publishes no whisper history. EventSub delivers them live and there is
//! no endpoint that returns what was missed, so anything sent while Lurkn was
//! closed is not recoverable — the local store is the whole record. The UI says
//! so rather than presenting a partial thread as a complete one.

use serde::{Deserialize, Serialize};
use specta::Type;
use time::OffsetDateTime;

use crate::ids::{Login, UserId};

/// One whisper, in either direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Whisper {
    pub id: String,
    /// The other party, whichever way the message went. Threads are keyed on it.
    pub peer_id: UserId,
    pub peer_login: Login,
    /// UNTRUSTED.
    pub peer_name: String,
    /// True when this account sent it.
    pub outgoing: bool,
    /// UNTRUSTED. Rendered as text, never as markup.
    pub body: String,
    #[serde(with = "time::serde::rfc3339")]
    #[specta(type = String)]
    pub sent_at: OffsetDateTime,
    pub read: bool,
}

/// One conversation, for the thread list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WhisperThread {
    pub peer_id: UserId,
    pub peer_login: Login,
    /// UNTRUSTED.
    pub peer_name: String,
    /// The most recent message, for the preview line.
    pub last_body: String,
    pub last_outgoing: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[specta(type = String)]
    pub last_at: OffsetDateTime,
    pub unread: u32,
}

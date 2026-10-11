//! Typed Rust -> frontend events. See `docs/architecture.md` §8.1.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

/// The session changed: logged in, logged out, expired, or a login failed.
/// Carries no token (`docs/architecture.md` §11).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct AuthChanged(pub crate::commands::auth::AuthStatus);

/// The whole sidebar view. Small enough to send entire — a few hundred rows —
/// and far simpler than patching the list in the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct FollowsUpdated(pub crate::commands::follows::FollowsView);

/// Who just went live or offline, for notifications and highlighting.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct LiveDelta {
    pub went_live: Vec<String>,
    pub went_offline: Vec<String>,
}

/// A tick's worth of chat, coalesced so a busy channel does not flood the IPC
/// bridge with one message per event. See `docs/architecture.md` §8.1.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ChatBatch {
    pub messages: Vec<lurkn_core::ChatMessage>,
}

/// Chat connection state, for the "reconnecting…" banner.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ChatStateChanged(pub crate::commands::chat::ChatStatus);

/// A channel's third-party emote sets finished loading. Chat rendered before
/// this point shows emote names as plain text.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct EmotesReady {
    pub channel_id: String,
    pub count: u32,
}

/// A whisper arrived and has been written to the store.
///
/// Carries the unread total so a badge can update without a round trip.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct WhisperReceived {
    pub whisper: lurkn_core::Whisper,
    pub unread: u32,
}

/// Messages from before Lurkn joined, for the top of the log.
///
/// Separate from [`ChatBatch`] because it is prepended rather than appended,
/// and because it may arrive after live messages have already landed.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistory {
    pub channel_id: String,
    pub messages: Vec<lurkn_core::ChatMessage>,
}

/// The open channel when it is not one the reader follows, with the same data
/// a follow carries. `None` when the open channel is a follow, or nothing is.
///
/// Its own event rather than part of the follow list: that list is the
/// reader's follows and nothing else, and folding a stranger into it would
/// make every consumer of the list — the sidebar, the tray, the live
/// notifications — have to filter it back out.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct OpenChannelChanged(pub Option<lurkn_core::FollowedChannel>);

/// The reader's own stream went on or off air.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct OwnStreamChanged(pub bool);

/// The tray asked for the whisper panel. Carries nothing: it is a request to
/// look at something, not a change of state.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ShowWhispers;

/// Settings changed, so every window can follow along.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct SettingsChanged(pub lurkn_core::Settings);

/// Settings over the chat, in the main window: asked for from the tray, or
/// put back there from the settings window, at the group it was showing.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ShowSettings(pub Option<String>);

/// Turn the settings window, already open, to a group: the chat's filter menu
/// asks for Filters.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct SettingsSection(pub String);

/// The theme file in effect changed: picked, edited on disk, or broken.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct ThemeChanged(pub crate::theme::ThemeState);

/// Moderation and room-state changes. Separate from [`ChatBatch`] because these
/// are rare and must be applied in order relative to the messages around them.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ModerationEvent {
    /// The whole chat was cleared.
    Cleared { channel_id: String },
    /// One user's messages were removed — a timeout or a ban.
    UserCleared { user_id: String, user_login: String },
    /// A single message was removed.
    MessageDeleted {
        message_id: String,
        user_login: String,
    },
    /// Followers-only, slow mode and friends changed.
    RoomState { state: lurkn_core::RoomState },
}

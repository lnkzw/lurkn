#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Pure domain types and logic for Lurkn.
//!
//! This crate must not perform IO. See `docs/architecture.md` §1 — the one-way
//! dependency rule is what keeps the rest of the workspace cheap to test.

pub mod chat;
pub mod emote;
pub mod error;
pub mod ids;
pub mod model;
pub mod rules;
pub mod settings;
pub mod theme;
pub mod url;
pub mod whisper;

pub use chat::{
    Badge, ChannelEvent, ChannelEventKind, ChatAuthor, ChatMessage, ChatToken, MessageKind,
    MessageState, Rgb, RoomState,
};
pub use emote::{Emote, EmoteScope, ProviderId};
pub use error::{CoreError, Result};
pub use ids::{Login, MessageId, UserId};
pub use model::{Category, ChannelHit, FollowedChannel, Stream, TwitchChannel, User, UserProfile};
pub use rules::ChatRules;
pub use settings::{
    ActivitySettings, BroadcastSettings, BrowserChoice, BrowserSettings, ChatFilter, ChatFont,
    ChatSettings, CustomCommand, EmoteSize, HighlightBadge, HighlightPhrase, HighlightSettings,
    HighlightUser, IgnoreAction, IgnorePhrase, IgnoreSettings, LaunchMode, NotificationSettings,
    OverlayLayout, OverlaySettings, OverlaySource, Settings, SidebarPosition, SidebarSettings,
    StatusBarPosition, StreamPage, ThemePreference, WindowSettings,
};
pub use url::{twitch_channel_url, twitch_player_url};
pub use whisper::{Whisper, WhisperThread};

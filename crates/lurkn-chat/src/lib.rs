#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Chat transport.
//!
//! EventSub WebSocket for reading, Helix `Send Chat Message` for writing. IRC is
//! deliberately not implemented — see `docs/architecture.md` §5.

pub mod api;
pub mod backoff;
pub mod dedupe;
pub mod eventsub;
pub mod history;
pub mod transport;

pub use api::ChatApi;
pub use dedupe::MessageDedupe;
pub use eventsub::socket::{spawn, ChatHandle};
pub use transport::{ChatConnState, ChatError, ChatEvent, ChatTransport};

/// EventSub WebSocket entry point.
pub const EVENTSUB_WS_URL: &str = "wss://eventsub.wss.twitch.tv/ws";
/// Twitch closes the socket if the first subscription is not created within 10s
/// of `session_welcome`.
pub const SUBSCRIBE_DEADLINE_SECS: u64 = 10;
/// Enabled subscriptions per socket.
pub const MAX_SUBSCRIPTIONS_PER_SOCKET: usize = 300;
/// Concurrent sockets per user access token.
pub const MAX_SOCKETS_PER_TOKEN: usize = 3;

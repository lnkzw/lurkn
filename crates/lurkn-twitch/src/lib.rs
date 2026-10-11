#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Helix API client. All Twitch HTTP lives here — see `docs/architecture.md` §4.

pub mod badges;
pub mod client;
pub mod dto;
pub mod endpoints;
pub mod error;
pub mod follows;
pub mod moderation;
pub mod ratelimit;
pub mod suggest;

pub use badges::{BadgeMap, BadgeService};
pub use client::HelixClient;
pub use endpoints::HELIX_BASE;
pub use error::{HelixError, Result};
pub use follows::FollowsService;

/// Live-status poll interval. Internal by design: not a user setting.
pub const LIVE_POLL_INTERVAL_SECS: u64 = 60;
/// Jitter applied to the poll so many clients do not align on the same second.
pub const LIVE_POLL_JITTER_SECS: u64 = 10;
/// Helix pages top out at 100 items for every endpoint Lurkn uses.
pub const MAX_PAGE_SIZE: u32 = 100;

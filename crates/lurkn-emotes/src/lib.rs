#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Emote providers and name resolution. See `docs/architecture.md` §6.

pub mod fetch;
pub mod hosts;
pub mod precedence;
pub mod provider;
pub mod providers;
pub mod registry;

pub use fetch::EmoteFetcher;
pub use hosts::is_allowed_emote_host;
pub use precedence::{EmoteScope, ProviderId, PRECEDENCE};
pub use provider::EmoteProvider;
pub use registry::EmoteRegistry;

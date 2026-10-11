#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Twitch Device Code Flow, token lifecycle and secure storage.
//!
//! See `docs/architecture.md` §3. Twitch has no PKCE, so DCF as a public client
//! is the only flow that lets a distributed binary avoid shipping a secret.
//!
//! This crate is the only one that talks to `id.twitch.tv`; Helix belongs to
//! `lurkn-twitch`.

pub mod device;
pub mod endpoints;
pub mod error;
pub mod refresh;
pub mod scopes;
pub mod session;
pub mod storage;
pub mod tokens;
pub mod validate;

pub use device::{DeviceFlow, DevicePrompt};
pub use endpoints::IdEndpoints;
pub use error::{AuthError, Result};
pub use scopes::{
    has_moderation, has_whispers, scope_param_with, MODERATION_SCOPES, OPTIONAL_SCOPES,
    REQUIRED_SCOPES,
};
pub use session::{AuthState, Session};
pub use storage::{default_storage, StorageKind, TokenStorage};
pub use tokens::TokenSet;
pub use validate::ValidatedToken;

/// Compiled-in Twitch application client id.
///
/// A client id is a public identifier, not a secret — every native Twitch client
/// ships one. The secret Lurkn never has is the *client secret*, which the
/// device code flow does not require. See `docs/architecture.md` §3.
pub const DEFAULT_CLIENT_ID: &str = "x298sg845rfhwemuvdurx3psnadkz1";

/// Overridable for development against a throwaway app.
pub fn client_id() -> String {
    std::env::var("LURKN_CLIENT_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_id_is_compiled_in() {
        assert!(!DEFAULT_CLIENT_ID.is_empty());
        assert_eq!(
            DEFAULT_CLIENT_ID.len(),
            30,
            "Twitch client ids are 30 characters"
        );
        assert!(
            DEFAULT_CLIENT_ID.bytes().all(|b| b.is_ascii_alphanumeric()),
            "client id should be alphanumeric"
        );
    }
}

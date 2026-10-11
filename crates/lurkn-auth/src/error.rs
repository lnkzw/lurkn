use thiserror::Error;

pub type Result<T> = std::result::Result<T, AuthError>;

/// Outcomes of the device-code flow and token lifecycle.
///
/// Twitch does **not** follow RFC 8628 error shapes: instead of
/// `{"error":"authorization_pending"}` it returns
/// `{"status":400,"message":"authorization_pending"}`. Classification therefore
/// keys on `message`. See [`crate::device::classify`].
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum AuthError {
    /// The user has not finished approving yet. Keep polling.
    #[error("authorization still pending")]
    AuthorizationPending,

    /// Twitch asked us to back off. Increase the interval, keep polling.
    #[error("polling too fast")]
    SlowDown,

    /// The device code aged out before the user approved it (30 minutes).
    #[error("the login code expired before it was used")]
    DeviceCodeExpired,

    /// The user explicitly declined.
    #[error("login was declined")]
    Denied,

    /// The refresh token is gone: either revoked, or past the 30-day public-client
    /// ceiling. The user must log in again.
    #[error("the saved Twitch session is no longer valid")]
    RefreshRejected,

    /// Twitch granted a token, but not everything Lurkn asked for. Continuing
    /// would produce confusing 401s later, so this fails loudly at login instead.
    #[error("Twitch did not grant the required permissions: {}", missing.join(", "))]
    ScopesNotGranted { missing: Vec<String> },

    #[error("no Twitch client id is configured")]
    NoClientId,

    /// Anything else Twitch said, kept verbatim for the log view.
    #[error("Twitch returned {status}: {message}")]
    Twitch { status: u16, message: String },

    #[error("could not reach Twitch: {0}")]
    Transport(String),

    #[error("could not read or write the saved session: {0}")]
    Storage(String),
}

impl AuthError {
    /// Whether the polling loop should continue after this error.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::AuthorizationPending | Self::SlowDown)
    }

    /// Whether the user has to go through the login flow again.
    pub fn requires_reauth(&self) -> bool {
        matches!(
            self,
            Self::RefreshRejected
                | Self::DeviceCodeExpired
                | Self::Denied
                | Self::ScopesNotGranted { .. }
        )
    }
}

impl From<reqwest::Error> for AuthError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_variants_keep_the_loop_going() {
        assert!(AuthError::AuthorizationPending.is_pending());
        assert!(AuthError::SlowDown.is_pending());
        assert!(!AuthError::Denied.is_pending());
        assert!(!AuthError::DeviceCodeExpired.is_pending());
    }

    #[test]
    fn terminal_variants_send_the_user_back_to_login() {
        assert!(AuthError::RefreshRejected.requires_reauth());
        assert!(AuthError::Denied.requires_reauth());
        assert!(AuthError::ScopesNotGranted {
            missing: vec!["user:read:chat".into()]
        }
        .requires_reauth());
        assert!(!AuthError::AuthorizationPending.requires_reauth());
        assert!(!AuthError::Transport("dns".into()).requires_reauth());
    }

    #[test]
    fn missing_scopes_are_named_in_the_message() {
        let error = AuthError::ScopesNotGranted {
            missing: vec!["user:read:chat".into(), "user:write:chat".into()],
        };
        assert_eq!(
            error.to_string(),
            "Twitch did not grant the required permissions: user:read:chat, user:write:chat"
        );
    }
}

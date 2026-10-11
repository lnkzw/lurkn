use thiserror::Error;

pub type Result<T> = std::result::Result<T, HelixError>;

#[derive(Debug, Error)]
pub enum HelixError {
    /// The token is gone and refreshing did not help. The user must log in again.
    #[error("the Twitch session is no longer valid")]
    Unauthorized,

    /// Twitch's point bucket is empty. Carries the seconds until it refills.
    #[error("rate limited by Twitch; retry in {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Twitch returned {status}: {message}")]
    Twitch { status: u16, message: String },

    #[error("could not reach Twitch: {0}")]
    Transport(String),

    #[error("could not read Twitch's response: {0}")]
    Malformed(String),

    #[error(transparent)]
    Auth(#[from] lurkn_auth::AuthError),
}

impl From<reqwest::Error> for HelixError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error.to_string())
    }
}

impl HelixError {
    /// Whether the UI should send the user back to the login screen.
    pub fn requires_reauth(&self) -> bool {
        match self {
            Self::Unauthorized => true,
            Self::Auth(e) => e.requires_reauth(),
            _ => false,
        }
    }

    /// Whether serving stale cache is the right response.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::RateLimited { .. })
            || matches!(self, Self::Twitch { status, .. } if *status >= 500)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_sends_the_user_back_to_login() {
        assert!(HelixError::Unauthorized.requires_reauth());
        assert!(!HelixError::Transport("dns".into()).requires_reauth());
    }

    #[test]
    fn network_and_server_faults_are_transient() {
        assert!(HelixError::Transport("dns".into()).is_transient());
        assert!(HelixError::RateLimited {
            retry_after_secs: 30
        }
        .is_transient());
        assert!(HelixError::Twitch {
            status: 503,
            message: "down".into()
        }
        .is_transient());
        // A 400 is our bug, not weather: do not paper over it with stale cache.
        assert!(!HelixError::Twitch {
            status: 400,
            message: "bad".into()
        }
        .is_transient());
        assert!(!HelixError::Unauthorized.is_transient());
    }
}

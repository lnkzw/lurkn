use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

/// Refresh once this fraction of the access token's lifetime has elapsed, so a
/// user action never lands on an expired token. See `docs/architecture.md` §3.4.
const REFRESH_AT_FRACTION: f64 = 0.8;

/// Public-client refresh tokens are invalidated 30 days after they are minted and
/// cannot be extended by refreshing. After that the user must redo the DCF.
pub const REFRESH_TOKEN_LIFETIME_DAYS: i64 = 30;

#[derive(Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    pub obtained_at: OffsetDateTime,
    pub expires_in: Duration,
    pub scopes: Vec<String>,
}

// Hand-written so a token can never be printed by an accidental `{:?}`.
impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("obtained_at", &self.obtained_at)
            .field("expires_in", &self.expires_in)
            .field("scopes", &self.scopes)
            .finish()
    }
}

impl TokenSet {
    pub fn expires_at(&self) -> OffsetDateTime {
        self.obtained_at + self.expires_in
    }

    /// When to proactively refresh: 80% into the access token's lifetime.
    pub fn refresh_at(&self) -> OffsetDateTime {
        let lifetime = self.expires_in.as_seconds_f64() * REFRESH_AT_FRACTION;
        self.obtained_at + Duration::seconds_f64(lifetime)
    }

    pub fn needs_refresh(&self, now: OffsetDateTime) -> bool {
        now >= self.refresh_at()
    }

    /// The hard wall: past this, refreshing fails and the user must log in again.
    pub fn reauth_required_at(&self) -> OffsetDateTime {
        self.obtained_at + Duration::days(REFRESH_TOKEN_LIFETIME_DAYS)
    }

    pub fn is_beyond_refresh_window(&self, now: OffsetDateTime) -> bool {
        now >= self.reauth_required_at()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn token() -> TokenSet {
        TokenSet {
            access_token: "ACCESS-SECRET-VALUE".into(),
            refresh_token: "REFRESH-SECRET-VALUE".into(),
            obtained_at: datetime!(2026-09-20 12:00:00 UTC),
            // What Twitch actually returns for DCF: 14820s, a shade over 4h.
            expires_in: Duration::seconds(14820),
            scopes: vec!["user:read:follows".into()],
        }
    }

    #[test]
    fn refreshes_at_eighty_percent_of_lifetime() {
        let t = token();
        assert_eq!(t.expires_at(), datetime!(2026-09-20 16:07:00 UTC));
        assert_eq!(t.refresh_at(), datetime!(2026-09-20 15:17:36 UTC));
        assert!(
            t.refresh_at() < t.expires_at(),
            "must refresh before expiry"
        );
    }

    #[test]
    fn needs_refresh_flips_at_the_threshold() {
        let t = token();
        assert!(!t.needs_refresh(datetime!(2026-09-20 15:17:35 UTC)));
        assert!(t.needs_refresh(datetime!(2026-09-20 15:17:36 UTC)));
    }

    #[test]
    fn thirty_day_wall_is_measured_from_mint_time() {
        let t = token();
        assert_eq!(t.reauth_required_at(), datetime!(2026-10-20 12:00:00 UTC));
        assert!(!t.is_beyond_refresh_window(datetime!(2026-10-20 11:59:59 UTC)));
        assert!(t.is_beyond_refresh_window(datetime!(2026-10-20 12:00:00 UTC)));
    }

    #[test]
    fn debug_never_leaks_tokens() {
        let rendered = format!("{:?}", token());
        assert!(
            !rendered.contains("ACCESS-SECRET-VALUE"),
            "access token leaked: {rendered}"
        );
        assert!(
            !rendered.contains("REFRESH-SECRET-VALUE"),
            "refresh token leaked: {rendered}"
        );
        assert!(rendered.contains("<redacted>"));
    }
}

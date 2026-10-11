use serde::Deserialize;
use time::{Duration, OffsetDateTime};
use url::Url;

use crate::endpoints::{IdEndpoints, GRANT_DEVICE_CODE};
use crate::error::{AuthError, Result};
use crate::scopes::scope_param_with;
use crate::tokens::TokenSet;

/// What the user is shown: a short code and where to type it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePrompt {
    pub user_code: String,
    pub verification_uri: Url,
    pub expires_at: OffsetDateTime,
    pub interval: Duration,
    /// Opaque; never shown to the user, never logged.
    pub device_code: String,
}

#[derive(Debug, Deserialize)]
struct DeviceResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    scope: Vec<String>,
}

/// Twitch's error envelope: `{"status":400,"message":"authorization_pending"}`.
#[derive(Debug, Deserialize)]
struct TwitchError {
    status: u16,
    message: String,
}

/// Map a Twitch identity error onto an [`AuthError`].
///
/// Twitch does not use RFC 8628's `error` field, and its messages are prose
/// ("invalid device code") rather than codes, so matching is case-insensitive and
/// substring-based. Verified against live responses on 2026-09-20.
pub fn classify(status: u16, message: &str) -> AuthError {
    let m = message.to_ascii_lowercase();

    if m.contains("authorization_pending") || m.contains("authorization pending") {
        AuthError::AuthorizationPending
    } else if m.contains("slow_down") || m.contains("slow down") {
        AuthError::SlowDown
    } else if m.contains("expired") {
        // Covers "expired device code" and "token expired".
        AuthError::DeviceCodeExpired
    } else if m.contains("denied") || m.contains("declined") || m.contains("abort") {
        AuthError::Denied
    } else if m.contains("invalid device code") {
        // Twitch also returns this once a device code has been consumed.
        AuthError::DeviceCodeExpired
    } else if m.contains("invalid refresh token") {
        AuthError::RefreshRejected
    } else {
        AuthError::Twitch {
            status,
            message: message.to_owned(),
        }
    }
}

/// Verify Twitch granted everything we asked for.
///
/// Both `scope` and `scopes` are accepted by the device endpoint, so a silently
/// unscoped token is a real failure mode. Catching it at login beats debugging
/// 401s three screens later.
pub fn check_granted_scopes(granted: &[String], required: &[&str]) -> Result<()> {
    let missing: Vec<String> = required
        .iter()
        .filter(|r| !granted.iter().any(|g| g.eq_ignore_ascii_case(r)))
        .map(|r| (*r).to_owned())
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(AuthError::ScopesNotGranted { missing })
    }
}

#[derive(Debug, Clone)]
pub struct DeviceFlow {
    http: reqwest::Client,
    endpoints: IdEndpoints,
    client_id: String,
}

impl DeviceFlow {
    pub fn new(http: reqwest::Client, endpoints: IdEndpoints, client_id: String) -> Self {
        Self {
            http,
            endpoints,
            client_id,
        }
    }

    async fn parse<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        let status = response.status();
        let body = response.text().await.map_err(AuthError::from)?;

        if status.is_success() {
            return serde_json::from_str(&body).map_err(|e| AuthError::Twitch {
                status: status.as_u16(),
                message: format!("unreadable response: {e}"),
            });
        }

        Err(match serde_json::from_str::<TwitchError>(&body) {
            Ok(err) => classify(err.status, &err.message),
            Err(_) => AuthError::Twitch {
                status: status.as_u16(),
                message: body,
            },
        })
    }

    /// Step 1: ask Twitch for a code to show the user.
    ///
    /// `extra` names optional scopes to add to the consent screen — features the
    /// user has opted into. Unknown names are dropped rather than forwarded.
    pub async fn start(&self, extra: &[&str]) -> Result<DevicePrompt> {
        if self.client_id.is_empty() {
            return Err(AuthError::NoClientId);
        }

        let scopes = scope_param_with(extra);
        let response = self
            .http
            .post(self.endpoints.device())
            // Twitch names this `scopes`, not OAuth's usual `scope`.
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scopes", scopes.as_str()),
            ])
            .send()
            .await?;

        let device: DeviceResponse = Self::parse(response).await?;
        let verification_uri =
            Url::parse(&device.verification_uri).map_err(|_| AuthError::Twitch {
                status: 200,
                message: format!("unusable verification_uri: {}", device.verification_uri),
            })?;

        Ok(DevicePrompt {
            user_code: device.user_code,
            verification_uri,
            expires_at: OffsetDateTime::now_utc() + Duration::seconds(device.expires_in),
            interval: Duration::seconds(device.interval.max(1)),
            device_code: device.device_code,
        })
    }

    /// Step 2, called on the prompt's interval until it stops returning
    /// [`AuthError::AuthorizationPending`].
    pub async fn poll_once(&self, device_code: &str) -> Result<TokenSet> {
        let response = self
            .http
            .post(self.endpoints.token())
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code),
                ("grant_type", GRANT_DEVICE_CODE),
            ])
            .send()
            .await?;

        let token: TokenResponse = Self::parse(response).await?;
        check_granted_scopes(&token.scope, crate::scopes::REQUIRED_SCOPES)?;

        Ok(TokenSet {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            obtained_at: OffsetDateTime::now_utc(),
            expires_in: Duration::seconds(token.expires_in),
            scopes: token.scope,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact strings captured from live Twitch responses on 2026-09-20.
    #[test]
    fn classifies_live_twitch_messages() {
        assert_eq!(
            classify(400, "authorization_pending"),
            AuthError::AuthorizationPending
        );
        assert_eq!(
            classify(400, "invalid device code"),
            AuthError::DeviceCodeExpired
        );
        assert_eq!(
            classify(400, "Invalid refresh token"),
            AuthError::RefreshRejected
        );
        assert_eq!(
            classify(401, "missing authorization token"),
            AuthError::Twitch {
                status: 401,
                message: "missing authorization token".into()
            }
        );
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(
            classify(400, "AUTHORIZATION_PENDING"),
            AuthError::AuthorizationPending
        );
        assert_eq!(classify(400, "Slow_Down"), AuthError::SlowDown);
    }

    #[test]
    fn recognises_expiry_and_refusal() {
        assert_eq!(
            classify(400, "expired device code"),
            AuthError::DeviceCodeExpired
        );
        assert_eq!(classify(400, "user denied the request"), AuthError::Denied);
    }

    #[test]
    fn unknown_messages_are_preserved_verbatim_for_the_log() {
        let err = classify(500, "something new from Twitch");
        assert_eq!(
            err,
            AuthError::Twitch {
                status: 500,
                message: "something new from Twitch".into()
            }
        );
    }

    #[test]
    fn granted_scopes_must_cover_everything_required() {
        let all: Vec<String> = crate::scopes::REQUIRED_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert!(check_granted_scopes(&all, crate::scopes::REQUIRED_SCOPES).is_ok());

        // Extra scopes are fine; missing ones are not.
        let mut extra = all.clone();
        extra.push("user:read:emotes".into());
        assert!(check_granted_scopes(&extra, crate::scopes::REQUIRED_SCOPES).is_ok());
    }

    /// The failure mode this exists for: Twitch accepts both `scope` and `scopes`
    /// on the device endpoint, so a token can come back with none at all.
    #[test]
    fn an_unscoped_token_is_rejected_at_login() {
        let err = check_granted_scopes(&[], crate::scopes::REQUIRED_SCOPES).unwrap_err();
        assert_eq!(
            err,
            AuthError::ScopesNotGranted {
                missing: vec![
                    "user:read:follows".into(),
                    "user:read:chat".into(),
                    "user:write:chat".into(),
                ]
            }
        );
        assert!(err.requires_reauth());
    }

    #[test]
    fn partially_granted_scopes_name_only_what_is_missing() {
        let granted = vec!["user:read:follows".to_owned()];
        let err = check_granted_scopes(&granted, crate::scopes::REQUIRED_SCOPES).unwrap_err();
        assert_eq!(
            err,
            AuthError::ScopesNotGranted {
                missing: vec!["user:read:chat".into(), "user:write:chat".into()]
            }
        );
    }

    #[test]
    fn scope_comparison_ignores_case() {
        let granted = vec![
            "USER:READ:FOLLOWS".to_owned(),
            "User:Read:Chat".to_owned(),
            "user:write:chat".to_owned(),
        ];
        assert!(check_granted_scopes(&granted, crate::scopes::REQUIRED_SCOPES).is_ok());
    }
}

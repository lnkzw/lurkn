use serde::Deserialize;
use time::{Duration, OffsetDateTime};

use crate::device::{check_granted_scopes, classify};
use crate::endpoints::{IdEndpoints, GRANT_REFRESH_TOKEN};
use crate::error::{AuthError, Result};
use crate::tokens::TokenSet;

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    scope: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TwitchError {
    status: u16,
    message: String,
}

/// Exchange a refresh token for a fresh access token.
///
/// No `client_secret` is sent: Lurkn is registered as a public client, and
/// Twitch accepts the refresh without one (verified 2026-09-20). Note that Twitch
/// **rotates** the refresh token, so the returned one must replace the stored one.
pub async fn refresh(
    http: &reqwest::Client,
    endpoints: &IdEndpoints,
    client_id: &str,
    refresh_token: &str,
) -> Result<TokenSet> {
    if client_id.is_empty() {
        return Err(AuthError::NoClientId);
    }

    let response = http
        .post(endpoints.token())
        .form(&[
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", GRANT_REFRESH_TOKEN),
        ])
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        return Err(match serde_json::from_str::<TwitchError>(&body) {
            Ok(err) => classify(err.status, &err.message),
            // A refresh that fails for an unreadable reason still means the saved
            // session is unusable; treat it as re-auth rather than a silent stall.
            Err(_) if status.as_u16() == 400 || status.as_u16() == 401 => {
                AuthError::RefreshRejected
            }
            Err(_) => AuthError::Twitch {
                status: status.as_u16(),
                message: body,
            },
        });
    }

    let token: RefreshResponse = serde_json::from_str(&body).map_err(|e| AuthError::Twitch {
        status: status.as_u16(),
        message: format!("unreadable refresh response: {e}"),
    })?;

    check_granted_scopes(&token.scope, crate::scopes::REQUIRED_SCOPES)?;

    Ok(TokenSet {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        obtained_at: OffsetDateTime::now_utc(),
        expires_in: Duration::seconds(token.expires_in),
        scopes: token.scope,
    })
}

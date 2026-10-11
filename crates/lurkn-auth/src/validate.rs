use lurkn_core::{Login, UserId};
use serde::Deserialize;

use crate::device::classify;
use crate::endpoints::IdEndpoints;
use crate::error::{AuthError, Result};

/// Identity as Twitch reports it for a token. Enough to know who is logged in
/// without requesting `user:read:email`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedToken {
    pub client_id: String,
    pub user_id: UserId,
    pub login: Login,
    pub scopes: Vec<String>,
    pub expires_in_secs: i64,
}

#[derive(Debug, Deserialize)]
struct ValidateResponse {
    client_id: String,
    login: String,
    user_id: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct TwitchError {
    status: u16,
    message: String,
}

/// `GET /oauth2/validate`. Also the cheapest way to confirm a restored token still
/// works before the UI claims the user is logged in.
pub async fn validate(
    http: &reqwest::Client,
    endpoints: &IdEndpoints,
    access_token: &str,
) -> Result<ValidatedToken> {
    let response = http
        .get(endpoints.validate())
        .header("Authorization", format!("OAuth {access_token}"))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        return Err(match serde_json::from_str::<TwitchError>(&body) {
            Ok(err) => classify(err.status, &err.message),
            Err(_) => AuthError::Twitch {
                status: status.as_u16(),
                message: body,
            },
        });
    }

    let parsed: ValidateResponse = serde_json::from_str(&body).map_err(|e| AuthError::Twitch {
        status: status.as_u16(),
        message: format!("unreadable validate response: {e}"),
    })?;

    let login = Login::parse(&parsed.login).map_err(|e| AuthError::Twitch {
        status: status.as_u16(),
        message: format!("Twitch returned an unusable login: {e}"),
    })?;

    Ok(ValidatedToken {
        client_id: parsed.client_id,
        user_id: UserId(parsed.user_id),
        login,
        scopes: parsed.scopes,
        expires_in_secs: parsed.expires_in,
    })
}

//! Authentication commands. Thin: validate, delegate, emit.

use std::sync::Arc;

use lurkn_auth::{AuthError, Session};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use time::format_description::well_known::Rfc3339;

use crate::events::AuthChanged;
use crate::state::AppState;

/// What the frontend knows about the session. Never carries a token
/// (`docs/architecture.md` §11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AuthStatus {
    LoggedOut,
    /// Showing the user their code while we poll.
    AwaitingUser {
        user_code: String,
        verification_uri: String,
        /// RFC 3339. The code dies after 30 minutes.
        expires_at: String,
    },
    Authenticated {
        user_id: String,
        login: String,
        /// Where the session is saved, so the user is never silently downgraded
        /// from the keyring to a file.
        stored_in: String,
        /// Whether this token carries the optional whisper scope. A session
        /// predating the feature does not, and says so rather than failing
        /// silently when a whisper never arrives.
        whispers: bool,
        /// Whether it carries the whole moderation bundle. All of it or none:
        /// a half-granted set would put buttons on screen that fail.
        moderation: bool,
    },
    /// Cached data stays on screen; an inline banner offers a new login.
    Expired,
    /// Login failed for a reason the user can act on.
    Failed {
        message: String,
    },
}

impl AuthStatus {
    /// Map an [`AuthError`] to what the user should see: a sentence, not a
    /// status code.
    fn from_error(error: &AuthError) -> Self {
        match error {
            AuthError::RefreshRejected => Self::Expired,
            AuthError::NoClientId => Self::Failed {
                message: "Lurkn was built without a Twitch client id.".to_owned(),
            },
            AuthError::Denied => Self::Failed {
                message: "The login was declined on Twitch.".to_owned(),
            },
            AuthError::DeviceCodeExpired => Self::Failed {
                message: "That login code expired. Start again to get a new one.".to_owned(),
            },
            AuthError::ScopesNotGranted { .. } => Self::Failed {
                message: "Twitch did not grant the permissions Lurkn needs. \
                          Start again and accept all of them."
                    .to_owned(),
            },
            AuthError::Transport(_) => Self::Failed {
                message: "Could not reach Twitch. Check your internet connection.".to_owned(),
            },
            AuthError::Storage(_) => Self::Failed {
                message: "Could not save your Twitch session.".to_owned(),
            },
            AuthError::Twitch { .. } | AuthError::AuthorizationPending | AuthError::SlowDown => {
                Self::Failed {
                    message: "Twitch rejected the login.".to_owned(),
                }
            }
        }
    }
}

/// Publish a new status: store it, then tell the frontend.
async fn publish(app: &AppHandle, state: &AppState, status: AuthStatus) {
    *state.auth.write().await = status.clone();
    if let Err(error) = AuthChanged(status).emit(app) {
        tracing::error!(%error, "could not emit the auth state");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn auth_status(state: State<'_, AppState>) -> Result<AuthStatus, ()> {
    Ok(state.auth.read().await.clone())
}

/// Start the device code flow and poll in the background until it resolves.
#[tauri::command]
#[specta::specta]
/// Start the device code flow.
///
/// `whispers` adds the optional whisper scope to the consent screen. It is a
/// parameter rather than a constant because the scope is opt-in: asking every
/// user for it would widen consent for a feature most will never turn on.
pub async fn auth_begin(
    app: AppHandle,
    state: State<'_, AppState>,
    whispers: Option<bool>,
    moderation: Option<bool>,
) -> Result<AuthStatus, ()> {
    // A second attempt replaces the first; two pollers would fight over the code.
    if let Some(task) = state.login_task.lock().await.take() {
        task.abort();
    }

    // Asked for together with whatever is already granted, so turning one
    // feature on does not silently turn another off.
    let mut extra: Vec<&str> = Vec::new();
    if whispers.unwrap_or(false) {
        extra.push("user:manage:whispers");
    }
    if moderation.unwrap_or(false) {
        extra.extend(lurkn_auth::MODERATION_SCOPES);
    }

    let prompt = match state.session.begin_login_with(&extra).await {
        Ok(prompt) => prompt,
        Err(error) => {
            tracing::warn!(%error, "could not start the Twitch login");
            let status = AuthStatus::from_error(&error);
            publish(&app, &state, status.clone()).await;
            return Ok(status);
        }
    };

    let status = AuthStatus::AwaitingUser {
        user_code: prompt.user_code.clone(),
        verification_uri: prompt.verification_uri.to_string(),
        expires_at: prompt.expires_at.format(&Rfc3339).unwrap_or_default(),
    };
    publish(&app, &state, status.clone()).await;

    let session = Arc::clone(&state.session);
    let stored_in = state.session.storage_kind().describe().to_owned();
    let app_for_task = app.clone();
    let device_code = prompt.device_code.clone();
    let interval = prompt.interval;
    let expires_at = prompt.expires_at;

    let task = tauri::async_runtime::spawn(async move {
        let outcome = poll_until_done(&session, &device_code, interval, expires_at).await;
        let status = match outcome {
            Ok(identity) => {
                tracing::info!(login = %identity.login, "logged in to Twitch");
                AuthStatus::Authenticated {
                    user_id: identity.user_id.to_string(),
                    login: identity.login.to_string(),
                    stored_in,
                    whispers: lurkn_auth::has_whispers(&identity.scopes),
                    moderation: lurkn_auth::has_moderation(&identity.scopes),
                }
            }
            Err(error) => {
                tracing::warn!(%error, "the Twitch login did not complete");
                AuthStatus::from_error(&error)
            }
        };

        let authenticated = matches!(status, AuthStatus::Authenticated { .. });

        if let Some(state) = app_for_task.try_state::<AppState>() {
            *state.auth.write().await = status.clone();
        }
        if let Err(error) = AuthChanged(status).emit(&app_for_task) {
            tracing::error!(%error, "could not emit the auth state");
        }

        // Fill the sidebar straight away rather than waiting for the poll tick.
        if authenticated {
            if let Some(state) = app_for_task.try_state::<AppState>() {
                crate::tasks::follows::refresh_now(&app_for_task, &state, true).await;
            }
            crate::overlay::sync(&app_for_task).await;
        }
    });

    *state.login_task.lock().await = Some(task);
    Ok(status)
}

/// Poll on Twitch's interval, backing off when asked, until the code resolves.
async fn poll_until_done(
    session: &Session,
    device_code: &str,
    interval: time::Duration,
    expires_at: time::OffsetDateTime,
) -> lurkn_auth::Result<lurkn_auth::ValidatedToken> {
    let mut wait = interval.unsigned_abs();

    loop {
        tokio::time::sleep(wait).await;

        if time::OffsetDateTime::now_utc() >= expires_at {
            return Err(AuthError::DeviceCodeExpired);
        }

        match session.complete_login(device_code).await {
            Ok(identity) => return Ok(identity),
            Err(AuthError::AuthorizationPending) => {}
            // Twitch asked us to slow down; the RFC says add the original interval.
            Err(AuthError::SlowDown) => wait += interval.unsigned_abs(),
            Err(error) => return Err(error),
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn auth_cancel(app: AppHandle, state: State<'_, AppState>) -> Result<AuthStatus, ()> {
    if let Some(task) = state.login_task.lock().await.take() {
        task.abort();
    }
    publish(&app, &state, AuthStatus::LoggedOut).await;
    Ok(AuthStatus::LoggedOut)
}

#[tauri::command]
#[specta::specta]
pub async fn auth_logout(app: AppHandle, state: State<'_, AppState>) -> Result<AuthStatus, ()> {
    if let Some(task) = state.login_task.lock().await.take() {
        task.abort();
    }
    // Chat holds subscriptions bound to the token being discarded.
    state.stop_chat().await;

    if let Err(error) = state.session.logout().await {
        tracing::warn!(%error, "could not clear the saved session");
    }
    publish(&app, &state, AuthStatus::LoggedOut).await;
    // The overlay stays up but stops showing a channel nobody is logged in to.
    crate::overlay::sync(&app).await;
    Ok(AuthStatus::LoggedOut)
}

/// Open the Twitch activation page. Reuses the browser launcher rather than
/// introducing a second way to open a URL (`docs/architecture.md` §3.2).
#[tauri::command]
#[specta::specta]
pub async fn auth_open_verification(state: State<'_, AppState>) -> Result<(), String> {
    let status = state.auth.read().await.clone();
    let AuthStatus::AwaitingUser {
        verification_uri, ..
    } = status
    else {
        return Err("There is no login in progress.".to_owned());
    };

    let url = url::Url::parse(&verification_uri)
        .map_err(|_| "The Twitch login link was unusable.".to_owned())?;

    state.browser.open_url(&url).map_err(|error| {
        tracing::error!(%error, "could not open the verification page");
        "Could not open your browser. Please check your system's default browser settings."
            .to_owned()
    })
}

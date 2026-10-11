//! The chat overlay's settings-screen commands. The overlay itself is
//! configured through ordinary settings; see `crate::overlay`.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::commands::auth::AuthStatus;
use crate::state::{AppState, OpenAs};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatus {
    /// The address to paste into an OBS browser source, while it runs.
    pub url: Option<String>,
    /// Why it is not running although it is switched on.
    pub error: Option<String>,
    /// Pages connected right now: OBS, a preview, or both.
    pub listeners: u32,
    /// The login of the channel it shows. `None` while logged out, or with
    /// the overlay on the open channel and none open.
    pub channel: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn overlay_status(state: State<'_, AppState>) -> Result<OverlayStatus, ()> {
    let server = state.overlay.read().await.clone();
    let listeners = match &server {
        Some(server) => server.listeners().await as u32,
        None => 0,
    };
    Ok(OverlayStatus {
        url: server.as_ref().map(|s| s.url()),
        error: state.overlay_error.read().await.clone(),
        listeners,
        channel: shown_login(&state).await,
    })
}

/// The overlay's channel is either the open one or the reader's own, so its
/// login is already known to one of the two.
async fn shown_login(state: &AppState) -> Option<String> {
    let shown = state.overlay_channel.read().await.clone()?;

    if let Some((login, id)) = state.current_channel.read().await.as_ref() {
        if *id == shown {
            return Some(login.to_string());
        }
    }
    match &*state.auth.read().await {
        AuthStatus::Authenticated { user_id, login, .. } if *user_id == shown.0 => {
            Some(login.clone())
        }
        _ => None,
    }
}

/// Show a sample line on every connected overlay. It never goes to Twitch.
#[tauri::command]
#[specta::specta]
pub async fn overlay_test(state: State<'_, AppState>) -> Result<u32, String> {
    let Some(server) = state.overlay.read().await.clone() else {
        return Err("The overlay is not running.".to_owned());
    };
    let name = match &*state.auth.read().await {
        AuthStatus::Authenticated { login, .. } => login.clone(),
        _ => "Lurkn".to_owned(),
    };
    Ok(server.message(&crate::overlay::test_message(&name)).await as u32)
}

/// Open the overlay in the browser with sample lines looping, so the look can
/// be judged without OBS.
#[tauri::command]
#[specta::specta]
pub async fn overlay_preview(state: State<'_, AppState>) -> Result<(), String> {
    let Some(server) = state.overlay.read().await.clone() else {
        return Err("The overlay is not running.".to_owned());
    };
    let url = url::Url::parse(&format!("{}?demo", server.url()))
        .map_err(|_| "The overlay address is not usable.".to_owned())?;

    // A link, not a stream: app mode or kiosk would make it awkward to close.
    state.open_url(&url, OpenAs::Link).await.map_err(|error| {
        tracing::warn!(%error, "could not open the overlay preview");
        "Could not open your browser.".to_owned()
    })
}

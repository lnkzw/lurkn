//! Followed-channel commands. See `docs/architecture.md` §4.3.

use lurkn_core::FollowedChannel;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::state::{AppState, OpenAs};

/// The sidebar's view of the follow list, plus how trustworthy it is.
///
/// Cached data stays on screen when Twitch is unreachable, but the user is told
/// it is stale rather than being shown a silent lie.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FollowsView {
    pub channels: Vec<FollowedChannel>,
    /// True once a live poll has succeeded this session.
    pub live_known: bool,
    /// Set when the last refresh failed and this list came from the cache.
    pub stale_reason: Option<String>,
    pub loading: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn follows_list(state: State<'_, AppState>) -> Result<FollowsView, ()> {
    Ok(state.follows.read().await.view())
}

/// Force a refresh now, ignoring the TTL. Wired to the sidebar's refresh button.
#[tauri::command]
#[specta::specta]
pub async fn follows_refresh(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<FollowsView, ()> {
    crate::tasks::follows::refresh_now(&app, &state, true).await;
    Ok(state.follows.read().await.view())
}

/// Open a channel's Twitch page in the system browser.
///
/// Deliberately separate from selecting a channel: arrow-keying down the sidebar
/// must not spray browser tabs. See `docs/architecture.md` §8.3.
#[tauri::command]
#[specta::specta]
pub async fn open_channel(state: State<'_, AppState>, login: String) -> Result<(), String> {
    let login = lurkn_core::Login::parse(&login)
        .map_err(|_| "That channel name is not usable.".to_owned())?;

    open_channel_for(&state, &login).await
}

/// Open a channel, for callers that already hold a parsed login and the state —
/// the command above, and the tray's live list.
pub async fn open_channel_for(state: &AppState, login: &lurkn_core::Login) -> Result<(), String> {
    let browser = state.settings.read().await.browser.clone();

    if browser.sync_window {
        return watch_in_synced_window(state, login).await;
    }

    // Only a channel the user asked for gets this treatment. A link from chat
    // is opened as written.
    let url = match browser.page {
        lurkn_core::StreamPage::Channel => lurkn_core::twitch_channel_url(login),
        lurkn_core::StreamPage::Player => lurkn_core::twitch_player_url(login),
    };

    state
        .open_url(&url, OpenAs::Stream)
        .await
        .map_err(browser_message)
}

/// Point the synced window at a channel, opening one if none is listening.
///
/// The count of listeners comes from an actual write, so a window closed behind
/// Lurkn's back means a new one is opened rather than a stream going nowhere.
/// See `docs/architecture.md` §7.2.
async fn watch_in_synced_window(state: &AppState, login: &lurkn_core::Login) -> Result<(), String> {
    let server = state.watch_server().await.map_err(|error| {
        tracing::error!(%error, "could not start the watch window server");
        "Could not start the synced window. Turn the setting off to open streams normally."
            .to_owned()
    })?;

    if server.set_channel(login).await > 0 {
        tracing::debug!(channel = login.as_str(), "switched the synced window");
        return Ok(());
    }

    let url = url::Url::parse(&server.page_url())
        .map_err(|_| "Could not build the address of the synced window.".to_owned())?;

    tracing::info!(channel = login.as_str(), "opening a synced window");
    state
        .open_url(&url, OpenAs::Stream)
        .await
        .map_err(browser_message)
}

/// Open a link from chat in the system browser.
///
/// The WebView cannot follow `target="_blank"` on its own, and letting it
/// navigate would replace Lurkn's own UI. The scheme is re-checked here even
/// though the tokenizer already refused anything but http/https: this is the
/// last gate before a string reaches the OS (`docs/architecture.md` §11).
///
/// Opened as a plain link, never with the stream settings: see [`OpenAs`].
#[tauri::command]
#[specta::specta]
pub async fn open_link(state: State<'_, AppState>, url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|_| "That link is not usable.".to_owned())?;

    lurkn_browser::check_scheme(&parsed).map_err(|_| "Lurkn only opens web links.".to_owned())?;

    state
        .open_url(&parsed, OpenAs::Link)
        .await
        .map_err(browser_message)
}

/// What to tell the user when a page could not be opened.
///
/// Which browser failed is the user's own setting, so the message points at the
/// setting rather than at the system default, which may not be involved at all.
fn browser_message(error: lurkn_browser::BrowserError) -> String {
    use lurkn_browser::BrowserError;

    tracing::error!(%error, "could not open a page");
    match error {
        BrowserError::Missing => {
            "The browser you chose is not installed any more. Pick another in settings.".to_owned()
        }
        BrowserError::LaunchFailed { .. } => {
            "Could not start the browser you chose. Check the browser setting.".to_owned()
        }
        BrowserError::DisallowedScheme { .. } => "Lurkn only opens web links.".to_owned(),
    }
}

/// The browsers installed on this machine, for the browser picker, with the
/// system default marked.
#[tauri::command]
#[specta::specta]
pub async fn list_browsers() -> Result<Vec<lurkn_browser::BrowserInfo>, ()> {
    let browsers = crate::browser::installed();
    let default = lurkn_browser::default_id(&browsers);
    Ok(browsers
        .iter()
        .map(|browser| browser.info(default.as_deref()))
        .collect())
}

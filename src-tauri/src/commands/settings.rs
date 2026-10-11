//! Settings and the debug log view.

use lurkn_core::Settings;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::events::SettingsChanged;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn settings_get(state: State<'_, AppState>) -> Result<Settings, ()> {
    Ok(state.settings.read().await.clone())
}

/// Replace the settings wholesale. Values are sanitized before they are stored,
/// so the frontend cannot persist an unusable font size.
#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    apply(&app, &state, settings).await
}

/// Store, apply and announce new settings. Shared with the synced window,
/// whose font keys change the chat font from outside the settings screen.
pub async fn apply(
    app: &AppHandle,
    state: &AppState,
    settings: Settings,
) -> Result<Settings, String> {
    let settings = settings.sanitized();

    state.store.save_settings(&settings).map_err(|error| {
        tracing::warn!(%error, "could not save settings");
        "Could not save your settings.".to_owned()
    })?;

    *state.rules.write().await = std::sync::Arc::new(lurkn_core::ChatRules::compile(&settings));
    *state.settings.write().await = settings.clone();
    crate::theme::wake(state);

    if let Err(error) = SettingsChanged(settings.clone()).emit(app) {
        tracing::error!(%error, "could not emit the settings change");
    }

    // In the background: the first start fetches your channel's emotes, and a
    // slider being dragged should not wait on that. The settings screen asks
    // for the overlay's status separately.
    let app = app.clone();
    tauri::async_runtime::spawn(async move { crate::overlay::sync(&app).await });
    Ok(settings)
}

/// How Lurkn's chat looks right now, for the chat beside the player in the
/// synced window. Sent by the frontend, which is the one place the palette is
/// fully resolved — theme file, system preference and all.
#[tauri::command]
#[specta::specta]
pub async fn watch_set_look(
    state: State<'_, AppState>,
    look: lurkn_watch::WatchLook,
) -> Result<(), ()> {
    let look = look.sanitized();
    *state.watch_look.write().await = Some(look.clone());
    if let Some(server) = crate::watch::running(&state).await {
        server.set_look(look).await;
    }
    Ok(())
}

/// Whether a highlight or ignore pattern compiles, so the settings screen can
/// say so beside the pattern rather than the rule silently never matching.
///
/// Asked of Rust rather than checked in the WebView: the `regex` crate is what
/// runs the rule, and JavaScript's dialect accepts and rejects different
/// things.
#[tauri::command]
#[specta::specta]
pub async fn check_pattern(pattern: String, regex: bool) -> Result<(), String> {
    lurkn_core::rules::check(&pattern, regex)
}

/// Where the on-disk log lives, so the settings screen can point at it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogInfo {
    pub path: Option<String>,
    pub lines: Vec<String>,
}

/// The tail of today's log: technical detail stays available, just not in the
/// user's face.
///
/// Credentials are redacted as the log is written, so nothing here needs
/// scrubbing on the way out.
#[tauri::command]
#[specta::specta]
pub async fn log_tail(lines: u32) -> Result<LogInfo, ()> {
    let wanted = (lines.clamp(10, 2000)) as usize;
    let Some(path) = crate::current_log_path() else {
        return Ok(LogInfo {
            path: None,
            lines: Vec::new(),
        });
    };

    let contents = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let all: Vec<&str> = contents.lines().collect();
    let tail = all[all.len().saturating_sub(wanted)..]
        .iter()
        .map(|l| (*l).to_owned())
        .collect();

    Ok(LogInfo {
        path: Some(path.display().to_string()),
        lines: tail,
    })
}

/// Open the folder holding the log in the user's file manager.
#[tauri::command]
#[specta::specta]
pub async fn open_log_folder(state: State<'_, AppState>) -> Result<(), String> {
    let Some(path) = crate::current_log_path() else {
        return Err("There is no log file yet.".to_owned());
    };
    let folder = path.parent().unwrap_or(&path).to_path_buf();

    // Reuses the opener the browser launcher already uses; no second mechanism.
    state.open_path(&folder).map_err(|error| {
        tracing::warn!(%error, "could not open the log folder");
        "Could not open your file manager.".to_owned()
    })
}

/// Font families installed on this machine, for the chat font picker.
///
/// Only names that survive sanitising are returned, so anything the user can
/// pick is something Lurkn can actually apply.
#[tauri::command]
#[specta::specta]
pub async fn list_fonts() -> Result<Vec<String>, ()> {
    Ok(crate::fonts::system_families().to_vec())
}

/// The theme file in effect, for the first paint. Changes after that arrive as
/// `ThemeChanged`.
#[tauri::command]
#[specta::specta]
pub async fn theme_current(state: State<'_, AppState>) -> Result<crate::theme::ThemeState, ()> {
    Ok(state.theme.read().await.clone())
}

/// The theme files the picker can offer, and the folder they live in.
///
/// Read each time the picker is opened rather than watched: a new file only
/// matters at the moment someone goes to choose it.
#[tauri::command]
#[specta::specta]
pub async fn theme_list() -> Result<crate::theme::ThemeList, ()> {
    Ok(tauri::async_runtime::spawn_blocking(crate::theme::list)
        .await
        .unwrap_or(crate::theme::ThemeList {
            folder: None,
            themes: Vec::new(),
        }))
}

/// Open the themes folder, creating it with an example theme the first time.
#[tauri::command]
#[specta::specta]
pub async fn open_theme_folder(state: State<'_, AppState>) -> Result<(), String> {
    crate::theme::open_folder(&state)
}

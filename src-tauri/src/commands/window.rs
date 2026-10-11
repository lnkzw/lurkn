//! Windows beyond the main one: settings and the activity feed, each opened
//! on its own.
//!
//! Both are the same frontend under `?view=…`, showing one screen alone. The
//! settings window needs nothing of its own in Rust: it reads and writes
//! settings through the same commands, and every window hears
//! `SettingsChanged`, so whichever one made a change, both show it. The
//! activity window is fed by the main window, which holds the chat log the
//! feed is a view of.

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tauri_specta::Event;

use crate::events::{SettingsSection, ShowSettings};

/// The windows' labels, which the capability file grants commands to.
pub const SETTINGS_WINDOW: &str = "settings";
pub const ACTIVITY_WINDOW: &str = "activity";

/// Open the settings window at a group, or bring it forward and turn it to
/// that group if it is already open.
#[tauri::command]
#[specta::specta]
pub async fn open_settings_window(app: AppHandle, section: Option<String>) -> Result<(), String> {
    open_settings(&app, section)
}

/// Settings, the way the reader asked for them: in their own window, or over
/// the chat in the main window. For the tray, which has no frontend state of
/// its own to ask.
///
/// From a task rather than the tray's own callback: the preference is behind
/// an async lock, and building a webview on the thread that is dispatching the
/// menu event can stall it.
pub fn show_settings(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<crate::state::AppState>() else {
            return;
        };
        let in_window = state.settings.read().await.window.settings_in_window;

        if in_window {
            if let Err(error) = open_settings(&app, None) {
                tracing::warn!(error, "could not open settings from the tray");
            }
            return;
        }

        settings_over_chat(&app, None);
    });
}

/// Put settings back over the chat: the main window forward, showing them at
/// the group the settings window was on. The settings window closes itself.
#[tauri::command]
#[specta::specta]
pub fn settings_in_main(app: AppHandle, section: Option<String>) {
    settings_over_chat(&app, group(section));
}

fn settings_over_chat(app: &AppHandle, section: Option<String>) {
    crate::tray::show_window(app);
    if let Err(error) = ShowSettings(section).emit_to(app, "main") {
        tracing::warn!(%error, "could not open settings in the main window");
    }
}

/// A settings group id: a short lowercase word. It lands in a URL and in the
/// other window's state, so anything else is dropped rather than passed along.
fn group(section: Option<String>) -> Option<String> {
    section.filter(|s| !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_lowercase()))
}

fn open_settings(app: &AppHandle, section: Option<String>) -> Result<(), String> {
    let section = group(section);

    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        if let Some(section) = section {
            if let Err(error) = SettingsSection(section).emit_to(app, SETTINGS_WINDOW) {
                tracing::warn!(%error, "could not turn the settings window to a group");
            }
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let path = match &section {
        Some(section) => format!("index.html?view=settings&section={section}"),
        None => "index.html?view=settings".to_owned(),
    };

    let builder = WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::App(path.into()))
        .title("Lurkn settings")
        .inner_size(780.0, 720.0)
        .min_inner_size(520.0, 420.0);
    // The settings header is a little over 50px tall.
    framed(builder, 20.0).build().map(|_| ()).map_err(|error| {
        tracing::error!(%error, "could not open the settings window");
        "Could not open the settings window.".to_owned()
    })
}

/// The frame every extra window gets: the main window's.
///
/// Undecorated, because the window's own header is its title bar — it drags
/// the window and its ✕ closes it — and a system bar above it would be a
/// second one. On macOS the traffic lights stay, over the left of that header
/// (which leaves them room), as in the main window; `lights_y` sets them
/// level with it.
fn framed<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    lights_y: f64,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(target_os = "macos")]
    let builder = builder
        .decorations(true)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(14.0, lights_y));
    #[cfg(not(target_os = "macos"))]
    let builder = {
        let _ = lights_y;
        builder.decorations(false)
    };

    builder
        .shadow(true)
        // The main window's own background, so it does not flash white
        // before the first paint.
        .background_color(tauri::window::Color(14, 14, 16, 255))
}

/// Bring the main window forward. The settings window asks for it when what
/// it started — a login — is shown there.
#[tauri::command]
#[specta::specta]
pub fn show_main_window(app: AppHandle) {
    crate::tray::show_window(&app);
}

/// Open or remove the activity window, to match the settings: the main window
/// calls this whenever `activity.enabled` or `activity.in_window` changes.
///
/// An open window is left where it is rather than brought forward — this runs
/// on every settings change. Removing it destroys rather than closes it, so
/// only a close the reader asked for reaches [`activity_closed`].
#[tauri::command]
#[specta::specta]
pub async fn activity_window(app: AppHandle, open: bool) -> Result<(), String> {
    let existing = app.get_webview_window(ACTIVITY_WINDOW);
    match (open, existing) {
        (true, Some(_)) | (false, None) => Ok(()),
        (false, Some(window)) => window.destroy().map_err(|error| {
            tracing::warn!(%error, "could not remove the activity window");
            "Could not close the activity window.".to_owned()
        }),
        (true, None) => framed(
            WebviewWindowBuilder::new(
                &app,
                ACTIVITY_WINDOW,
                WebviewUrl::App("index.html?view=activity".into()),
            )
            .title("Lurkn activity")
            .inner_size(340.0, 640.0)
            .min_inner_size(240.0, 240.0),
            // The feed's header is one rail row, 36px.
            11.0,
        )
        .build()
        .map(|_| ())
        .map_err(|error| {
            tracing::error!(%error, "could not open the activity window");
            "Could not open the activity window.".to_owned()
        }),
    }
}

/// The reader closed the activity window themselves — its ✕, or the window
/// manager's close. That means "hide the feed", the same as the ✕ on the
/// column, so the setting follows; otherwise the next settings change would
/// open the window straight back up.
pub fn activity_closed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<crate::state::AppState>() else {
            return;
        };
        let mut settings = state.settings.read().await.clone();
        if !(settings.activity.enabled && settings.activity.in_window) {
            return;
        }
        settings.activity.enabled = false;
        if let Err(error) = crate::commands::settings::apply(&app, &state, settings).await {
            tracing::warn!(error, "could not hide the activity feed");
        }
    });
}

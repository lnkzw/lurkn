#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Tauri shell. Wiring only — all logic lives in `crates/`.

mod browser;
mod chat_api;
mod commands;
mod events;
mod fonts;
mod logging;
mod notify;
mod overlay;
mod state;
mod tasks;
mod theme;
mod tray;
mod update;
mod watch;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::logging::Redacting;

fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter =
        EnvFilter::try_from_env("LURKN_LOG").unwrap_or_else(|_| EnvFilter::new("info,lurkn=debug"));

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_writer(Redacting(std::io::stdout));

    // A rolling file is what backs the log view in Settings → Advanced.
    let log_dir = dirs_log_path();
    let file_layer = log_dir.as_ref().and_then(|dir| {
        std::fs::create_dir_all(dir).ok()?;
        let appender = tracing_appender::rolling::daily(dir, "lurkn.log");
        let (writer, guard) = tracing_appender::non_blocking(appender);
        let layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(Redacting(writer));
        Some((layer, guard))
    });

    match file_layer {
        Some((layer, guard)) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .with(layer)
                .init();
            Some(guard)
        }
        None => {
            tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .init();
            tracing::warn!("no writable log directory; file logging disabled");
            None
        }
    }
}

/// Today's log file, matching the daily rotation the appender uses.
pub(crate) fn current_log_path() -> Option<std::path::PathBuf> {
    use time::format_description::well_known::Iso8601;

    let dir = dirs_log_path()?;
    let today = time::OffsetDateTime::now_utc()
        .date()
        .format(&Iso8601::DATE)
        .ok()?;
    Some(dir.join(format!("lurkn.log.{today}")))
}

/// Where the log goes: `$XDG_STATE_HOME/lurkn/logs` on Linux (`~/.local/state`),
/// `~/Library/Logs/lurkn` on macOS, where Console looks for it, and
/// `%LOCALAPPDATA%\lurkn\logs` on Windows.
fn dirs_log_path() -> Option<std::path::PathBuf> {
    if let Some(state) = dirs::state_dir() {
        return Some(state.join("lurkn").join("logs"));
    }
    if cfg!(target_os = "macos") {
        dirs::home_dir().map(|home| home.join("Library").join("Logs").join("lurkn"))
    } else {
        dirs::data_local_dir().map(|base| base.join("lurkn").join("logs"))
    }
}

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::app_info,
            commands::auth::auth_status,
            commands::auth::auth_begin,
            commands::auth::auth_cancel,
            commands::auth::auth_logout,
            commands::auth::auth_open_verification,
            commands::follows::follows_list,
            commands::follows::follows_refresh,
            commands::follows::open_channel,
            commands::follows::open_link,
            commands::follows::list_browsers,
            commands::chat::select_channel,
            commands::chat::chat_status,
            commands::chat::send_message,
            commands::chat::last_channel,
            commands::chat::user_profile,
            commands::chat::chat_emotes,
            commands::chat::search_channels,
            commands::chat::recent_channels,
            commands::chat::suggested_channels,
            commands::chat::my_channel,
            commands::chat::own_stream_live,
            commands::moderation::moderated_channels,
            commands::moderation::moderate_ban,
            commands::moderation::moderate_unban,
            commands::moderation::moderate_delete,
            commands::moderation::moderate_room,
            commands::moderation::moderate_announce,
            commands::whispers::whisper_threads,
            commands::whispers::whisper_thread,
            commands::whispers::whispers_mark_read,
            commands::whispers::whispers_unread,
            commands::whispers::whisper_send,
            commands::whispers::whisper_find_user,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::check_pattern,
            commands::settings::list_fonts,
            commands::settings::log_tail,
            commands::settings::open_log_folder,
            commands::settings::theme_current,
            commands::settings::theme_list,
            commands::settings::open_theme_folder,
            commands::settings::watch_set_look,
            commands::window::open_settings_window,
            commands::window::show_main_window,
            commands::window::settings_in_main,
            commands::window::activity_window,
            commands::overlay::overlay_status,
            commands::overlay::overlay_test,
            commands::overlay::overlay_preview,
            commands::update::update_status,
            commands::update::update_check,
            commands::update::update_install,
        ])
        .events(collect_events![
            events::AuthChanged,
            events::FollowsUpdated,
            events::LiveDelta,
            events::ChatBatch,
            events::ChatStateChanged,
            events::EmotesReady,
            events::SettingsChanged,
            events::SettingsSection,
            events::ShowSettings,
            events::ThemeChanged,
            events::WhisperReceived,
            events::ShowWhispers,
            events::ChatHistory,
            events::OpenChannelChanged,
            events::OwnStreamChanged,
            events::ModerationEvent,
            events::UpdateChanged,
        ])
}

/// Where the generated TypeScript lands. Relative to `src-tauri/`.
#[cfg(any(debug_assertions, test))]
const BINDINGS_PATH: &str = "../src/lib/ipc/bindings.ts";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _log_guard = init_logging();
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    {
        // Regenerated on every debug run; src/lib/ipc/bindings.ts is not hand-edited.
        use specta_typescript::Typescript;
        if let Err(error) = builder.export(Typescript::default(), BINDINGS_PATH) {
            tracing::error!(%error, "failed to export TypeScript bindings");
        }
    }

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Lurkn");

    tauri::Builder::default()
        // First, so a second launch is turned away before it builds anything:
        // two processes would mean two chat sockets, two tray icons and two
        // writers on one database. The running one comes forward instead.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tracing::info!("Lurkn was started again; showing the running window");
            tray::show_window(app);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        // Driven from Rust only; the WebView is granted none of its commands.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let handle = app.handle().clone();
            let browser = std::sync::Arc::new(browser::SystemBrowser::new(handle.clone()));
            app.manage(state::AppState::new(handle.clone(), browser));
            app.manage(update::Updates::default());

            // Restoring can hit the network, so it must not block the window.
            let poller_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                // The cached list paints before any request goes out.
                tasks::follows::load_from_cache(&handle).await;
                restore_session(handle.clone()).await;

                if let Some(state) = handle.try_state::<state::AppState>() {
                    if state.current_user_id().await.is_some() {
                        tasks::follows::refresh_now(&handle, &state, false).await;
                    }
                }

                // After the session is back, so a running overlay starts out
                // subscribed to the right channel.
                overlay::sync(&handle).await;
            });
            tauri::async_runtime::spawn(tasks::follows::run_poller(poller_handle.clone()));
            tauri::async_runtime::spawn(tasks::chat::run_pump(poller_handle));
            tauri::async_runtime::spawn(theme::run(app.handle().clone()));
            tauri::async_runtime::spawn(update::run(app.handle().clone()));

            // The tray is what makes closing the window safe: the chat socket,
            // the whisper subscription and the watch-window server all live in
            // this process, and Twitch will not replay what they miss.
            tray::install(app.handle());

            // Hidden only when asked for, and only when there is a tray to get
            // the window back from — otherwise Lurkn would start invisible with
            // no way to reach it.
            if tray::start_hidden() {
                match app.handle().tray_by_id(tray::TRAY_ID) {
                    Some(_) => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                            tracing::info!("started hidden; open Lurkn from the tray");
                        }
                    }
                    None => tracing::warn!(
                        "LURKN_START_HIDDEN is set but there is no tray to restore from"
                    ),
                }
            }

            // Unread whispers from an earlier session, and whatever the cached
            // follow list already knows is live, before anything is fetched.
            let tray_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tray::refresh(&tray_handle).await;
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            let tray_present = window.app_handle().tray_by_id("lurkn").is_some();
            tray::on_window_event(window, event, tray_present);

            if window.label() == commands::window::ACTIVITY_WINDOW
                && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                commands::window::activity_closed(window.app_handle());
            }
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            tracing::error!(%error, "fatal: could not start the application window");
            std::process::exit(1);
        });
}

/// Bring back a saved session, if there is one that still works.
async fn restore_session(app: tauri::AppHandle) {
    use commands::auth::AuthStatus;
    use tauri::Manager;
    use tauri_specta::Event;

    let Some(state) = app.try_state::<state::AppState>() else {
        return;
    };

    let status = match state.session.restore().await {
        Ok(Some(identity)) => {
            tracing::info!(login = %identity.login, "restored the saved Twitch session");
            AuthStatus::Authenticated {
                user_id: identity.user_id.to_string(),
                login: identity.login.to_string(),
                stored_in: state.session.storage_kind().describe().to_owned(),
                whispers: lurkn_auth::has_whispers(&identity.scopes),
                moderation: lurkn_auth::has_moderation(&identity.scopes),
            }
        }
        Ok(None) => AuthStatus::LoggedOut,
        Err(error) if error.requires_reauth() => {
            tracing::info!(%error, "the saved session needs a new login");
            AuthStatus::Expired
        }
        Err(error) => {
            // Offline at startup: not a reason to throw the session away.
            tracing::warn!(%error, "could not check the saved session");
            AuthStatus::Expired
        }
    };

    *state.auth.write().await = status.clone();
    if let Err(error) = events::AuthChanged(status).emit(&app) {
        tracing::error!(%error, "could not emit the restored auth state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerates `src/lib/ipc/bindings.ts`.
    ///
    /// Running it as a test means `cargo test` keeps the TypeScript in step
    /// without launching a window, and CI can fail on an uncommitted diff.
    /// See `docs/architecture.md` §8.2.
    #[test]
    fn export_typescript_bindings() {
        use specta_typescript::Typescript;

        specta_builder()
            .export(Typescript::default(), BINDINGS_PATH)
            .expect("failed to export TypeScript bindings");
    }
}

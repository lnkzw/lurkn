//! The tray icon, and the "keep running when the window closes" behaviour.
//!
//! This is not decoration. Lurkn owns things that only exist while its process
//! does: the chat socket, the whisper subscription — whose history Twitch will
//! not serve again — and the watch-window server, whose port and token are made
//! fresh each session, so a restart orphans any window already open on it. A
//! tray lets the window be closed without losing any of that.
//!
//! Quitting from the tray is the way out, and it is deliberately the only one
//! that is not reversible by reopening the window.
//!
//! Interaction differs by platform, and not by choice. Linux delivers no tray
//! mouse events to the application at all, so the menu is the entire interface
//! there and is bound to both buttons. Windows and macOS do deliver clicks, so
//! there the left button toggles the window and the menu stays on the right.

use tauri::menu::{Menu, MenuEvent, MenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

const SHOW: &str = "show";
const WHISPERS: &str = "whispers";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

/// Prefix for a live channel's menu item; the rest of the id is its login.
const LIVE: &str = "live:";

/// How many live channels the submenu shows.
///
/// A tray menu is a shortcut, not the sidebar: past a dozen it is faster to
/// open the window, and a menu taller than the screen is unusable.
const LIVE_LIMIT: usize = 12;

/// The tray's id, which is how the rest of the app finds it again.
pub const TRAY_ID: &str = "lurkn";

/// What the menu shows. Gathered once per rebuild so the menu is consistent
/// with itself, rather than each item reading its own snapshot.
struct TrayState {
    unread: u32,
    /// `(login, label)` per live channel, most watched first.
    live: Vec<(String, String)>,
}

/// Start with the window hidden in the tray, rather than in front of whatever
/// the person is doing.
///
/// Set by `LURKN_START_HIDDEN=1`. Meant for development, where `tauri dev`
/// rebuilds and relaunches on every Rust change and each relaunch otherwise
/// throws a window onto the workspace in use. It is a variable rather than a
/// setting because it is about how Lurkn was started, not how it should behave.
pub fn start_hidden() -> bool {
    std::env::var("LURKN_START_HIDDEN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Build the tray icon. Failure is logged, not fatal: a desktop with no tray
/// (a bare Wayland compositor, a broken StatusNotifier host) must still get an
/// application.
pub fn install(app: &AppHandle) {
    if let Err(error) = try_install(app) {
        tracing::warn!(%error, "no tray icon; closing the window will quit Lurkn");
    }
}

fn try_install(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(
        app,
        &TrayState {
            unread: 0,
            live: Vec::new(),
        },
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            tauri::Error::AssetNotFound("the window icon, for the tray".to_owned())
        })?)
        .tooltip("Lurkn")
        .menu(&menu)
        // Linux emits no tray click events at all — Tauri's own note: "Linux:
        // Unsupported. The event is not emitted even though the icon is shown
        // and will still show a context menu on right click." So there, the
        // menu has to answer the left button too, or the left button does
        // nothing whatever. Elsewhere a real click arrives and toggles the
        // window, and opening the menu as well would fight it.
        .show_menu_on_left_click(cfg!(target_os = "linux"))
        .on_menu_event(on_menu)
        .on_tray_icon_event(on_click)
        .build(app)?;

    Ok(())
}

/// The menu for a given state.
///
/// Rebuilt rather than relabelled: the live list changes shape, not just text,
/// and keeping handles to every item across the app would be more machinery
/// than a short menu is worth.
fn build_menu(app: &AppHandle, state: &TrayState) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, SHOW, "Show Lurkn", true, None::<&str>)?;

    let label = match state.unread {
        0 => "Whispers".to_owned(),
        // Past a hundred the exact number stops being information.
        n if n > 99 => "Whispers (99+)".to_owned(),
        n => format!("Whispers ({n})"),
    };
    let whispers = MenuItem::with_id(app, WHISPERS, label, true, None::<&str>)?;

    let settings = MenuItem::with_id(app, SETTINGS, "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;

    // Built even when empty, and disabled instead of hidden: a menu whose items
    // move between openings is one you have to read every time.
    let live = build_live(app, &state.live)?;

    Menu::with_items(app, &[&show, &live, &whispers, &settings, &quit])
}

fn build_live(app: &AppHandle, live: &[(String, String)]) -> tauri::Result<Submenu<tauri::Wry>> {
    if live.is_empty() {
        let none = MenuItem::with_id(app, "live-none", "Nobody is live", false, None::<&str>)?;
        return Submenu::with_items(app, "Live now", true, &[&none]);
    }

    let mut items = Vec::with_capacity(live.len());
    for (login, label) in live {
        items.push(MenuItem::with_id(
            app,
            format!("{LIVE}{login}"),
            label,
            true,
            None::<&str>,
        )?);
    }

    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();

    Submenu::with_items(app, format!("Live now ({})", live.len()), true, &refs)
}

/// Rebuild the tray from what the app currently knows.
///
/// Reads its own inputs rather than taking them as arguments: the unread count
/// and the live list change in different places, and passing both through every
/// caller would mean each one knowing about the other's business.
///
/// Silent on failure: the tray is an indicator, and a desktop that will not take
/// a menu is not a reason to fail whatever produced the change.
pub async fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };

    let unread = state.store.unread_whispers().unwrap_or(0);

    let mut channels: Vec<(u32, String, String)> = state
        .follows
        .read()
        .await
        .view()
        .channels
        .into_iter()
        .filter_map(|channel| {
            let stream = channel.stream?;
            let name = channel.channel.user.display_name;
            let game = stream
                .category
                .map(|c| format!(" — {}", c.name))
                .unwrap_or_default();

            Some((
                stream.viewer_count,
                channel.channel.user.login.to_string(),
                format!("{name}{game}"),
            ))
        })
        .collect();

    // Busiest first, which is the order a viewer is most likely to want.
    channels.sort_by_key(|(viewers, ..)| std::cmp::Reverse(*viewers));

    let live: Vec<(String, String)> = channels
        .into_iter()
        .take(LIVE_LIMIT)
        .map(|(_, login, label)| (login, label))
        .collect();

    let tooltip = match unread {
        0 => "Lurkn".to_owned(),
        1 => "Lurkn — 1 unread whisper".to_owned(),
        n => format!("Lurkn — {n} unread whispers"),
    };
    let _ = tray.set_tooltip(Some(tooltip));

    match build_menu(app, &TrayState { unread, live }) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(error) => tracing::debug!(%error, "could not rebuild the tray menu"),
    }
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        SHOW => show_window(app),
        WHISPERS => {
            show_window(app);

            use tauri_specta::Event;
            if let Err(error) = crate::events::ShowWhispers.emit(app) {
                tracing::warn!(%error, "could not open the whisper panel");
            }
        }
        SETTINGS => crate::commands::window::show_settings(app),
        QUIT => {
            tracing::info!("quitting from the tray");
            app.exit(0);
        }
        id if id.starts_with(LIVE) => open_live(app, &id[LIVE.len()..]),
        other => tracing::debug!(id = other, "unhandled tray menu item"),
    }
}

/// Toggle the window on a left click.
///
/// Never called on Linux, which emits no tray mouse events — the menu is the
/// whole interaction there. Kept because it is correct on Windows and macOS,
/// where the click does arrive and a menu on the left button is wrong.
fn on_click(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
    // Only a completed left click. Tauri also reports moves and enter/leave,
    // and reacting to those would make the window flicker on a mouse pass.
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        toggle_window(tray.app_handle());
    }
}

/// Open a live channel the same way the sidebar does, synced window and all.
fn open_live(app: &AppHandle, login: &str) {
    let Ok(parsed) = lurkn_core::Login::parse(login) else {
        tracing::warn!(login, "a tray item carried an unusable channel name");
        return;
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<crate::state::AppState>() else {
            return;
        };
        if let Err(message) = crate::commands::follows::open_channel_for(&state, &parsed).await {
            // No window is guaranteed to be open to show this in, so the log is
            // where it goes.
            tracing::warn!(message, "could not open a channel from the tray");
        }
    });
}

pub fn show_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// A left click shows the window, or hides it again if it is already in front.
fn toggle_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let visible = window.is_visible().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);

    // Visible but behind something counts as wanting it in front, not as
    // wanting it gone.
    if visible && focused {
        let _ = window.hide();
    } else {
        show_window(app);
    }
}

/// Intercept the main window's close button so it hides instead of quitting.
///
/// Only when a tray icon exists to get the window back from: without one this
/// would strand the user with a process they cannot see or stop. Any other
/// window — settings opened on their own — simply closes.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent, tray_present: bool) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if !tray_present || window.label() != "main" {
            return;
        }

        api.prevent_close();
        let _ = window.hide();
        tracing::debug!("window hidden to the tray; chat and whispers keep running");
    }
}

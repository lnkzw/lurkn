//! Desktop notifications.
//!
//! Fired from Rust rather than from the WebView on purpose: the window is
//! routinely hidden behind the tray, and a notification that needs a live page
//! to send it would be silent exactly when it matters. It also keeps the
//! WebView's capability surface unchanged — see `docs/architecture.md` §11.
//!
//! Every one of these is off-by-default or user-toggled, and none of them fires
//! for something the user did themselves.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Show a notification, or log why not.
///
/// Never fails a caller: a notification is the least important thing happening
/// at any of these call sites, and a desktop without a notification daemon is
/// a normal thing to run into on Linux.
fn show(app: &AppHandle, title: &str, body: &str) {
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!(%error, "could not show a desktop notification");
    }
}

/// A followed channel went live.
///
/// One notification per channel rather than one summarising several: they are
/// separate things to act on, and a merged line ("3 channels went live") makes
/// the reader open the window to find out which.
pub fn went_live(app: &AppHandle, name: &str, category: Option<&str>) {
    let body = match category {
        Some(game) if !game.is_empty() => format!("{name} is live — {game}"),
        _ => format!("{name} is live"),
    };
    show(app, "Live on Twitch", &body);
}

/// Several channels went live at once.
///
/// One notification rather than a stack: past a handful, a notification per
/// channel is a wall to dismiss rather than something to act on. The first few
/// names are still given, because "6 channels went live" alone would send the
/// reader to the window to find out which.
pub fn many_went_live(app: &AppHandle, names: &[&str]) {
    const SHOWN: usize = 3;

    let listed = names
        .iter()
        .take(SHOWN)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");

    let body = if names.len() > SHOWN {
        format!("{listed} and {} more are live", names.len() - SHOWN)
    } else {
        format!("{listed} are live")
    };

    show(app, "Live on Twitch", &body);
}

/// The reader's own stream went on or off air.
///
/// Worth saying out loud because the usual way to find out is to alt-tab to
/// Twitch and look — and because a stream that stopped without being told to is
/// the thing a streamer most wants to hear about at once.
pub fn own_stream(app: &AppHandle, live: bool) {
    if live {
        show(app, "You are live", "Twitch has your stream.");
    } else {
        show(
            app,
            "Your stream ended",
            "Twitch is no longer receiving it.",
        );
    }
}

/// A whisper arrived.
pub fn whisper(app: &AppHandle, from: &str, body: &str) {
    // Whispers are private, so the preview is short: a notification can sit on
    // screen in front of anyone walking past.
    const PREVIEW: usize = 80;

    let preview: String = body.chars().take(PREVIEW).collect();
    let shown = if body.chars().count() > PREVIEW {
        format!("{preview}…")
    } else {
        preview
    };

    show(app, &format!("Whisper from {from}"), &shown);
}

/// A chat message named the user, or matched one of their keywords.
pub fn highlight(app: &AppHandle, channel: &str, from: &str, body: &str) {
    const PREVIEW: usize = 120;

    let preview: String = body.chars().take(PREVIEW).collect();
    let shown = if body.chars().count() > PREVIEW {
        format!("{preview}…")
    } else {
        preview
    };

    show(app, &format!("{from} in {channel}"), &shown);
}

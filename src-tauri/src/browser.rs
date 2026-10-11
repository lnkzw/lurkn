//! The one concrete [`BrowserLauncher`]: hand the URL to the OS.
//!
//! No assumption of a particular browser, desktop or display server. On
//! Linux that is `xdg-open`, which honours the user's configured default
//! browser; elsewhere the opener plugin.

use lurkn_browser::{BrowserLauncher, Result};
use tauri::AppHandle;
use url::Url;

#[derive(Debug)]
pub struct SystemBrowser {
    /// For the opener plugin, which Linux does without.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    app: AppHandle,
}

impl SystemBrowser {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// `xdg-open`, started with the environment `lurkn_browser::host` cleans.
    #[cfg(target_os = "linux")]
    fn hand_over(&self, url: &Url) -> Result<()> {
        lurkn_browser::launch::spawn(&lurkn_browser::launch::for_desktop(url.as_str()))
    }

    #[cfg(not(target_os = "linux"))]
    fn hand_over(&self, url: &Url) -> Result<()> {
        use tauri_plugin_opener::OpenerExt;

        self.app
            .opener()
            .open_url(url.as_str(), None::<&str>)
            .map_err(|e| {
                tracing::error!(error = %e, "the system browser could not be launched");
                lurkn_browser::BrowserError::LaunchFailed {
                    detail: e.to_string(),
                }
            })
    }
}

impl BrowserLauncher for SystemBrowser {
    fn open_url(&self, url: &Url) -> Result<()> {
        // Re-checked here even though callers validate: this is the last gate
        // before a string reaches the OS.
        lurkn_browser::check_scheme(url)?;
        self.hand_over(url)
    }
}

/// The browsers installed on this machine, newest scan each time.
///
/// Not cached: the list is read when the settings screen opens and when a
/// stream is opened, both rare, and a cache would keep showing a browser the
/// user has just uninstalled.
pub fn installed() -> Vec<lurkn_browser::Browser> {
    lurkn_browser::installed()
}

/// The desktop's own default web browser, as a desktop-entry id.
///
/// Asked of `xdg-mime` rather than guessed, so it matches what the opener
/// plugin would have done. Used to honour app and kiosk mode for the "system
/// default" choice, which otherwise could not take flags at all.
pub fn default_browser_id() -> Option<String> {
    let output = lurkn_browser::host::command("xdg-mime")
        .args(["query", "default", "x-scheme-handler/https"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let id = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!id.is_empty()).then_some(id)
}

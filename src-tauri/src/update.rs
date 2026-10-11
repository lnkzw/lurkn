//! Finding and installing a newer Lurkn. See `docs/architecture.md` §14.1.
//!
//! All of it happens here. The WebView holds no updater permission, only the
//! three commands in `commands::update`, so nothing on the page can point the
//! updater at another file or skip its signature check.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::events::UpdateChanged;
use crate::state::AppState;

/// After start, so the check never competes with logging in and joining chat.
const FIRST_CHECK: Duration = Duration::from_secs(15);
/// How often the loop wakes to see whether a check is due. Short enough that
/// switching checks back on takes effect the same hour.
const TICK: Duration = Duration::from_secs(60 * 60);
/// How old the last check may get. Lurkn tends to run for days in the tray.
const INTERVAL: Duration = Duration::from_secs(12 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UpdateStatus {
    /// Not looked yet.
    Unknown,
    Checking,
    UpToDate,
    /// No answer: offline, GitHub unreachable, or no build for this system.
    CheckFailed {
        message: String,
    },
    Available {
        version: String,
        /// Whether Lurkn can replace itself where it is installed. A `deb` or
        /// `rpm`, and whatever a package manager installed, is updated the way
        /// it was installed instead.
        installable: bool,
    },
    Downloading {
        version: String,
        /// `None` until the size is known.
        percent: Option<u8>,
    },
    /// Downloaded and its signature checked; installing, then restarting.
    Installing {
        version: String,
    },
    InstallFailed {
        version: String,
        message: String,
    },
}

impl UpdateStatus {
    /// Something is in flight that a second request must not race.
    fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Installing { .. }
        )
    }
}

struct Inner {
    status: UpdateStatus,
    /// What the last check found, kept so installing does not ask again.
    pending: Option<Update>,
    checked: Option<Instant>,
}

/// Managed state. A plain mutex: it is never held across an `.await`.
pub struct Updates(Mutex<Inner>);

impl Default for Updates {
    fn default() -> Self {
        Self(Mutex::new(Inner {
            status: UpdateStatus::Unknown,
            pending: None,
            checked: None,
        }))
    }
}

impl Updates {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn status(app: &AppHandle) -> UpdateStatus {
    app.state::<Updates>().lock().status.clone()
}

/// Store a new status and tell every window.
fn publish(app: &AppHandle, status: UpdateStatus, pending: Option<Option<Update>>) {
    {
        let updates = app.state::<Updates>();
        let mut inner = updates.lock();
        inner.status = status.clone();
        if let Some(pending) = pending {
            inner.pending = pending;
        }
    }
    if let Err(error) = UpdateChanged(status).emit(app) {
        tracing::error!(%error, "could not emit the update status");
    }
}

/// Check now and then, while the reader wants it.
pub async fn run(app: AppHandle) {
    tokio::time::sleep(FIRST_CHECK).await;
    loop {
        if automatic(&app).await && due(&app) {
            check(&app).await;
        }
        tokio::time::sleep(TICK).await;
    }
}

/// Never on its own in a debug build: a development copy would always be told
/// about the last release.
async fn automatic(app: &AppHandle) -> bool {
    if cfg!(debug_assertions) {
        return false;
    }
    match app.try_state::<AppState>() {
        Some(state) => state.settings.read().await.updates.check,
        None => false,
    }
}

fn due(app: &AppHandle) -> bool {
    app.state::<Updates>()
        .lock()
        .checked
        .is_none_or(|at| at.elapsed() >= INTERVAL)
}

/// Ask GitHub for the latest release.
///
/// A newer version already found stays on screen while it is checked again,
/// and a failed check does not take it away.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let updates = app.state::<Updates>();
    let previous = {
        let mut inner = updates.lock();
        if inner.status.busy() {
            return inner.status.clone();
        }
        inner.checked = Some(Instant::now());
        inner.status.clone()
    };
    let quiet = matches!(previous, UpdateStatus::Available { .. });
    if !quiet {
        publish(app, UpdateStatus::Checking, None);
    }

    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => Err(error),
    };

    match found {
        Ok(Some(update)) => {
            tracing::info!(version = %update.version, "a newer Lurkn is available");
            let status = UpdateStatus::Available {
                version: update.version.clone(),
                installable: installable(),
            };
            publish(app, status.clone(), Some(Some(update)));
            status
        }
        Ok(None) => {
            publish(app, UpdateStatus::UpToDate, Some(None));
            UpdateStatus::UpToDate
        }
        Err(error) => {
            tracing::warn!(%error, "could not check for updates");
            let status = if quiet {
                previous
            } else {
                UpdateStatus::CheckFailed {
                    message: check_message(&error).to_owned(),
                }
            };
            publish(app, status.clone(), None);
            status
        }
    }
}

/// Download, verify and install what the last check found, then restart.
///
/// On Windows the installer takes over and Lurkn exits inside the plugin; it
/// starts again when the installer is done.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let update = {
        let inner = updates.lock();
        if inner.status.busy() {
            return Ok(());
        }
        let ready = matches!(
            inner.status,
            UpdateStatus::Available {
                installable: true,
                ..
            } | UpdateStatus::InstallFailed { .. }
        );
        match (&inner.pending, ready) {
            (Some(update), true) => update.clone(),
            _ => return Err("There is no update to install.".to_owned()),
        }
    };
    let version = update.version.clone();
    tracing::info!(%version, "downloading the update");
    publish(
        app,
        UpdateStatus::Downloading {
            version: version.clone(),
            percent: None,
        },
        None,
    );

    let progress = app.clone();
    let mut received: u64 = 0;
    let mut shown: Option<u8> = None;
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let percent = total
                    .filter(|total| *total > 0)
                    .map(|total| (received.saturating_mul(100) / total).min(100) as u8);
                // One event per percent, not per chunk.
                if percent != shown {
                    shown = percent;
                    let status = UpdateStatus::Downloading {
                        version: version.clone(),
                        percent,
                    };
                    progress.state::<Updates>().lock().status = status.clone();
                    let _ = UpdateChanged(status).emit(&progress);
                }
            },
            || {},
        )
        .await;

    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(error) => return Err(failed(app, &version, &error)),
    };

    publish(
        app,
        UpdateStatus::Installing {
            version: version.clone(),
        },
        None,
    );
    let installed = tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await;
    match installed {
        Ok(Ok(())) => {
            tracing::info!(%version, "update installed; restarting");
            app.request_restart();
            Ok(())
        }
        Ok(Err(error)) => Err(failed(app, &version, &error)),
        Err(error) => {
            tracing::error!(%error, "the install task failed");
            let message = "Could not install it here.".to_owned();
            publish(
                app,
                UpdateStatus::InstallFailed {
                    version,
                    message: message.clone(),
                },
                None,
            );
            Err(message)
        }
    }
}

fn failed(app: &AppHandle, version: &str, error: &tauri_plugin_updater::Error) -> String {
    tracing::error!(%error, %version, "could not install the update");
    let message = install_message(error).to_owned();
    publish(
        app,
        UpdateStatus::InstallFailed {
            version: version.to_owned(),
            message: message.clone(),
        },
        None,
    );
    message
}

/// Whether Lurkn can replace itself in place.
///
/// The AppImage, the Windows installer and the Mac app can. A `deb` or `rpm`
/// would need root and would go behind the package manager's back, and the
/// AUR package is built from the `deb`, so all three only say an update is
/// out. A debug build has no installed copy to replace.
fn installable() -> bool {
    use tauri::utils::config::BundleType;

    if cfg!(debug_assertions) {
        return false;
    }
    match tauri::utils::platform::bundle_type() {
        // The plugin replaces the file this variable names.
        Some(BundleType::AppImage) => std::env::var_os("APPIMAGE").is_some(),
        Some(BundleType::Nsis | BundleType::Msi | BundleType::App) => true,
        _ => false,
    }
}

fn check_message(error: &tauri_plugin_updater::Error) -> &'static str {
    use tauri_plugin_updater::Error;

    match error {
        Error::Reqwest(_) | Error::Network(_) => "Could not reach GitHub.",
        // Also what a release page without `latest.json` comes back as.
        Error::ReleaseNotFound | Error::Serialization(_) => "GitHub had no release to offer.",
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            "The latest release has no download for this system yet."
        }
        _ => "Could not check for updates.",
    }
}

fn install_message(error: &tauri_plugin_updater::Error) -> &'static str {
    use tauri_plugin_updater::Error;

    match error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => {
            "The download is not signed by Lurkn, so it was not installed."
        }
        Error::Reqwest(_) | Error::Network(_) => "The download did not finish.",
        _ => "Could not install it here.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_work_in_flight_is_busy() {
        assert!(UpdateStatus::Checking.busy());
        assert!(UpdateStatus::Installing {
            version: "0.2.0".into()
        }
        .busy());
        assert!(!UpdateStatus::Available {
            version: "0.2.0".into(),
            installable: true
        }
        .busy());
        assert!(!UpdateStatus::InstallFailed {
            version: "0.2.0".into(),
            message: String::new()
        }
        .busy());
    }

    #[test]
    fn a_debug_build_never_installs_itself() {
        assert!(!installable());
    }

    /// The plugin reads this when the app starts, and a typo there would stop
    /// Lurkn from starting at all, so it is parsed here first.
    #[test]
    fn the_updater_config_parses() {
        let file: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let config: tauri_plugin_updater::Config =
            serde_json::from_value(file["plugins"]["updater"].clone()).unwrap();

        assert!(config.require_signed_version);
        assert!(!config.pubkey.is_empty());
        assert!(config.endpoints.iter().all(|url| url.scheme() == "https"));
    }

    #[test]
    fn a_bad_signature_says_so() {
        let error = tauri_plugin_updater::Error::MissingSignedVersion;
        assert!(install_message(&error).contains("not signed"));
    }
}

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Browser launching: the system default, a browser found installed, or a
//! command of the reader's own. See `docs/architecture.md` §7.

pub mod desktop;
pub mod detect;
pub mod family;
pub mod host;
pub mod launch;

pub use detect::{installed, Browser, BrowserInfo};
pub use family::Family;
pub use launch::Launch;

use lurkn_core::Login;
use url::Url;

pub type Result<T> = std::result::Result<T, BrowserError>;

/// No assumption of a particular browser, desktop or display server.
pub trait BrowserLauncher: Send + Sync {
    fn open_url(&self, url: &Url) -> Result<()>;
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    /// Rendered to the user as "Could not open your browser."
    /// The technical detail stays in the log view.
    #[error("could not hand the URL to the system browser: {detail}")]
    LaunchFailed { detail: String },
    #[error("refused to open a URL with scheme {scheme:?}")]
    DisallowedScheme { scheme: String },
    /// The browser named in settings is no longer on the machine.
    #[error("the browser chosen in settings is not installed any more")]
    Missing,
}

/// Schemes the launcher will hand to the OS. Everything else is refused, so a
/// hostile chat link can never reach `xdg-open`.
pub const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

pub fn check_scheme(url: &Url) -> Result<()> {
    if ALLOWED_SCHEMES.contains(&url.scheme()) {
        Ok(())
    } else {
        Err(BrowserError::DisallowedScheme {
            scheme: url.scheme().to_owned(),
        })
    }
}

/// Opens a channel page. Takes a validated [`Login`], never a raw string.
pub fn open_channel(launcher: &dyn BrowserLauncher, login: &Login) -> Result<()> {
    let url = lurkn_core::twitch_channel_url(login);
    check_scheme(&url)?;
    launcher.open_url(&url)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingLauncher {
        opened: Mutex<Vec<String>>,
    }

    impl BrowserLauncher for RecordingLauncher {
        fn open_url(&self, url: &Url) -> Result<()> {
            self.opened.lock().unwrap().push(url.to_string());
            Ok(())
        }
    }

    #[test]
    fn open_channel_builds_the_twitch_url() {
        let launcher = RecordingLauncher::default();
        let login = Login::parse("forsen").unwrap();
        open_channel(&launcher, &login).unwrap();
        assert_eq!(
            launcher.opened.lock().unwrap().as_slice(),
            ["https://www.twitch.tv/forsen"]
        );
    }

    #[test]
    fn refuses_non_http_schemes() {
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "ftp://x.test/",
        ] {
            let url = Url::parse(bad).unwrap();
            assert!(
                matches!(
                    check_scheme(&url),
                    Err(BrowserError::DisallowedScheme { .. })
                ),
                "should have refused {bad}"
            );
        }
    }

    #[test]
    fn allows_http_and_https() {
        assert!(check_scheme(&Url::parse("https://www.twitch.tv/forsen").unwrap()).is_ok());
        assert!(check_scheme(&Url::parse("http://example.test/").unwrap()).is_ok());
    }
}

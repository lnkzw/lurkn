//! Which engine a browser is built on, and therefore what command line it takes.
//!
//! Classification is by name because there is nothing better to go on: a
//! desktop entry says what to run, not what it is. Anything unrecognised is
//! [`Family::Other`] and gets a plain URL — a wrong guess would be a browser
//! that refuses to start.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    /// Gecko. Takes `--kiosk`, and has no app mode: the XUL `-app` flag was for
    /// running other XUL applications and is long gone.
    Firefox,
    /// Blink. Takes both `--app=URL` and `--kiosk`.
    Chromium,
    #[default]
    Other,
}

impl Family {
    /// A single window holding only the page.
    pub fn supports_app(self) -> bool {
        matches!(self, Self::Chromium)
    }

    /// Fullscreen, no browser chrome.
    pub fn supports_kiosk(self) -> bool {
        matches!(self, Self::Firefox | Self::Chromium)
    }

    /// `--new-window`, understood by both families.
    pub fn supports_new_window(self) -> bool {
        matches!(self, Self::Firefox | Self::Chromium)
    }
}

/// Distinctive enough to match anywhere in a command line, which is what makes
/// them work for Flatpak entries too (`app/org.mozilla.firefox/...`).
const FIREFOX_MARKERS: &[&str] = &[
    "firefox",
    "librewolf",
    "waterfox",
    "floorp",
    "icecat",
    "basilisk",
    "palemoon",
    "mullvad-browser",
    "tor-browser",
    "torbrowser",
    "zen-browser",
    "zen_browser",
];

const CHROMIUM_MARKERS: &[&str] = &[
    "chromium",
    "chrome",
    "brave",
    "vivaldi",
    "opera",
    "edge",
    "thorium",
    "iridium",
    "yandex",
    "ungoogled",
];

/// Guess a family from the program and its arguments.
///
/// The whole line is searched, not just the binary: a Flatpak or snap wrapper
/// puts the real application id in an argument.
pub fn classify(argv: &[String]) -> Family {
    let line = argv.join(" ").to_lowercase();

    // Zen ships its binary as plain `zen`, which is too short to search for in
    // a whole command line without matching unrelated paths.
    let program = argv.first().map(|p| basename(p)).unwrap_or_default();
    if program == "zen" {
        return Family::Firefox;
    }

    if FIREFOX_MARKERS.iter().any(|m| line.contains(m)) {
        return Family::Firefox;
    }
    if CHROMIUM_MARKERS.iter().any(|m| line.contains(m)) {
        return Family::Chromium;
    }
    Family::Other
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .trim_end_matches(".exe")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(line: &str) -> Vec<String> {
        line.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn recognises_gecko_browsers() {
        for line in [
            "/usr/bin/firefox",
            "/usr/lib/firefox-esr/firefox-esr",
            "librewolf",
            "/usr/bin/flatpak run --branch=stable app/org.mozilla.firefox/x86_64/stable",
            "/opt/zen/zen",
        ] {
            assert_eq!(classify(&argv(line)), Family::Firefox, "{line}");
        }
    }

    /// A Windows program path, and a Mac bundle with its identifier.
    #[test]
    fn recognises_browsers_on_windows_and_macos() {
        let cases: [(&[&str], Family); 6] = [
            (&[r"C:\Program Files\Zen Browser\zen.exe"], Family::Firefox),
            (
                &[r"C:\Program Files\LibreWolf\librewolf.exe"],
                Family::Firefox,
            ),
            (
                &[r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"],
                Family::Chromium,
            ),
            (
                &["/Applications/Google Chrome.app", "com.google.Chrome"],
                Family::Chromium,
            ),
            (
                &["/Applications/Zen.app", "app.zen-browser.zen"],
                Family::Firefox,
            ),
            (
                &["/Applications/Safari.app", "com.apple.Safari"],
                Family::Other,
            ),
        ];
        for (line, family) in cases {
            let argv: Vec<String> = line.iter().map(|a| (*a).to_owned()).collect();
            assert_eq!(classify(&argv), family, "{line:?}");
        }
    }

    #[test]
    fn recognises_blink_browsers() {
        for line in [
            "/usr/bin/chromium",
            "/opt/google/chrome/google-chrome",
            "/usr/bin/brave-browser",
            "/usr/bin/flatpak run app/com.microsoft.Edge/x86_64/stable",
            "vivaldi-stable",
        ] {
            assert_eq!(classify(&argv(line)), Family::Chromium, "{line}");
        }
    }

    #[test]
    fn anything_else_gets_a_plain_url() {
        for line in [
            "/usr/bin/epiphany",
            "/usr/bin/falkon",
            "/usr/bin/qutebrowser",
        ] {
            let family = classify(&argv(line));
            assert_eq!(family, Family::Other, "{line}");
            assert!(!family.supports_app());
            assert!(!family.supports_kiosk());
        }
    }

    /// `zen` is matched on the binary name alone, so an unrelated path holding
    /// those three letters must not be mistaken for a browser.
    #[test]
    fn a_short_name_does_not_match_a_path() {
        assert_eq!(classify(&argv("/usr/bin/zenity --info")), Family::Other);
    }

    #[test]
    fn both_families_can_be_told_to_open_a_window() {
        assert!(Family::Firefox.supports_new_window());
        assert!(Family::Chromium.supports_new_window());
        assert!(!Family::Other.supports_new_window());
    }

    #[test]
    fn only_chromium_has_an_app_mode() {
        assert!(Family::Chromium.supports_app());
        assert!(!Family::Firefox.supports_app());
        assert!(Family::Firefox.supports_kiosk());
        assert!(Family::Chromium.supports_kiosk());
    }
}

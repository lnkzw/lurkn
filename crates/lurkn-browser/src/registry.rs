//! Browsers on Windows: the ones registered under `StartMenuInternet`.
//!
//! A browser's installer registers it there so Windows can offer it as the
//! default: a key per browser, named by the browser, with its display name as
//! the key's default value and the program under `shell\open\command`. It is
//! where the "Default apps" settings get their list of browsers, the Windows
//! counterpart of the desktop entries Linux reads.
//!
//! The rules are plain functions over strings, so they are tested on every
//! platform; only the registry reading is Windows-only.

use crate::detect::{Browser, Start};
use crate::family::classify;
use crate::launch::{split_command, Quoting};

/// Build a browser from a `StartMenuInternet` registration, or `None` if its
/// program is not there.
///
/// `exists` decides whether a program is installed; it is a parameter so the
/// rules can be tested without touching the filesystem.
pub fn browser_from(
    id: &str,
    name: &str,
    command: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Option<Browser> {
    let argv = command_argv(command, exists)?;

    // A display name may be a resource reference (`@C:\…\x.dll,-101`), which
    // only the shell can resolve; the key's own name reads better than that.
    let name = name.trim();
    let name = if name.is_empty() || name.starts_with('@') {
        id
    } else {
        name
    };

    Some(Browser {
        id: id.to_owned(),
        name: name.to_owned(),
        family: classify(&argv),
        start: Start::Command(argv),
    })
}

/// The program a registered command runs, and the arguments it always gets.
///
/// The URL placeholders a protocol handler's command carries (`"%1"`) are left
/// out: Lurkn adds the URL itself. Some installers write a path with spaces
/// without quoting it; that is taken whole when it is a program and its first
/// word is not.
pub fn command_argv(command: &str, exists: &dyn Fn(&str) -> bool) -> Option<Vec<String>> {
    let mut argv = split_command(command, Quoting::Windows);
    argv.retain(|arg| !arg.is_empty() && !arg.contains("%1") && !arg.contains("%L"));

    match argv.first() {
        Some(program) if exists(program) => Some(argv),
        _ => {
            let whole = command.trim().trim_matches('"');
            exists(whole).then(|| vec![whole.to_owned()])
        }
    }
}

/// The installed browser whose program is `program`, compared the way Windows
/// compares paths: without case, either slash.
pub fn by_program(installed: &[Browser], program: &str) -> Option<String> {
    let same = |a: &str, b: &str| a.replace('/', "\\").eq_ignore_ascii_case(&b.replace('/', "\\"));
    installed
        .iter()
        .find(|browser| match &browser.start {
            Start::Command(argv) => argv.first().is_some_and(|own| same(own, program)),
            Start::Bundle(_) => false,
        })
        .map(|browser| browser.id.clone())
}

#[cfg(windows)]
pub use system::{default_id, installed};

#[cfg(windows)]
mod system {
    use windows_registry::{Key, CURRENT_USER, LOCAL_MACHINE};

    use super::{browser_from, by_program, command_argv};
    use crate::detect::{program_exists, Browser};

    const CLIENTS: &str = r"Software\Clients\StartMenuInternet";

    /// Every registered browser whose program is there. The user's own
    /// registrations come first, then the machine's, then a 32-bit installer's.
    pub fn installed() -> Vec<Browser> {
        let roots: [(&Key, &str); 3] = [
            (CURRENT_USER, CLIENTS),
            (LOCAL_MACHINE, CLIENTS),
            (LOCAL_MACHINE, r"Software\WOW6432Node\Clients\StartMenuInternet"),
        ];

        let mut found: Vec<Browser> = Vec::new();
        for (root, path) in roots {
            let Ok(clients) = root.open(path) else {
                continue;
            };
            let Ok(ids) = clients.keys() else {
                continue;
            };
            for id in ids {
                if found.iter().any(|b| b.id.eq_ignore_ascii_case(&id)) {
                    continue;
                }
                let Ok(key) = clients.open(&id) else {
                    continue;
                };
                let name = key.get_string("").unwrap_or_default();
                let Ok(command) = key
                    .open(r"shell\open\command")
                    .and_then(|command| command.get_string(""))
                else {
                    continue;
                };
                if let Some(browser) = browser_from(&id, &name, &command, &program_exists) {
                    found.push(browser);
                }
            }
        }
        found
    }

    /// The default browser: the program behind the class the user chose for
    /// `https`, matched to a registered browser.
    pub fn default_id(installed: &[Browser]) -> Option<String> {
        let class = chosen_class()?;
        // What `HKEY_CLASSES_ROOT` shows, spelled out: the user's classes over
        // the machine's. Wine does not merge the two there.
        let path = format!(r"Software\Classes\{class}\shell\open\command");
        let command = [CURRENT_USER, LOCAL_MACHINE]
            .into_iter()
            .find_map(|root| root.open(&path).and_then(|key| key.get_string("")).ok())?;
        let argv = command_argv(&command, &program_exists)?;
        by_program(installed, argv.first()?)
    }

    /// The class (`ChromeHTML`, `MSEdgeHTM`, …) the user chose for `https`.
    ///
    /// Windows 11 25H2 records the choice under `UserChoiceLatest` and stops
    /// updating `UserChoice`, which may then still name a browser chosen long
    /// ago, so the newer key is asked first. Where it keeps the class has been
    /// seen both as a value and as a subkey's default.
    fn chosen_class() -> Option<String> {
        let https = CURRENT_USER
            .open(r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https")
            .ok()?;
        let read = |path: &str, value: &str| {
            https
                .open(path)
                .and_then(|key| key.get_string(value))
                .ok()
                .map(|class| class.trim().to_owned())
                .filter(|class| !class.is_empty())
        };
        read("UserChoiceLatest", "ProgId")
            .or_else(|| read(r"UserChoiceLatest\ProgId", ""))
            .or_else(|| read("UserChoice", "ProgId"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::Family;

    const CHROME: &str = r"C:\Program Files\Google\Chrome\Application\chrome.exe";

    fn present(_: &str) -> bool {
        true
    }
    fn only_chrome(path: &str) -> bool {
        path == CHROME
    }

    #[test]
    fn a_registration_becomes_a_browser() {
        let browser = browser_from(
            "Google Chrome",
            "Google Chrome",
            &format!(r#""{CHROME}""#),
            &present,
        )
        .unwrap();

        assert_eq!(browser.id, "Google Chrome");
        assert_eq!(browser.family, Family::Chromium);
        assert_eq!(browser.start, Start::Command(vec![CHROME.to_owned()]));
    }

    #[test]
    fn a_missing_program_is_skipped() {
        assert!(browser_from("Gone", "Gone", r#""C:\Gone\gone.exe""#, &only_chrome).is_none());
    }

    #[test]
    fn a_resource_name_falls_back_to_the_key() {
        let browser = browser_from(
            "LibreWolf-2A1B",
            r"@C:\Program Files\LibreWolf\librewolf.exe,-1",
            r#""C:\Program Files\LibreWolf\librewolf.exe""#,
            &present,
        )
        .unwrap();
        assert_eq!(browser.name, "LibreWolf-2A1B");
        assert_eq!(browser.family, Family::Firefox);
    }

    #[test]
    fn the_url_placeholder_is_left_out() {
        let argv = command_argv(
            r#""C:\Program Files\LibreWolf\librewolf.exe" -osint -url "%1""#,
            &present,
        )
        .unwrap();
        assert_eq!(
            argv,
            [r"C:\Program Files\LibreWolf\librewolf.exe", "-osint", "-url"]
        );
    }

    #[test]
    fn an_unquoted_path_with_spaces_is_taken_whole() {
        assert_eq!(command_argv(CHROME, &only_chrome).unwrap(), [CHROME]);
    }

    #[test]
    fn the_default_is_found_by_its_program() {
        let browsers = [
            browser_from("Firefox-308046B0AF4A39CB", "Firefox", r#""C:\Firefox\firefox.exe""#, &present)
                .unwrap(),
            browser_from("Google Chrome", "Google Chrome", &format!(r#""{CHROME}""#), &present)
                .unwrap(),
        ];
        assert_eq!(
            by_program(&browsers, &CHROME.to_lowercase()),
            Some("Google Chrome".to_owned())
        );
        assert_eq!(by_program(&browsers, r"C:\Other\other.exe"), None);
    }
}

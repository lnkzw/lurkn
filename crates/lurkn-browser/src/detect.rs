//! Finding the browsers installed on this machine.
//!
//! The system tells us, in the place its own "default browser" choice is built
//! from, so what Lurkn offers matches what the user expects to be there:
//!
//! - Linux: desktop entries whose `MimeType` claims web links (here);
//! - Windows: the browsers registered under `StartMenuInternet` (`crate::registry`);
//! - macOS: applications whose `Info.plist` claims `https` (`crate::bundles`).
//!
//! Nothing here is Lurkn-specific and nothing is hard-coded per browser. A
//! browser the system does not know about will not be listed — the custom
//! command exists for exactly that case.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::desktop::{self, DesktopEntry};
use crate::family::{classify, Family};

/// An installed browser Lurkn can launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browser {
    /// Stable across restarts, which a path or an index would not be: the
    /// desktop entry's file name (`firefox.desktop`), the registered name on
    /// Windows (`Google Chrome`), the bundle identifier on macOS
    /// (`org.mozilla.firefox`).
    pub id: String,
    pub name: String,
    pub family: Family,
    pub start: Start,
}

/// How a browser is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// A program and the arguments it always gets; flags and the URL follow.
    /// On Linux the desktop's field codes are already removed.
    Command(Vec<String>),
    /// A macOS application bundle, by path, started through `open`.
    Bundle(String),
}

impl Browser {
    /// `default_id` is the system default's id, if it is known.
    pub fn info(&self, default_id: Option<&str>) -> BrowserInfo {
        BrowserInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            family: self.family,
            supports_app: self.family.supports_app(),
            supports_kiosk: self.family.supports_kiosk(),
            is_default: default_id == Some(self.id.as_str()),
        }
    }
}

/// What the settings screen needs to know about a browser. The command line
/// itself stays in the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInfo {
    pub id: String,
    pub name: String,
    pub family: Family,
    pub supports_app: bool,
    pub supports_kiosk: bool,
    /// The one the system opens web links with.
    pub is_default: bool,
}

/// Every browser the system knows of, sorted by name.
pub fn installed() -> Vec<Browser> {
    #[cfg(windows)]
    let mut found = crate::registry::installed();
    #[cfg(target_os = "macos")]
    let mut found = crate::bundles::installed();
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut found = desktop_entries();

    found.sort_by_key(|b| b.name.to_lowercase());
    found
}

/// The id of the system's default browser, if it is one of `installed`.
///
/// Used to honour app and kiosk mode for the "system default" choice, which
/// otherwise could not take flags at all, and to tell the settings screen
/// which browser that is.
pub fn default_id(installed: &[Browser]) -> Option<String> {
    #[cfg(windows)]
    {
        crate::registry::default_id(installed)
    }
    #[cfg(target_os = "macos")]
    {
        crate::bundles::default_id(installed)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        xdg_default(installed)
    }
}

/// Asked of `xdg-mime` rather than guessed: it is what `xdg-open` itself goes by.
#[cfg(not(any(windows, target_os = "macos")))]
fn xdg_default(installed: &[Browser]) -> Option<String> {
    let output = crate::host::command("xdg-mime")
        .args(["query", "default", "x-scheme-handler/https"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let id = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    installed.iter().any(|b| b.id == id).then_some(id)
}

/// Build a browser from a parsed entry, or `None` if it is not one.
///
/// `exists` decides whether the program is actually installed; it is a
/// parameter so the rules can be tested without touching the filesystem.
pub fn browser_from(
    id: &str,
    entry: &DesktopEntry,
    exists: &dyn Fn(&str) -> bool,
) -> Option<Browser> {
    if !entry.is_visible() || !entry.handles_web() {
        return None;
    }

    let argv = desktop::parse_exec(&entry.exec);
    let program = argv.first()?;

    // TryExec is the entry's own "is this installed" check; honour it, and fall
    // back to the program when it is absent.
    let probe = if entry.try_exec.is_empty() {
        program.as_str()
    } else {
        entry.try_exec.as_str()
    };
    if !exists(probe) {
        return None;
    }

    Some(Browser {
        id: id.to_owned(),
        name: if entry.name.is_empty() {
            id.trim_end_matches(".desktop").to_owned()
        } else {
            entry.name.clone()
        },
        family: classify(&argv),
        start: Start::Command(argv),
    })
}

/// Directories holding desktop entries, most specific first.
///
/// Per the XDG base directory spec: the user's own entries override the
/// system's, and a later duplicate of the same file name is ignored.
pub fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        dirs.push(PathBuf::from(home).join("applications"));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());

    for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(Path::new(dir).join("applications"));
    }

    dirs
}

/// Whether a program exists: a path as written, a bare name on PATH.
pub fn program_exists(program: &str) -> bool {
    if program.contains('/') || (cfg!(windows) && program.contains('\\')) {
        return Path::new(program).is_file();
    }

    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let file = dir.join(program);
        file.is_file() || (cfg!(windows) && dir.join(format!("{program}.exe")).is_file())
    })
}

/// Every desktop entry that is a browser. The first entry of a name wins.
#[cfg(not(any(windows, target_os = "macos")))]
fn desktop_entries() -> Vec<Browser> {
    let mut found: Vec<Browser> = Vec::new();

    for dir in application_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "desktop") {
                continue;
            }
            let Some(id) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // An entry of the same name in an earlier directory wins.
            if found.iter().any(|b| b.id == id) {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };

            if let Some(browser) = browser_from(id, &desktop::parse(&contents), &program_exists) {
                found.push(browser);
            }
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(exec: &str, mime: &str) -> DesktopEntry {
        desktop::parse(&format!(
            "[Desktop Entry]\nName=Test Browser\nExec={exec}\nMimeType={mime}\n"
        ))
    }

    fn present(_: &str) -> bool {
        true
    }
    fn absent(_: &str) -> bool {
        false
    }

    #[test]
    fn a_web_handler_becomes_a_browser() {
        let browser = browser_from(
            "firefox.desktop",
            &entry("/usr/bin/firefox %u", "x-scheme-handler/https;"),
            &present,
        )
        .unwrap();

        assert_eq!(browser.id, "firefox.desktop");
        assert_eq!(browser.name, "Test Browser");
        assert_eq!(browser.family, Family::Firefox);
        assert_eq!(
            browser.start,
            Start::Command(vec!["/usr/bin/firefox".into()])
        );
        assert!(browser.info(None).supports_kiosk);
        assert!(!browser.info(None).supports_app);
        assert!(!browser.info(None).is_default);
        assert!(browser.info(Some("firefox.desktop")).is_default);
    }

    #[test]
    fn an_entry_that_does_not_open_links_is_skipped() {
        assert!(browser_from(
            "w.desktop",
            &entry("/usr/bin/writer %f", "text/plain;"),
            &present
        )
        .is_none());
    }

    #[test]
    fn an_uninstalled_program_is_skipped() {
        assert!(browser_from(
            "gone.desktop",
            &entry("/usr/bin/gone %u", "x-scheme-handler/https;"),
            &absent
        )
        .is_none());
    }

    #[test]
    fn a_hidden_entry_is_skipped() {
        let hidden = desktop::parse(
            "[Desktop Entry]\nName=X\nExec=/usr/bin/x %u\nMimeType=x-scheme-handler/https;\nNoDisplay=true\n",
        );
        assert!(browser_from("x.desktop", &hidden, &present).is_none());
    }

    #[test]
    fn a_nameless_entry_falls_back_to_its_id() {
        let nameless = desktop::parse(
            "[Desktop Entry]\nExec=/usr/bin/x %u\nMimeType=x-scheme-handler/https;\n",
        );
        assert_eq!(
            browser_from("x.desktop", &nameless, &present).unwrap().name,
            "x"
        );
    }

    /// Prints what this machine actually has. Ignored by default: the result
    /// depends on the machine, so it can only be read, not asserted.
    /// `cargo test -p lurkn-browser -- --ignored --nocapture`
    #[test]
    #[ignore = "depends on what is installed"]
    fn list_this_machines_browsers() {
        let browsers = installed();
        println!("default: {:?}", default_id(&browsers));
        for browser in browsers {
            println!(
                "{:<28} {:<16} {:?}  {:?}",
                browser.id, browser.name, browser.family, browser.start
            );
        }
    }

    #[test]
    fn the_user_directory_comes_first() {
        let dirs = application_dirs();
        assert!(dirs.len() >= 2);
        assert!(
            dirs[0].ends_with("applications"),
            "every directory is an applications directory"
        );
    }
}

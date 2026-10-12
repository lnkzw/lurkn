//! Browsers on macOS: the applications that say they open `https` links.
//!
//! An application declares the URL schemes it handles in its `Info.plist`
//! (`CFBundleURLTypes`), and those declarations are what LaunchServices offers
//! as default browsers. Lurkn reads the same declarations from the application
//! folders.
//!
//! The rules are plain functions over parsed property lists, so they are
//! tested on every platform; only the folders and files are macOS-only.

use plist::{Dictionary, Value};

use crate::detect::{Browser, Start};
use crate::family::classify;

/// What LaunchServices uses when nothing else was ever chosen.
pub const SAFARI: &str = "com.apple.safari";

/// Build a browser from an application's `Info.plist`, or `None` if it does
/// not open web links.
pub fn browser_from(path: &str, info: &Dictionary) -> Option<Browser> {
    if !handles_web(info) {
        return None;
    }
    let id = text(info, "CFBundleIdentifier")?;

    let name = text(info, "CFBundleDisplayName")
        .or_else(|| text(info, "CFBundleName"))
        .unwrap_or_else(|| {
            let file = path.rsplit('/').next().unwrap_or(path);
            file.trim_end_matches(".app").to_owned()
        });

    Some(Browser {
        // The identifier names the browser, the path only where it is: both
        // are searched for what it is built on.
        family: classify(&[path.to_owned(), id.clone()]),
        id,
        name,
        start: Start::Bundle(path.to_owned()),
    })
}

fn handles_web(info: &Dictionary) -> bool {
    info.get("CFBundleURLTypes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_dictionary)
        .filter_map(|kind| kind.get("CFBundleURLSchemes")?.as_array())
        .flatten()
        .filter_map(Value::as_string)
        .any(|scheme| scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
}

fn text(info: &Dictionary, key: &str) -> Option<String> {
    info.get(key)
        .and_then(Value::as_string)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The bundle identifier LaunchServices opens `https` with, from its record of
/// the user's choices. No record for `https` means the choice was never
/// changed, which leaves Safari.
pub fn chosen_handler(record: &Value) -> String {
    record
        .as_dictionary()
        .and_then(|record| record.get("LSHandlers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_dictionary)
        .find(|handler| {
            handler
                .get("LSHandlerURLScheme")
                .and_then(Value::as_string)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https"))
        })
        .and_then(|handler| text(handler, "LSHandlerRoleAll"))
        .unwrap_or_else(|| SAFARI.to_owned())
}

/// The installed browser with this identifier. LaunchServices keeps its
/// record in lower case; bundles are written any way.
pub fn by_identifier(installed: &[Browser], id: &str) -> Option<String> {
    installed
        .iter()
        .find(|browser| browser.id.eq_ignore_ascii_case(id))
        .map(|browser| browser.id.clone())
}

#[cfg(target_os = "macos")]
pub use system::{default_id, installed};

#[cfg(target_os = "macos")]
mod system {
    use std::path::PathBuf;

    use super::{browser_from, by_identifier, chosen_handler, SAFARI};
    use crate::detect::Browser;

    /// The user's own applications first, then the machine's.
    fn folders() -> Vec<PathBuf> {
        let mut folders = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(PathBuf::from(home).join("Applications"));
        }
        folders.push(PathBuf::from("/Applications"));
        folders
    }

    pub fn installed() -> Vec<Browser> {
        let mut found: Vec<Browser> = Vec::new();
        for folder in folders() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "app") {
                    continue;
                }
                let Some(text) = path.to_str() else {
                    continue;
                };
                let Ok(info) = plist::Value::from_file(path.join("Contents/Info.plist")) else {
                    continue;
                };
                let Some(info) = info.as_dictionary() else {
                    continue;
                };
                if let Some(browser) = browser_from(text, info) {
                    // The same browser in both folders: the user's copy wins.
                    if !found.iter().any(|b| b.id.eq_ignore_ascii_case(&browser.id)) {
                        found.push(browser);
                    }
                }
            }
        }
        found
    }

    pub fn default_id(installed: &[Browser]) -> Option<String> {
        let home = std::env::var_os("HOME")?;
        let record = PathBuf::from(home).join(
            "Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist",
        );
        let handler = if record.exists() {
            chosen_handler(&plist::Value::from_file(&record).ok()?)
        } else {
            SAFARI.to_owned()
        };
        by_identifier(installed, &handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::Family;

    fn parse(xml: &str) -> Value {
        Value::from_reader_xml(xml.as_bytes()).unwrap()
    }

    fn info(body: &str) -> Dictionary {
        parse(&format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>{body}</dict></plist>"#
        ))
        .into_dictionary()
        .unwrap()
    }

    const WEB: &str = "<key>CFBundleURLTypes</key><array><dict>\
        <key>CFBundleURLSchemes</key><array><string>http</string><string>https</string></array>\
        </dict></array>";

    #[test]
    fn an_app_that_opens_web_links_is_a_browser() {
        let browser = browser_from(
            "/Applications/Google Chrome.app",
            &info(&format!(
                "<key>CFBundleIdentifier</key><string>com.google.Chrome</string>\
                 <key>CFBundleName</key><string>Chrome</string>\
                 <key>CFBundleDisplayName</key><string>Google Chrome</string>{WEB}"
            )),
        )
        .unwrap();

        assert_eq!(browser.id, "com.google.Chrome");
        assert_eq!(browser.name, "Google Chrome");
        assert_eq!(browser.family, Family::Chromium);
        assert_eq!(
            browser.start,
            Start::Bundle("/Applications/Google Chrome.app".to_owned())
        );
    }

    #[test]
    fn an_app_that_does_not_is_skipped() {
        assert!(browser_from(
            "/Applications/Notes.app",
            &info("<key>CFBundleIdentifier</key><string>com.apple.Notes</string>"),
        )
        .is_none());
    }

    #[test]
    fn a_nameless_app_is_named_by_its_folder() {
        let browser = browser_from(
            "/Applications/Zen.app",
            &info(&format!(
                "<key>CFBundleIdentifier</key><string>app.zen-browser.zen</string>{WEB}"
            )),
        )
        .unwrap();
        assert_eq!(browser.name, "Zen");
        assert_eq!(browser.family, Family::Firefox);
    }

    #[test]
    fn the_chosen_handler_is_read_from_the_record() {
        let record = parse(
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
            <key>LSHandlers</key><array>
              <dict><key>LSHandlerContentType</key><string>public.html</string>
                    <key>LSHandlerRoleAll</key><string>com.apple.safari</string></dict>
              <dict><key>LSHandlerURLScheme</key><string>https</string>
                    <key>LSHandlerRoleAll</key><string>org.mozilla.firefox</string></dict>
            </array></dict></plist>"#,
        );
        assert_eq!(chosen_handler(&record), "org.mozilla.firefox");
    }

    #[test]
    fn no_choice_leaves_safari() {
        let record = parse(
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
            <key>LSHandlers</key><array/></dict></plist>"#,
        );
        assert_eq!(chosen_handler(&record), SAFARI);
    }

    #[test]
    fn the_record_matches_without_case() {
        let chrome = browser_from(
            "/Applications/Google Chrome.app",
            &info(&format!(
                "<key>CFBundleIdentifier</key><string>com.google.Chrome</string>{WEB}"
            )),
        )
        .unwrap();
        assert_eq!(
            by_identifier(&[chrome], "com.google.chrome"),
            Some("com.google.Chrome".to_owned())
        );
    }
}

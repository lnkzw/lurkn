//! Just enough of the freedesktop desktop-entry format to find browsers.
//!
//! Only the `[Desktop Entry]` group is read, and only the handful of keys that
//! decide whether an entry is a browser and how to run it. A full INI parser
//! would be more code for no gain.

/// One parsed `.desktop` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub try_exec: String,
    pub mime_types: Vec<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
}

impl DesktopEntry {
    /// Whether the desktop would offer this entry to the user at all.
    pub fn is_visible(&self) -> bool {
        !self.no_display && !self.hidden && !self.terminal
    }

    /// Whether the entry claims to open web links.
    pub fn handles_web(&self) -> bool {
        self.mime_types
            .iter()
            .any(|m| m == "x-scheme-handler/https" || m == "x-scheme-handler/http")
    }
}

/// Parse the `[Desktop Entry]` group of a desktop file.
///
/// Localised keys (`Name[de]`) are ignored in favour of the unlocalised one:
/// the value is shown next to a list of browsers, where the English name is
/// what people recognise, and picking a locale here would need the whole
/// fallback chain to be right.
pub fn parse(contents: &str) -> DesktopEntry {
    let mut entry = DesktopEntry::default();
    let mut inside = false;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        if !inside {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());

        match key {
            "Name" => entry.name = value.to_owned(),
            "Exec" => entry.exec = value.to_owned(),
            "TryExec" => entry.try_exec = value.to_owned(),
            "MimeType" => {
                entry.mime_types = value
                    .split(';')
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            "NoDisplay" => entry.no_display = value == "true",
            "Hidden" => entry.hidden = value == "true",
            "Terminal" => entry.terminal = value == "true",
            _ => {}
        }
    }

    entry
}

/// Split an `Exec` value into arguments, dropping the field codes.
///
/// Field codes (`%u`, `%F`, `%i`, …) are where the desktop would substitute the
/// file or URL being opened. Lurkn appends the URL itself, in the position the
/// chosen mode needs, so every code is removed here. `@@`/`@@u` are Flatpak's
/// own markers around them and go the same way.
pub fn parse_exec(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                // Inside quotes a backslash escapes the next character.
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ' ' | '\t' if !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            '%' => {
                match chars.next() {
                    // `%%` is a literal percent sign.
                    Some('%') => {
                        current.push('%');
                        started = true;
                    }
                    // Any other code is a substitution Lurkn makes itself.
                    Some(_) => {}
                    None => {}
                }
            }
            _ => {
                current.push(c);
                started = true;
            }
        }
    }

    if started {
        args.push(current);
    }

    args.retain(|a| !a.is_empty() && a != "@@" && a != "@@u" && a != "@@U");
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "\
[Desktop Entry]
Version=1.0
Name=Firefox
# a comment
Exec=/usr/lib/firefox/firefox %u
TryExec=/usr/lib/firefox/firefox
Terminal=false
MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;
Categories=Network;WebBrowser;

[Desktop Action new-window]
Name=New Window
Exec=/usr/lib/firefox/firefox --new-window %u
";

    #[test]
    fn reads_the_main_group_only() {
        let entry = parse(FIREFOX);
        assert_eq!(entry.name, "Firefox");
        assert_eq!(entry.exec, "/usr/lib/firefox/firefox %u");
        assert_eq!(entry.try_exec, "/usr/lib/firefox/firefox");
        assert!(entry.handles_web());
        assert!(entry.is_visible());
    }

    #[test]
    fn an_action_group_does_not_overwrite_the_entry() {
        // The action's Exec comes later in the file; taking it would open a
        // second window with the wrong flags.
        assert!(!parse(FIREFOX).exec.contains("--new-window"));
    }

    #[test]
    fn hidden_entries_are_not_visible() {
        for key in ["NoDisplay=true", "Hidden=true", "Terminal=true"] {
            let entry = parse(&format!("[Desktop Entry]\nName=X\n{key}\n"));
            assert!(!entry.is_visible(), "{key}");
        }
    }

    #[test]
    fn an_entry_without_a_web_scheme_is_not_a_browser() {
        let entry = parse("[Desktop Entry]\nName=Writer\nMimeType=application/vnd.oasis;\n");
        assert!(!entry.handles_web());
    }

    #[test]
    fn exec_drops_the_field_codes() {
        assert_eq!(
            parse_exec("/usr/lib/firefox/firefox %u"),
            ["/usr/lib/firefox/firefox"]
        );
        assert_eq!(
            parse_exec("chromium --incognito %U"),
            ["chromium", "--incognito"]
        );
    }

    #[test]
    fn exec_keeps_quoted_arguments_whole() {
        assert_eq!(
            parse_exec(r#""/opt/My Browser/browser" --flag="a b" %u"#),
            ["/opt/My Browser/browser", "--flag=a b"]
        );
    }

    #[test]
    fn exec_unwraps_flatpak_markers() {
        assert_eq!(
            parse_exec("/usr/bin/flatpak run --branch=stable app/org.mozilla.firefox/x86_64/stable @@u %u @@"),
            [
                "/usr/bin/flatpak",
                "run",
                "--branch=stable",
                "app/org.mozilla.firefox/x86_64/stable"
            ]
        );
    }

    #[test]
    fn exec_keeps_a_literal_percent() {
        assert_eq!(
            parse_exec("browser --tag=100%% %u"),
            ["browser", "--tag=100%"]
        );
    }
}

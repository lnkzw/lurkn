//! Turning a choice into an argv, and running it.
//!
//! Nothing here goes through a shell. The program and each argument are passed
//! to the OS separately, so a space, a quote or a semicolon in a command or a
//! URL is data and can never become another command. See `docs/architecture.md` §11.

use lurkn_core::LaunchMode;
use url::Url;

use crate::desktop;
use crate::detect::Browser;
use crate::family::Family;
use crate::{BrowserError, Result};

/// A program and its arguments, ready to spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

/// The mode a browser will actually run in.
///
/// A request the browser cannot honour is stepped down rather than refused:
/// asking Firefox for app mode should still open the stream.
pub fn effective_mode(family: Family, wanted: LaunchMode) -> LaunchMode {
    match wanted {
        LaunchMode::Normal => LaunchMode::Normal,
        // Firefox has no app mode, and kiosk is the closest thing it has to
        // "just the page".
        LaunchMode::App if family.supports_app() => LaunchMode::App,
        LaunchMode::App if family.supports_kiosk() => LaunchMode::Kiosk,
        LaunchMode::Kiosk if family.supports_kiosk() => LaunchMode::Kiosk,
        _ => LaunchMode::Normal,
    }
}

/// Build the command line for an installed browser.
///
/// `new_window` asks for a window of its own instead of a tab in one that is
/// already open. It is only added in normal mode: app and kiosk already open a
/// separate window, and asking for both confuses Chromium.
pub fn for_browser(
    browser: &Browser,
    url: &Url,
    wanted: LaunchMode,
    new_window: bool,
) -> Result<Launch> {
    let mut argv = browser.argv.clone();
    if argv.is_empty() {
        return Err(BrowserError::LaunchFailed {
            detail: format!("{} has no command to run", browser.name),
        });
    }

    let program = argv.remove(0);
    match effective_mode(browser.family, wanted) {
        // Chromium wants the URL inside the flag; a separate argument opens an
        // ordinary window instead.
        LaunchMode::App => argv.push(format!("--app={url}")),
        LaunchMode::Kiosk => {
            argv.push("--kiosk".to_owned());
            argv.push(url.to_string());
        }
        LaunchMode::Normal => {
            if new_window && browser.family.supports_new_window() {
                argv.push("--new-window".to_owned());
            }
            argv.push(url.to_string());
        }
    }

    Ok(Launch {
        program,
        args: argv,
    })
}

/// Build the command line for a user-supplied command.
///
/// `%u` marks where the URL goes — the same code desktop entries use. Without
/// one the URL is appended, which is what almost every program expects.
pub fn for_command(line: &str, url: &Url) -> Result<Launch> {
    // Field codes are meaningful here, so the desktop parser's stripping is not
    // wanted: split on the same quoting rules, then substitute.
    let mut argv = split_command(line);
    argv.retain(|a| !a.is_empty());

    if argv.is_empty() {
        return Err(BrowserError::LaunchFailed {
            detail: "the command is empty".to_owned(),
        });
    }

    let program = argv.remove(0);
    if program.starts_with('%') {
        return Err(BrowserError::LaunchFailed {
            detail: "the command must start with a program".to_owned(),
        });
    }

    let mut substituted = false;
    for arg in &mut argv {
        if arg.contains("%u") || arg.contains("%U") {
            *arg = arg.replace("%u", url.as_str()).replace("%U", url.as_str());
            substituted = true;
        }
    }
    if !substituted {
        argv.push(url.to_string());
    }

    Ok(Launch {
        program,
        args: argv,
    })
}

/// Split a command line into arguments, honouring quotes but expanding nothing.
///
/// No globbing, no variable expansion, no operators: `~`, `$HOME`, `&&` and `|`
/// are literal text here. Anything that needs them can point at a script.
fn split_command(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' if quote.is_none() => {
                quote = Some(c);
                started = true;
            }
            c if Some(c) == quote => {
                quote = None;
            }
            '\\' if quote != Some('\'') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    started = true;
                }
            }
            ' ' | '\t' if quote.is_none() => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
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
    args
}

/// Start a program without waiting for it.
///
/// The child is detached from Lurkn's own streams: a browser writing to stderr
/// must not end up in Lurkn's terminal, and a browser reading stdin must not
/// steal it. The exit status is collected on a thread purely so the process
/// table does not fill with zombies over a long session.
pub fn spawn(launch: &Launch) -> Result<()> {
    use std::process::{Command, Stdio};

    let child = Command::new(&launch.program)
        .args(&launch.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| BrowserError::LaunchFailed {
            detail: format!("{}: {error}", launch.program),
        })?;

    let program = launch.program.clone();
    std::thread::spawn(move || {
        let mut child = child;
        if let Err(error) = child.wait() {
            tracing::debug!(%error, program, "could not reap the browser process");
        }
    });

    Ok(())
}

/// Re-exported so callers can strip field codes from a desktop `Exec` without
/// reaching into the parser module.
pub use desktop::parse_exec;

#[cfg(test)]
mod tests {
    use super::*;

    fn url() -> Url {
        Url::parse("https://www.twitch.tv/forsen").unwrap()
    }

    fn browser(family: Family, argv: &[&str]) -> Browser {
        Browser {
            id: "test.desktop".into(),
            name: "Test".into(),
            family,
            argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    #[test]
    fn chromium_app_mode_puts_the_url_in_the_flag() {
        let launch = for_browser(
            &browser(Family::Chromium, &["/usr/bin/chromium"]),
            &url(),
            LaunchMode::App,
            false,
        )
        .unwrap();

        assert_eq!(launch.program, "/usr/bin/chromium");
        assert_eq!(launch.args, ["--app=https://www.twitch.tv/forsen"]);
    }

    #[test]
    fn kiosk_passes_the_url_separately() {
        for family in [Family::Chromium, Family::Firefox] {
            let launch = for_browser(
                &browser(family, &["/usr/bin/b"]),
                &url(),
                LaunchMode::Kiosk,
                false,
            )
            .unwrap();
            assert_eq!(launch.args, ["--kiosk", "https://www.twitch.tv/forsen"]);
        }
    }

    /// Firefox has no app mode; the request becomes the closest thing it has
    /// rather than an error or a silently ordinary window.
    #[test]
    fn firefox_app_mode_becomes_kiosk() {
        assert_eq!(
            effective_mode(Family::Firefox, LaunchMode::App),
            LaunchMode::Kiosk
        );
        let launch = for_browser(
            &browser(Family::Firefox, &["/usr/bin/firefox"]),
            &url(),
            LaunchMode::App,
            false,
        )
        .unwrap();
        assert_eq!(launch.args, ["--kiosk", "https://www.twitch.tv/forsen"]);
    }

    /// An unknown browser gets a plain URL: a flag it does not understand could
    /// stop it starting at all.
    #[test]
    fn an_unknown_browser_gets_no_flags() {
        for wanted in [LaunchMode::App, LaunchMode::Kiosk, LaunchMode::Normal] {
            assert_eq!(effective_mode(Family::Other, wanted), LaunchMode::Normal);
            let launch = for_browser(
                &browser(Family::Other, &["/usr/bin/b"]),
                &url(),
                wanted,
                true,
            )
            .unwrap();
            assert_eq!(launch.args, ["https://www.twitch.tv/forsen"]);
        }
    }

    #[test]
    fn a_browsers_own_flags_are_kept() {
        let launch = for_browser(
            &browser(Family::Chromium, &["/usr/bin/chromium", "--incognito"]),
            &url(),
            LaunchMode::Normal,
            false,
        )
        .unwrap();
        assert_eq!(launch.args, ["--incognito", "https://www.twitch.tv/forsen"]);
    }

    #[test]
    fn a_new_window_is_asked_for_only_in_normal_mode() {
        let chromium = browser(Family::Chromium, &["/usr/bin/chromium"]);

        let normal = for_browser(&chromium, &url(), LaunchMode::Normal, true).unwrap();
        assert_eq!(
            normal.args,
            ["--new-window", "https://www.twitch.tv/forsen"]
        );

        // App and kiosk already open a window of their own, and asking for both
        // confuses Chromium.
        for mode in [LaunchMode::App, LaunchMode::Kiosk] {
            let launch = for_browser(&chromium, &url(), mode, true).unwrap();
            assert!(
                !launch.args.iter().any(|a| a == "--new-window"),
                "{mode:?} must not also ask for a new window"
            );
        }
    }

    /// A browser that does not take the flag gets a plain URL rather than an
    /// argument it might refuse to start on.
    #[test]
    fn an_unknown_browser_is_not_told_to_open_a_window() {
        let launch = for_browser(
            &browser(Family::Other, &["/usr/bin/b"]),
            &url(),
            LaunchMode::Normal,
            true,
        )
        .unwrap();
        assert_eq!(launch.args, ["https://www.twitch.tv/forsen"]);
    }

    #[test]
    fn a_command_substitutes_the_placeholder() {
        let launch = for_command("mpv --no-border %u", &url()).unwrap();
        assert_eq!(launch.program, "mpv");
        assert_eq!(launch.args, ["--no-border", "https://www.twitch.tv/forsen"]);
    }

    #[test]
    fn a_command_without_a_placeholder_gets_the_url_appended() {
        let launch = for_command("streamlink", &url()).unwrap();
        assert_eq!(launch.program, "streamlink");
        assert_eq!(launch.args, ["https://www.twitch.tv/forsen"]);
    }

    #[test]
    fn a_placeholder_inside_an_argument_is_substituted() {
        let launch = for_command("open --url=%u", &url()).unwrap();
        assert_eq!(launch.args, ["--url=https://www.twitch.tv/forsen"]);
    }

    #[test]
    fn a_command_keeps_quoted_paths_whole() {
        let launch = for_command(r#""/opt/My Player/play" --flag %u"#, &url()).unwrap();
        assert_eq!(launch.program, "/opt/My Player/play");
        assert_eq!(launch.args, ["--flag", "https://www.twitch.tv/forsen"]);
    }

    /// Without a shell these are ordinary characters, so they land in one
    /// argument of one program rather than starting a second command.
    #[test]
    fn shell_operators_are_not_operators() {
        let launch = for_command("player 'a; rm -rf ~ && echo b' %u", &url()).unwrap();
        assert_eq!(launch.program, "player");
        assert_eq!(
            launch.args,
            ["a; rm -rf ~ && echo b", "https://www.twitch.tv/forsen"]
        );
    }

    #[test]
    fn an_empty_command_is_refused() {
        assert!(matches!(
            for_command("   ", &url()),
            Err(BrowserError::LaunchFailed { .. })
        ));
        assert!(matches!(
            for_command("%u", &url()),
            Err(BrowserError::LaunchFailed { .. })
        ));
    }
}

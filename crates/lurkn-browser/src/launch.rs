//! Turning a choice into an argv, and running it.
//!
//! Nothing here goes through a shell. The program and each argument are passed
//! to the OS separately, so a space, a quote or a semicolon in a command or a
//! URL is data and can never become another command. See `docs/architecture.md` §11.

use lurkn_core::LaunchMode;
use url::Url;

use crate::desktop;
use crate::detect::{Browser, Start};
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
    let mut extra = Vec::new();
    match effective_mode(browser.family, wanted) {
        // Chromium wants the URL inside the flag; a separate argument opens an
        // ordinary window instead.
        LaunchMode::App => extra.push(format!("--app={url}")),
        LaunchMode::Kiosk => {
            extra.push("--kiosk".to_owned());
            extra.push(url.to_string());
        }
        LaunchMode::Normal => {
            if new_window && browser.family.supports_new_window() {
                extra.push("--new-window".to_owned());
            }
            extra.push(url.to_string());
        }
    }

    match &browser.start {
        Start::Command(argv) => {
            let Some((program, own)) = argv.split_first() else {
                return Err(BrowserError::LaunchFailed {
                    detail: format!("{} has no command to run", browser.name),
                });
            };
            Ok(Launch {
                program: program.clone(),
                args: own.iter().cloned().chain(extra).collect(),
            })
        }
        Start::Bundle(app) => Ok(for_bundle(app, url, extra)),
    }
}

/// A Mac application, started by LaunchServices through `open` rather than by
/// running the program inside the bundle. A browser started as Lurkn's child
/// would count Lurkn as the app responsible for it, and macOS would ask for
/// camera or microphone access in Lurkn's name for as long as it ran.
///
/// A URL alone travels as an Apple event, which every browser takes and which
/// reaches a copy that is already running. Flags only reach a new process, so
/// `-n` starts one; Chromium and Firefox hand the request to the copy already
/// running and exit.
fn for_bundle(app: &str, url: &Url, extra: Vec<String>) -> Launch {
    let plain = matches!(extra.as_slice(), [only] if only == url.as_str());
    let args = if plain {
        vec!["-a".to_owned(), app.to_owned(), url.to_string()]
    } else {
        ["-n", "-a", app, "--args"]
            .into_iter()
            .map(str::to_owned)
            .chain(extra)
            .collect()
    };
    Launch {
        program: "/usr/bin/open".to_owned(),
        args,
    }
}

/// Build the command line for a user-supplied command.
///
/// `%u` marks where the URL goes — the same code desktop entries use. Without
/// one the URL is appended, which is what almost every program expects.
pub fn for_command(line: &str, url: &Url) -> Result<Launch> {
    // Field codes are meaningful here, so the desktop parser's stripping is not
    // wanted: split on the same quoting rules, then substitute.
    let style = if cfg!(windows) {
        Quoting::Windows
    } else {
        Quoting::Unix
    };
    let mut argv = split_command(line, style);
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

/// How a command line marks what belongs together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quoting {
    /// Single and double quotes, and a backslash before any character.
    Unix,
    /// Double quotes only. A backslash is the path separator, and an
    /// apostrophe is a letter in a name like `O'Brien`.
    Windows,
}

/// Split a command line into arguments, honouring quotes but expanding nothing.
///
/// No globbing, no variable expansion, no operators: `~`, `$HOME`, `%APPDATA%`,
/// `&&` and `|` are literal text here. Anything that needs them can point at a
/// script.
pub fn split_command(line: &str, style: Quoting) -> Vec<String> {
    let unix = style == Quoting::Unix;
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if quote.is_none() => {
                quote = Some(c);
                started = true;
            }
            '\'' if unix && quote.is_none() => {
                quote = Some(c);
                started = true;
            }
            c if Some(c) == quote => {
                quote = None;
            }
            '\\' if unix && quote != Some('\'') => {
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

/// Hand a URL or a folder to the desktop, the way a link opens anywhere else.
///
/// `xdg-open` started directly rather than through the opener plugin, so it
/// gets the environment [`spawn`] cleans: from an AppImage, the plugin would
/// pass the image's library path on to whatever browser or file manager
/// `xdg-open` picks.
pub fn for_desktop(target: &str) -> Launch {
    Launch {
        program: "xdg-open".to_owned(),
        args: vec![target.to_owned()],
    }
}

/// Start a program without waiting for it.
///
/// The child is detached from Lurkn's own streams: a browser writing to stderr
/// must not end up in Lurkn's terminal, and a browser reading stdin must not
/// steal it. The exit status is collected on a thread purely so the process
/// table does not fill with zombies over a long session.
pub fn spawn(launch: &Launch) -> Result<()> {
    use std::process::Stdio;

    // Without what an AppImage points into itself; see `crate::host`.
    let mut command = crate::host::command(&launch.program);
    command
        .args(&launch.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // A console program started from a windowed one gets a console window of
    // its own, which would flash up for nothing: its output is discarded.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command
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
            start: Start::Command(argv.iter().map(|a| (*a).to_owned()).collect()),
        }
    }

    fn bundle(family: Family) -> Browser {
        Browser {
            id: "com.example.browser".into(),
            name: "Test".into(),
            family,
            start: Start::Bundle("/Applications/Test Browser.app".into()),
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

    /// A plain URL goes to the copy already running; anything with flags needs
    /// a process of its own to read them.
    #[test]
    fn a_mac_app_gets_flags_only_through_a_new_process() {
        let chrome = bundle(Family::Chromium);

        let plain = for_browser(&chrome, &url(), LaunchMode::Normal, false).unwrap();
        assert_eq!(plain.program, "/usr/bin/open");
        assert_eq!(
            plain.args,
            [
                "-a",
                "/Applications/Test Browser.app",
                "https://www.twitch.tv/forsen"
            ]
        );

        let app = for_browser(&chrome, &url(), LaunchMode::App, false).unwrap();
        assert_eq!(
            app.args,
            [
                "-n",
                "-a",
                "/Applications/Test Browser.app",
                "--args",
                "--app=https://www.twitch.tv/forsen"
            ]
        );

        let window = for_browser(&chrome, &url(), LaunchMode::Normal, true).unwrap();
        assert_eq!(
            window.args,
            [
                "-n",
                "-a",
                "/Applications/Test Browser.app",
                "--args",
                "--new-window",
                "https://www.twitch.tv/forsen"
            ]
        );
    }

    /// Safari takes no flags, so it is never started a second time.
    #[test]
    fn an_unknown_mac_app_only_ever_gets_the_url() {
        for wanted in [LaunchMode::App, LaunchMode::Kiosk, LaunchMode::Normal] {
            let launch = for_browser(&bundle(Family::Other), &url(), wanted, true).unwrap();
            assert_eq!(launch.args[0], "-a", "{wanted:?}");
        }
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
        let launch = for_command(r#"player "a; rm -rf ~ && echo b" %u"#, &url()).unwrap();
        assert_eq!(launch.program, "player");
        assert_eq!(
            launch.args,
            ["a; rm -rf ~ && echo b", "https://www.twitch.tv/forsen"]
        );
    }

    #[test]
    fn windows_paths_keep_their_backslashes() {
        assert_eq!(
            split_command(
                r#""C:\Program Files\LibreWolf\librewolf.exe" --kiosk %u"#,
                Quoting::Windows
            ),
            [r"C:\Program Files\LibreWolf\librewolf.exe", "--kiosk", "%u"]
        );
        assert_eq!(
            split_command(r"\\server\share\play.exe", Quoting::Windows),
            [r"\\server\share\play.exe"]
        );
    }

    /// An apostrophe in a Windows path is a letter, not the start of a quote
    /// that would swallow the rest of the line.
    #[test]
    fn windows_has_no_single_quotes() {
        assert_eq!(
            split_command(r"C:\Users\O'Brien\mpv.exe %u", Quoting::Windows),
            [r"C:\Users\O'Brien\mpv.exe", "%u"]
        );
    }

    #[test]
    fn unix_backslashes_escape() {
        assert_eq!(
            split_command(r"/opt/My\ Player/play %u", Quoting::Unix),
            ["/opt/My Player/play", "%u"]
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

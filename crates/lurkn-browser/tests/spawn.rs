//! `spawn` really starts a program, and hands it exactly the arguments built.
//!
//! The argument building is unit-tested; what this covers is the step after it,
//! where a wrong assumption would only show up as a browser that does not open.
//!
//! Unix only: the stand-in program is a shell script. What it checks — argv
//! passed through untouched, no shell in between — is the same code on Windows.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lurkn_browser::launch::{self, Launch};

/// A script that writes its arguments, one per line, and exits.
fn recorder(name: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("lurkn-spawn-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let out = dir.join("argv");
    let script = dir.join("record.sh");

    let mut file = std::fs::File::create(&script).unwrap();
    writeln!(file, "#!/bin/sh").unwrap();
    writeln!(
        file,
        "for a in \"$@\"; do echo \"$a\"; done > '{}'",
        out.display()
    )
    .unwrap();
    drop(file);

    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, out)
}

fn wait_for(path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if !contents.is_empty() {
                return contents;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the spawned program never wrote its arguments");
}

#[test]
fn spawn_passes_each_argument_separately() {
    let (script, out) = recorder("args");

    launch::spawn(&Launch {
        program: script.display().to_string(),
        args: vec![
            "--kiosk".to_owned(),
            "https://www.twitch.tv/forsen".to_owned(),
            // A space and a semicolon survive as one argument, because there is
            // no shell in between to split them or act on them.
            "a b; echo pwned".to_owned(),
        ],
    })
    .unwrap();

    assert_eq!(
        wait_for(&out).lines().collect::<Vec<_>>(),
        ["--kiosk", "https://www.twitch.tv/forsen", "a b; echo pwned"]
    );

    let _ = std::fs::remove_dir_all(script.parent().unwrap());
}

#[test]
fn spawning_something_that_is_not_there_is_an_error() {
    let result = launch::spawn(&Launch {
        program: "/nonexistent/lurkn-test-browser".to_owned(),
        args: vec!["https://www.twitch.tv/forsen".to_owned()],
    });

    assert!(matches!(
        result,
        Err(lurkn_browser::BrowserError::LaunchFailed { .. })
    ));
}

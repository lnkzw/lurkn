//! The environment an outside program is started with.
//!
//! Inside an AppImage, the runtime and its GTK hook point the library path,
//! GTK's module paths and more into the mounted image, so that Lurkn finds the
//! libraries it brought. A browser started with that environment loads those
//! libraries instead of its own and dies before it shows a window — silently,
//! since its output is discarded. So whatever points into the image is taken
//! back out before anything is started.
//!
//! Outside an AppImage nothing changes.

use std::ffi::{OsStr, OsString};
use std::process::Command;

/// The AppImage runtime's markers. The started program is not in the image.
const RUNTIME: &[&str] = &["APPDIR", "APPIMAGE", "ARGV0", "OWD"];

/// Set by the GTK hook to fixed values rather than paths: a forced X11 backend
/// and a forced theme. Whatever the reader had before is gone by then, so the
/// program gets its own default.
const HOOK: &[&str] = &["GDK_BACKEND", "GTK_THEME", "PYTHONDONTWRITEBYTECODE"];

/// What to change in an inherited environment; `None` removes the variable.
///
/// Path lists keep the entries outside the image, in their order, and are
/// removed when nothing is left.
pub fn host_environment<I>(vars: I) -> Vec<(OsString, Option<OsString>)>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let vars: Vec<(OsString, OsString)> = vars.into_iter().collect();
    let appdir = vars
        .iter()
        .find(|(key, _)| key == "APPDIR")
        .and_then(|(_, value)| value.to_str())
        .map(|value| value.trim_end_matches('/'))
        .filter(|value| !value.is_empty());
    let Some(appdir) = appdir else {
        return Vec::new();
    };
    let inside = |part: &str| {
        part.strip_prefix(appdir)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };

    let mut changes = Vec::new();
    for (key, value) in &vars {
        let Some(name) = key.to_str() else { continue };
        if RUNTIME.contains(&name) || HOOK.contains(&name) {
            changes.push((key.clone(), None));
            continue;
        }
        let Some(text) = value.to_str() else { continue };
        if !text.split(':').any(inside) {
            continue;
        }
        let kept: Vec<&str> = text
            .split(':')
            .filter(|part| !part.is_empty() && !inside(part))
            .collect();
        let kept = (!kept.is_empty()).then(|| OsString::from(kept.join(":")));
        changes.push((key.clone(), kept));
    }
    changes
}

/// A command for a program outside Lurkn, with the environment it would get
/// if Lurkn were not running from an AppImage.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for (key, value) in host_environment(std::env::vars_os()) {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNT: &str = "/tmp/.mount_LurknAb12";

    fn env(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }

    fn change<'a>(
        changes: &'a [(OsString, Option<OsString>)],
        key: &str,
    ) -> Option<&'a Option<OsString>> {
        changes.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    #[test]
    fn outside_an_appimage_nothing_changes() {
        let vars = env(&[
            ("PATH", "/usr/bin"),
            ("GDK_BACKEND", "wayland"),
            ("LD_LIBRARY_PATH", "/opt/lib"),
        ]);
        assert!(host_environment(vars).is_empty());
    }

    #[test]
    fn path_lists_lose_only_the_image() {
        let vars = env(&[
            ("APPDIR", MOUNT),
            (
                "PATH",
                &format!("{MOUNT}/usr/bin/:{MOUNT}/bin/:/home/me/.cargo/bin:/usr/bin"),
            ),
            (
                "XDG_DATA_DIRS",
                &format!("{MOUNT}/usr/share/:/usr/local/share:/usr/share"),
            ),
        ]);
        let changes = host_environment(vars);
        assert_eq!(
            change(&changes, "PATH"),
            Some(&Some(OsString::from("/home/me/.cargo/bin:/usr/bin")))
        );
        assert_eq!(
            change(&changes, "XDG_DATA_DIRS"),
            Some(&Some(OsString::from("/usr/local/share:/usr/share")))
        );
    }

    #[test]
    fn variables_wholly_inside_the_image_go() {
        let vars = env(&[
            ("APPDIR", MOUNT),
            (
                "LD_LIBRARY_PATH",
                &format!("{MOUNT}/usr/lib/:{MOUNT}/usr/lib64/"),
            ),
            ("GTK_PATH", &format!("{MOUNT}//usr/lib/gtk-3.0")),
            (
                "GDK_PIXBUF_MODULE_FILE",
                &format!("{MOUNT}//usr/lib/loaders.cache"),
            ),
        ]);
        let changes = host_environment(vars);
        for key in ["LD_LIBRARY_PATH", "GTK_PATH", "GDK_PIXBUF_MODULE_FILE"] {
            assert_eq!(change(&changes, key), Some(&None), "{key}");
        }
    }

    #[test]
    fn the_hook_and_the_runtime_markers_go() {
        let vars = env(&[
            ("APPDIR", MOUNT),
            ("APPIMAGE", "/home/me/Downloads/Lurkn.AppImage"),
            ("OWD", "/home/me/Downloads"),
            ("GDK_BACKEND", "x11"),
            ("GTK_THEME", "Adwaita:dark"),
        ]);
        let changes = host_environment(vars);
        for key in ["APPDIR", "APPIMAGE", "OWD", "GDK_BACKEND", "GTK_THEME"] {
            assert_eq!(change(&changes, key), Some(&None), "{key}");
        }
    }

    #[test]
    fn the_rest_is_left_alone() {
        let vars = env(&[
            ("APPDIR", MOUNT),
            ("HOME", "/home/me"),
            // Shares a prefix with the mount without being inside it.
            ("SOMEWHERE", &format!("{MOUNT}cd/usr/lib")),
        ]);
        let changes = host_environment(vars);
        assert_eq!(change(&changes, "HOME"), None);
        assert_eq!(change(&changes, "SOMEWHERE"), None);
    }
}

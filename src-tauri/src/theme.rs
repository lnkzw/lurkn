//! Theme files on disk: where they live, which one is in effect, and noticing
//! when it changes. See `docs/architecture.md` §8.5; the parsing is
//! `lurkn_core::theme`.
//!
//! The chosen file is polled rather than watched. A stat every half second is
//! nothing, it needs no platform watcher, and it follows symlinks — which is
//! how stow, chezmoi and home-manager put a dotfile in place, and where inotify
//! watches go quiet.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use lurkn_core::theme::{self, Theme};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::events::ThemeChanged;
use crate::state::AppState;

/// How often the chosen theme file is checked for an edit. Short enough that a
/// wallpaper script's write shows up as it happens.
const POLL: Duration = Duration::from_millis(500);

/// The theme file in effect, and anything the reader should know about it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThemeState {
    /// `None` while the built-in palette is in use: none chosen, or the chosen
    /// file could not be read before it ever loaded.
    pub theme: Option<Theme>,
    /// Why the chosen file is not fully applied. A file that stops parsing
    /// mid-edit keeps its last good palette, and says so here.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ThemeEntry {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ThemeList {
    /// Where theme files go, for the settings screen to show. `None` when no
    /// config folder could be worked out at all.
    pub folder: Option<String>,
    pub themes: Vec<ThemeEntry>,
}

/// Lurkn's config folder.
///
/// `LURKN_CONFIG_DIR` wins, then `$XDG_CONFIG_HOME/lurkn` on any platform, then
/// the platform's own place: `%APPDATA%\lurkn` on Windows, `~/.config/lurkn`
/// elsewhere. macOS gets `~/.config` rather than `~/Library/Application
/// Support` on purpose: that is where the rest of a dotfiles repo already
/// points — Alacritty, WezTerm, Ghostty and friends all read it there.
pub fn config_dir() -> Option<PathBuf> {
    resolve_config_dir(|name| std::env::var_os(name), cfg!(windows))
}

fn resolve_config_dir(env: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let set = |name: &str| env(name).filter(|value| !value.is_empty());

    if let Some(dir) = set("LURKN_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(base) = set("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(base).join("lurkn"));
    }
    if windows {
        return set("APPDATA").map(|base| PathBuf::from(base).join("lurkn"));
    }
    set("HOME").map(|home| PathBuf::from(home).join(".config").join("lurkn"))
}

pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("themes"))
}

fn theme_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.toml"))
}

/// Every theme file in the folder, by name. A file that does not parse is
/// still listed, under its id, so picking it shows what is wrong with it.
pub fn list() -> ThemeList {
    let Some(dir) = themes_dir() else {
        return ThemeList {
            folder: None,
            themes: Vec::new(),
        };
    };

    let mut themes: Vec<ThemeEntry> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "toml" {
                return None;
            }
            let id = theme::sanitize_theme_id(path.file_stem()?.to_str()?)?;
            let name = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| theme::parse(&id, &text).ok())
                .map_or_else(|| id.clone(), |parsed| parsed.theme.name);
            Some(ThemeEntry { id, name })
        })
        .collect();

    themes.sort_by_key(|entry| entry.name.to_lowercase());

    ThemeList {
        folder: Some(dir.display().to_string()),
        themes,
    }
}

/// Make the themes folder, put an example in it if it has no themes yet, and
/// open it in the file manager.
///
/// The example is written here rather than at startup: Lurkn does not create
/// files nobody asked for, but someone opening the themes folder is about to
/// write one, and a file listing every key beats a page of docs.
pub fn open_folder(state: &AppState) -> Result<(), String> {
    let dir = themes_dir().ok_or("Could not work out where your config folder is.")?;

    std::fs::create_dir_all(&dir).map_err(|error| {
        tracing::warn!(%error, path = %dir.display(), "could not create the themes folder");
        format!("Could not create {}.", dir.display())
    })?;

    if list().themes.is_empty() {
        let example = theme_path(&dir, "example");
        if let Err(error) = std::fs::write(&example, theme::EXAMPLE) {
            tracing::warn!(%error, path = %example.display(), "could not write the example theme");
        }
    }

    state.open_path(&dir).map_err(|error| {
        tracing::warn!(%error, "could not open the themes folder");
        "Could not open your file manager.".to_owned()
    })
}

/// What the poll compares to decide whether to reload. A changed size or
/// modification time is a changed file; an unchanged pair is not worth a read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stamp {
    BuiltIn,
    Missing(String),
    File(String, Option<SystemTime>, u64),
}

async fn stamp(id: &str) -> Stamp {
    if id.is_empty() {
        return Stamp::BuiltIn;
    }
    let Some(dir) = themes_dir() else {
        return Stamp::Missing(id.to_owned());
    };
    match tokio::fs::metadata(theme_path(&dir, id)).await {
        Ok(meta) => Stamp::File(id.to_owned(), meta.modified().ok(), meta.len()),
        Err(_) => Stamp::Missing(id.to_owned()),
    }
}

async fn load(id: &str, previous: &ThemeState) -> ThemeState {
    if id.is_empty() {
        return ThemeState::default();
    }
    let file = format!("{id}.toml");
    let Some(dir) = themes_dir() else {
        return ThemeState {
            theme: None,
            problems: vec!["Could not work out where your config folder is.".to_owned()],
        };
    };

    let text = match tokio::fs::read_to_string(theme_path(&dir, id)).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ThemeState {
                theme: None,
                problems: vec![format!("{file} is not in the themes folder.")],
            };
        }
        Err(error) => {
            return ThemeState {
                theme: None,
                problems: vec![format!("Could not read {file}: {error}")],
            };
        }
    };

    match theme::parse(id, &text) {
        Ok(parsed) => ThemeState {
            theme: Some(parsed.theme),
            problems: parsed
                .warnings
                .into_iter()
                .map(|warning| format!("{file}: {warning}"))
                .collect(),
        },
        // Mid-edit, a file is often briefly invalid. Keeping the palette it
        // last had stops the window flashing back to the default on every
        // half-typed line.
        Err(error) => ThemeState {
            theme: previous.theme.clone().filter(|theme| theme.id == id),
            problems: vec![format!("{file}: {}", error.trim_end())],
        },
    }
}

/// Keep the theme in effect matching settings and the file on disk, for the
/// life of the app.
///
/// Woken early by [`wake`] when settings change, so picking a theme applies at
/// once rather than on the next poll.
pub async fn run(app: AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let mut seen: Option<Stamp> = None;
    let mut tick = tokio::time::interval(POLL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            () = state.theme_wake.notified() => {}
        }

        let id = state.settings.read().await.custom_theme.clone();
        let now = stamp(&id).await;
        if seen.as_ref() == Some(&now) {
            continue;
        }
        seen = Some(now);

        let previous = state.theme.read().await.clone();
        let next = load(&id, &previous).await;
        if next == previous {
            continue;
        }

        if let Some(theme) = &next.theme {
            tracing::debug!(id = %theme.id, problems = next.problems.len(), "applied a theme file");
        }
        for problem in &next.problems {
            tracing::info!(%problem, "theme file");
        }

        *state.theme.write().await = next.clone();
        if let Err(error) = ThemeChanged(next).emit(&app) {
            tracing::error!(%error, "could not emit the theme change");
        }
    }
}

/// Have [`run`] look again now rather than on its next tick.
pub fn wake(state: &AppState) {
    state.theme_wake.notify_one();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(vars: &[(&str, &str)], windows: bool) -> Option<PathBuf> {
        resolve_config_dir(
            |name| {
                vars.iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            },
            windows,
        )
    }

    #[test]
    fn the_override_wins_everywhere() {
        let vars = [
            ("LURKN_CONFIG_DIR", "/dots/lurkn"),
            ("XDG_CONFIG_HOME", "/xdg"),
        ];
        assert_eq!(resolve(&vars, false), Some(PathBuf::from("/dots/lurkn")));
        assert_eq!(resolve(&vars, true), Some(PathBuf::from("/dots/lurkn")));
    }

    #[test]
    fn xdg_is_honoured_on_every_platform() {
        let vars = [("XDG_CONFIG_HOME", "/xdg"), ("APPDATA", "C:/roaming")];
        assert_eq!(resolve(&vars, true), Some(PathBuf::from("/xdg/lurkn")));
    }

    #[test]
    fn each_platform_falls_back_to_its_own_place() {
        let vars = [("HOME", "/home/me"), ("APPDATA", "C:/roaming")];
        assert_eq!(
            resolve(&vars, false),
            Some(PathBuf::from("/home/me/.config/lurkn"))
        );
        assert_eq!(
            resolve(&vars, true),
            Some(PathBuf::from("C:/roaming/lurkn"))
        );
    }

    #[test]
    fn an_empty_variable_counts_as_unset() {
        let vars = [("XDG_CONFIG_HOME", ""), ("HOME", "/home/me")];
        assert_eq!(
            resolve(&vars, false),
            Some(PathBuf::from("/home/me/.config/lurkn"))
        );
    }

    #[test]
    fn nothing_set_is_no_folder() {
        assert_eq!(resolve(&[], false), None);
    }
}

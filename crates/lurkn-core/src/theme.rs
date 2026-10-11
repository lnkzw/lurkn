//! Theme files: a palette, fonts and a corner radius, read from a TOML file the
//! user keeps in their config folder. See `docs/architecture.md` §8.5.
//!
//! Pure: the shell finds and reads the file, this module turns its text into
//! CSS custom properties. Every value is checked here before it can reach a
//! stylesheet, the same way a custom font name is.
//!
//! Every key is optional. Whatever a file leaves out comes from the built-in
//! palette its `variant` names, so a theme can be one line that changes the
//! accent, or a full palette generated from a wallpaper.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::settings::{sanitize_font_name, MAX_FONT_NAME};

/// The longest theme id accepted: a file stem, so the same bound as a font.
pub const MAX_THEME_ID: usize = 64;
pub const MAX_RADIUS: u8 = 16;

/// Which built-in palette a theme sits on, and what native widgets draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub enum ThemeVariant {
    #[default]
    Dark,
    Light,
}

/// One CSS custom property a theme sets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ThemeVar {
    /// Always one of the names in [`COLORS`] or the `--font`/`--mono`/`--radius`
    /// set below; never taken from the file.
    pub name: String,
    pub value: String,
}

/// A theme file, checked and ready to apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Theme {
    /// The file stem, which is what settings store.
    pub id: String,
    /// What the picker shows: the file's `name`, or the id.
    pub name: String,
    pub variant: ThemeVariant,
    pub vars: Vec<ThemeVar>,
}

/// A parsed theme, plus what in the file was skipped and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub theme: Theme,
    pub warnings: Vec<String>,
}

/// The colour keys a theme file may set, and the property each one drives.
///
/// Named for what they are rather than after the properties, so a theme reads
/// without knowing the stylesheet. The order is the order the example file
/// lists them in.
pub const COLORS: &[(&str, &str)] = &[
    ("background", "--bg"),
    ("surface", "--bg-raised"),
    ("border", "--border"),
    ("text", "--text"),
    ("text-dim", "--text-dim"),
    ("accent", "--accent"),
    ("on-accent", "--on-accent"),
    ("highlight", "--highlight"),
    ("success", "--ok"),
    ("warning", "--warn"),
    ("danger", "--danger"),
    ("live", "--live"),
    ("unread", "--unread"),
];

/// The file as written. Unknown keys are kept so they can be reported: a typo
/// in a hand-written theme should say so rather than silently do nothing.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct File {
    name: Option<String>,
    variant: Option<String>,
    #[serde(default)]
    colors: toml::Table,
    #[serde(default)]
    ui: toml::Table,
    #[serde(flatten)]
    rest: toml::Table,
}

/// Parse a theme file's text.
///
/// Only a file that is not TOML at all is an error. Anything else wrong in it —
/// a colour that is not one, a key Lurkn does not know — is skipped with a
/// warning, so one bad line does not cost the whole theme.
pub fn parse(id: &str, text: &str) -> Result<Parsed, String> {
    let file: File = toml::from_str(text).map_err(|error| error.to_string())?;
    let mut warnings = Vec::new();

    let variant = match file.variant.as_deref() {
        None | Some("dark") => ThemeVariant::Dark,
        Some("light") => ThemeVariant::Light,
        Some(other) => {
            warnings.push(format!(
                "variant: \"{other}\" is not \"dark\" or \"light\"; using dark"
            ));
            ThemeVariant::Dark
        }
    };

    let mut vars = Vec::new();

    for (key, value) in &file.colors {
        let Some((_, property)) = COLORS.iter().find(|(name, _)| name == key) else {
            warnings.push(format!("colors.{key}: not a colour Lurkn knows"));
            continue;
        };
        match value.as_str().and_then(parse_color) {
            Some(color) => vars.push(var(property, color)),
            None => warnings.push(format!(
                "colors.{key}: {value} is not a colour; use \"#rrggbb\""
            )),
        }
    }

    for (key, value) in &file.ui {
        match key.as_str() {
            "font" | "mono-font" => {
                let (property, fallback) = if key == "font" {
                    ("--font", UI_FALLBACK)
                } else {
                    ("--mono", MONO_FALLBACK)
                };
                match value.as_str().and_then(sanitize_font_name) {
                    Some(family) => vars.push(var(property, format!("\"{family}\", {fallback}"))),
                    None => warnings.push(format!("ui.{key}: {value} is not a font name")),
                }
            }
            "radius" => match value.as_integer() {
                Some(px) => {
                    let clamped = px.clamp(0, i64::from(MAX_RADIUS));
                    if clamped != px {
                        warnings.push(format!("ui.radius: {px} is out of range; using {clamped}"));
                    }
                    vars.push(var("--radius", format!("{clamped}px")));
                }
                None => warnings.push(format!("ui.radius: {value} is not a whole number")),
            },
            _ => warnings.push(format!("ui.{key}: not a setting Lurkn knows")),
        }
    }

    for key in file.rest.keys() {
        warnings.push(format!("{key}: not a setting Lurkn knows"));
    }

    let name = file
        .name
        .map(|name| name.trim().chars().take(MAX_FONT_NAME).collect::<String>())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.to_owned());

    Ok(Parsed {
        theme: Theme {
            id: id.to_owned(),
            name,
            variant,
            vars,
        },
        warnings,
    })
}

/// What a themed font falls back to when the family is not installed: the same
/// stacks `app.css` uses.
const UI_FALLBACK: &str = "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif";
const MONO_FALLBACK: &str = "ui-monospace, monospace";

fn var(name: &str, value: String) -> ThemeVar {
    ThemeVar {
        name: name.to_owned(),
        value,
    }
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`, with or without the `#`, normalised to
/// lowercase with it.
///
/// The leading `#` is optional because the colour templates of pywal, wallust
/// and matugen offer both forms, and a generated theme should not need a
/// second template just to add it.
pub fn parse_color(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    let valid = matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    valid.then(|| format!("#{}", hex.to_ascii_lowercase()))
}

/// Keep a theme id only if it is safe to use as a file stem.
///
/// The id comes back from the frontend and is joined onto the themes folder,
/// so anything that could leave that folder — a separator, a leading dot — is
/// refused outright rather than cleaned.
pub fn sanitize_theme_id(raw: &str) -> Option<String> {
    let id = raw.trim();
    let valid = !id.is_empty()
        && id.chars().count() <= MAX_THEME_ID
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'));
    valid.then(|| id.to_owned())
}

/// A starting point for someone writing their own theme: every key, set to the
/// built-in dark palette, with what each one colours.
pub const EXAMPLE: &str = r##"# A Lurkn theme. Pick it under Settings → Appearance → Theme.
#
# Lurkn rereads this file whenever it changes, so edits show up live — which
# also means pywal, wallust or matugen can write it straight from a template.
#
# Every key is optional. Anything left out comes from the built-in palette
# that `variant` names, so a theme can be as short as one accent colour.

name = "Example"

# "dark" or "light". Decides the fallback palette, and how native widgets
# such as dropdowns and checkboxes are drawn.
variant = "dark"

[colors]
background = "#0e0e10"   # the window, the chat log
surface    = "#18181b"   # the sidebar, popups, the message box
border     = "#2a2a2e"   # dividers and outlines
text       = "#efeff1"
text-dim   = "#adadb8"   # timestamps, hints, secondary labels
accent     = "#9146ff"   # buttons, focus rings, links, selection
on-accent  = "#ffffff"   # text on an accent-filled button
highlight  = "#9146ff"   # mentions and highlighted messages
success    = "#00c08b"   # connected, raids, saved
warning    = "#ffb020"   # errors, bits
danger     = "#e81123"   # the close button
live       = "#eb0400"   # the live dot
unread     = "#e5484d"   # the "you were here" line

[ui]
# font      = "Inter"            # the interface; chat has its own font setting
# mono-font = "JetBrains Mono"
radius = 6                       # corner rounding in pixels, 0 to 16
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn value(theme: &Theme, name: &str) -> Option<String> {
        theme
            .vars
            .iter()
            .find(|v| v.name == name)
            .map(|v| v.value.clone())
    }

    #[test]
    fn an_empty_file_is_the_dark_palette_unchanged() {
        let parsed = parse("blank", "").unwrap();
        assert_eq!(parsed.theme.variant, ThemeVariant::Dark);
        assert_eq!(parsed.theme.name, "blank");
        assert!(parsed.theme.vars.is_empty());
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn colours_map_to_their_properties() {
        let parsed = parse(
            "gruvbox",
            r##"
            name = "Gruvbox"
            variant = "light"
            [colors]
            background = "#FBF1C7"
            accent = "d65d0e"
            "##,
        )
        .unwrap();

        assert_eq!(parsed.theme.name, "Gruvbox");
        assert_eq!(parsed.theme.variant, ThemeVariant::Light);
        assert_eq!(value(&parsed.theme, "--bg").as_deref(), Some("#fbf1c7"));
        assert_eq!(value(&parsed.theme, "--accent").as_deref(), Some("#d65d0e"));
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn a_bad_value_is_skipped_and_the_rest_still_applies() {
        let parsed = parse(
            "t",
            r##"
            [colors]
            text = "red; } body { display: none"
            accent = "#123"
            backgrond = "#000000"
            "##,
        )
        .unwrap();

        assert_eq!(value(&parsed.theme, "--text"), None);
        assert_eq!(value(&parsed.theme, "--accent").as_deref(), Some("#123"));
        assert_eq!(parsed.warnings.len(), 2);
        assert!(parsed.warnings.iter().any(|w| w.starts_with("colors.text")));
        assert!(parsed
            .warnings
            .iter()
            .any(|w| w.starts_with("colors.backgrond")));
    }

    #[test]
    fn unknown_tables_and_keys_are_reported() {
        let parsed = parse("t", "colours = 1\n[ui]\nshadow = true\n").unwrap();
        assert_eq!(parsed.warnings.len(), 2);
    }

    #[test]
    fn fonts_are_sanitised_and_given_a_fallback() {
        let parsed = parse("t", "[ui]\nfont = \"Inter\"\nmono-font = \"\\\";}\"\n").unwrap();
        assert_eq!(
            value(&parsed.theme, "--font").as_deref(),
            Some("\"Inter\", system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif")
        );
        assert_eq!(value(&parsed.theme, "--mono"), None);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn the_radius_is_clamped() {
        let parsed = parse("t", "[ui]\nradius = 99\n").unwrap();
        assert_eq!(value(&parsed.theme, "--radius").as_deref(), Some("16px"));
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn an_unknown_variant_falls_back_to_dark() {
        let parsed = parse("t", "variant = \"sepia\"\n").unwrap();
        assert_eq!(parsed.theme.variant, ThemeVariant::Dark);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn a_file_that_is_not_toml_is_an_error() {
        assert!(parse("t", "[colors\n").is_err());
    }

    #[test]
    fn the_example_parses_cleanly() {
        let parsed = parse("example", EXAMPLE).unwrap();
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.theme.vars.len(), COLORS.len() + 1);
    }

    #[test]
    fn colours_accept_the_three_hex_lengths() {
        assert_eq!(parse_color("#ABC").as_deref(), Some("#abc"));
        assert_eq!(parse_color("aabbcc").as_deref(), Some("#aabbcc"));
        assert_eq!(parse_color("#aabbcc80").as_deref(), Some("#aabbcc80"));
        assert_eq!(parse_color("#abcd"), None);
        assert_eq!(parse_color("rgb(0,0,0)"), None);
        assert_eq!(parse_color(""), None);
    }

    #[test]
    fn theme_ids_cannot_leave_the_folder() {
        assert_eq!(
            sanitize_theme_id("gruvbox-dark").as_deref(),
            Some("gruvbox-dark")
        );
        assert_eq!(
            sanitize_theme_id("tokyo night.v2").as_deref(),
            Some("tokyo night.v2")
        );
        assert_eq!(sanitize_theme_id("../secrets"), None);
        assert_eq!(sanitize_theme_id("a/b"), None);
        assert_eq!(sanitize_theme_id("a\\b"), None);
        assert_eq!(sanitize_theme_id(".hidden"), None);
        assert_eq!(sanitize_theme_id("  "), None);
        assert_eq!(sanitize_theme_id(&"x".repeat(MAX_THEME_ID + 1)), None);
    }
}

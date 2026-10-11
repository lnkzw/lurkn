//! The chat beside the player in the synced window.
//!
//! Lurkn has decided everything about a message by the time it gets here —
//! ignores dropped, highlights marked, emotes and badges resolved — so this
//! only reduces it to what the page draws. The look comes from the frontend,
//! read off the palette Lurkn is actually painted with, so the window matches
//! whichever theme file, system preference and font are in effect without a
//! second copy of the palette living in Rust.

use lurkn_core::settings::{MAX_FONT_SIZE, MIN_FONT_SIZE};
use lurkn_core::{ChannelEvent, ChatMessage, ChatToken, MessageKind, MessageState};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::overlay::wire::{OverlayBadge, Part};

/// One chat line, reduced to what the page draws.
///
/// Every string in here is UNTRUSTED, and the page only ever assigns it to
/// `textContent` or an `<img>` attribute. Image URLs are the ones Rust already
/// checked against the provider allowlist, and the page's own policy refuses
/// any other host besides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessage {
    pub id: String,
    pub login: String,
    pub name: String,
    /// `#rrggbb`, or `None` for a chatter who never picked one.
    pub color: Option<String>,
    pub badges: Vec<OverlayBadge>,
    pub kind: MessageKind,
    pub parts: Vec<Part>,
    /// The message as typed, for the reply line of a later message that
    /// answers it.
    pub text: String,
    /// Unix milliseconds; the page formats it in local time, as Lurkn does.
    pub at: i64,
    pub deleted: bool,
    /// Names the reader, or matches one of their highlight rules.
    pub marked: bool,
    /// The tint the matching rule asked for; `None` is the theme's highlight.
    pub mark: Option<String>,
    pub reply_to: Option<String>,
    pub event: Option<ChannelEvent>,
}

impl WatchMessage {
    /// `None` for what Lurkn says itself rather than a chatter: it belongs to
    /// the app, not to the stream.
    pub fn from_chat(message: &ChatMessage) -> Option<Self> {
        if message.kind == MessageKind::System {
            return None;
        }

        let parts = message
            .tokens
            .iter()
            .map(|token| match token {
                ChatToken::Text { text } => Part::Text { text: text.clone() },
                // The size Lurkn draws, so the two load the same image.
                ChatToken::Emote { emote } => Part::Emote {
                    name: emote.name.clone(),
                    url: emote.url_2x.as_ref().unwrap_or(&emote.url_1x).to_string(),
                    aspect: emote.aspect,
                    zero_width: emote.zero_width,
                },
                // A window across the room is read, not clicked.
                ChatToken::Link { display, .. } => Part::Text {
                    text: display.clone(),
                },
                ChatToken::Mention { display, .. } => Part::Mention {
                    text: display.clone(),
                },
            })
            .collect();

        let badges = message
            .author
            .badges
            .iter()
            .filter_map(|badge| {
                let url = badge.url_2x.as_ref().or(badge.url_1x.as_ref())?;
                Some(OverlayBadge {
                    url: url.to_string(),
                    title: badge.title.clone(),
                })
            })
            .collect();

        Some(Self {
            id: message.id.as_str().to_owned(),
            login: message.author.login.as_str().to_owned(),
            name: message.author.display_name.clone(),
            color: message.author.color.map(|c| c.to_css()),
            badges,
            kind: message.kind,
            parts,
            text: message.raw_text.clone(),
            at: (message.sent_at.unix_timestamp_nanos() / 1_000_000) as i64,
            deleted: message.state == MessageState::Deleted,
            marked: message.highlighted,
            mark: message.highlight_color.map(|c| c.to_css()),
            reply_to: message.reply_to.as_ref().map(|id| id.as_str().to_owned()),
            event: message.event.clone(),
        })
    }
}

/// How Lurkn's chat looks right now, as the page applies it.
///
/// Sent by the frontend, so every field is checked again here before it is
/// stored: the page sets these as custom properties, which cannot inject
/// anything, but a value that is not a colour or a font should still never
/// get that far.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WatchLook {
    /// `dark` or `light`, for the scrollbar and other native parts.
    pub scheme: String,
    pub background: String,
    pub surface: String,
    pub border: String,
    pub text: String,
    pub text_dim: String,
    pub accent: String,
    pub highlight: String,
    pub ok: String,
    pub warn: String,
    /// A resolved `font-family` stack, with no `var()` left in it.
    pub font: String,
    pub font_size: u8,
    pub timestamps: bool,
    pub badges: bool,
    /// Emote height as a multiple of the font size.
    pub emote_scale: f32,
    pub alternate: bool,
    pub channel_events: bool,
    pub replies: bool,
}

/// Longer than any colour or font stack Lurkn produces.
const MAX_VALUE: usize = 256;

impl WatchLook {
    pub fn sanitized(mut self) -> Self {
        self.scheme = if self.scheme == "light" {
            "light".to_owned()
        } else {
            "dark".to_owned()
        };
        for value in [
            &mut self.background,
            &mut self.surface,
            &mut self.border,
            &mut self.text,
            &mut self.text_dim,
            &mut self.accent,
            &mut self.highlight,
            &mut self.ok,
            &mut self.warn,
        ] {
            *value = color(value);
        }
        self.font = keep(&self.font, |c| {
            c.is_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '"' | '\'')
        });
        self.font_size = self.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.emote_scale = if self.emote_scale.is_finite() {
            self.emote_scale.clamp(1.0, 3.0)
        } else {
            1.75
        };
        self
    }
}

/// A CSS colour: `#hex`, a name, or a function such as `rgb()` or
/// `color-mix()`. Anything that could end the value — `;`, braces, quotes — is
/// dropped, and what is left is inert in a custom property.
fn color(raw: &str) -> String {
    keep(raw, |c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '#' | '(' | ')' | ',' | '.' | '%' | ' ' | '/' | '-')
    })
}

fn keep(raw: &str, allowed: impl Fn(char) -> bool) -> String {
    raw.trim()
        .chars()
        .filter(|&c| allowed(c))
        .take(MAX_VALUE)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> WatchLook {
        WatchLook {
            scheme: "dark".into(),
            background: "#0e0e10".into(),
            surface: "#18181b".into(),
            border: "#2a2a2e".into(),
            text: "#efeff1".into(),
            text_dim: "#adadb8".into(),
            accent: "#9146ff".into(),
            highlight: "#9146ff".into(),
            ok: "#00c08b".into(),
            warn: "#ffb020".into(),
            font: "system-ui, 'Segoe UI', sans-serif".into(),
            font_size: 13,
            timestamps: true,
            badges: true,
            emote_scale: 1.75,
            alternate: false,
            channel_events: true,
            replies: true,
        }
    }

    #[test]
    fn a_real_look_survives_unchanged() {
        assert_eq!(look().sanitized(), look());
    }

    #[test]
    fn a_colour_function_survives() {
        let mut l = look();
        l.highlight = "color-mix(in srgb, #9146ff 50%, transparent)".into();
        l.text = "rgb(239 239 241 / 90%)".into();
        let s = l.clone().sanitized();
        assert_eq!(s.highlight, l.highlight);
        assert_eq!(s.text, l.text);
    }

    /// Nothing sent from the WebView can leave the value it is meant for.
    #[test]
    fn a_hostile_value_cannot_escape() {
        let mut l = look();
        l.accent = "red; } body { display: none".into();
        l.font = "Inter\"; } * { color: red; font: x".into();
        let s = l.sanitized();
        assert!(!s.accent.contains([';', '{', '}']), "{}", s.accent);
        assert!(!s.font.contains([';', '{', '}', ':']), "{}", s.font);
    }

    #[test]
    fn numbers_are_bounded() {
        let mut l = look();
        l.font_size = 200;
        l.emote_scale = f32::NAN;
        l.scheme = "</style>".into();
        let s = l.sanitized();
        assert_eq!(s.font_size, MAX_FONT_SIZE);
        assert_eq!(s.emote_scale, 1.75);
        assert_eq!(s.scheme, "dark");
    }
}

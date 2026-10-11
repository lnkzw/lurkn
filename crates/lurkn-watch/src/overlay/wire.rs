//! What the overlay page is sent, and which messages it is sent at all.
//!
//! Pure functions over core types, so the rules for "what reaches the stream"
//! are tested without a socket or a browser.

use lurkn_core::{
    ChannelEventKind, ChatFont, ChatMessage, ChatToken, MessageKind, MessageState, OverlayLayout,
    OverlaySettings,
};
use serde::Serialize;

/// Whether a message belongs on the overlay.
///
/// Chat only. Subs, raids and announcements arrive on the same socket, but
/// they are the activity feed's, and a streamer who wants them on screen
/// already has an alert box for that. A cheer stays: it is a chatter's own
/// message that happens to carry bits.
pub fn admits(message: &ChatMessage, settings: &OverlaySettings) -> bool {
    if message.kind == MessageKind::System || message.state == MessageState::Deleted {
        return false;
    }
    if message
        .event
        .as_ref()
        .is_some_and(|event| event.kind != ChannelEventKind::Cheer)
    {
        return false;
    }

    let text = message.raw_text.trim_start();
    if text.is_empty() || (settings.hide_commands && text.starts_with('!')) {
        return false;
    }

    let login = message.author.login.as_str();
    !settings
        .hidden_users
        .iter()
        .any(|hidden| hidden.eq_ignore_ascii_case(login))
}

/// One chat line, reduced to what the page draws.
///
/// Every string in here is UNTRUSTED and the page only ever assigns it to
/// `textContent` or an `<img>` attribute. Image URLs are the ones Rust already
/// checked against the provider allowlist, and the page's own policy refuses
/// any other host besides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayMessage {
    pub id: String,
    pub login: String,
    pub name: String,
    /// `#rrggbb`, or `None` for a chatter who never picked one.
    pub color: Option<String>,
    pub badges: Vec<OverlayBadge>,
    /// `/me`, which Twitch draws in the chatter's colour.
    pub action: bool,
    pub parts: Vec<Part>,
    /// Unix milliseconds. A page that reconnects is replayed recent messages,
    /// and fading them by their real age keeps one that was already gone from
    /// coming back for another full turn.
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayBadge {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Part {
    Text {
        text: String,
    },
    Emote {
        name: String,
        url: String,
        aspect: Option<(u16, u16)>,
        zero_width: bool,
    },
    Mention {
        text: String,
    },
}

impl OverlayMessage {
    pub fn from_chat(message: &ChatMessage) -> Self {
        let parts = message
            .tokens
            .iter()
            .map(|token| match token {
                ChatToken::Text { text } => Part::Text { text: text.clone() },
                // The largest image there is: an overlay is drawn big and
                // scaled by OBS, and the CDN caches it either way.
                ChatToken::Emote { emote } => Part::Emote {
                    name: emote.name.clone(),
                    url: emote
                        .url_3x
                        .as_ref()
                        .or(emote.url_2x.as_ref())
                        .unwrap_or(&emote.url_1x)
                        .to_string(),
                    aspect: emote.aspect,
                    zero_width: emote.zero_width,
                },
                // Nobody can click a link on a stream; its text is what it is.
                ChatToken::Link { display, .. } => Part::Text {
                    text: display.clone(),
                },
                ChatToken::Mention { display, .. } => Part::Mention {
                    text: format!("@{display}"),
                },
            })
            .collect();

        let badges = message
            .author
            .badges
            .iter()
            .filter_map(|badge| {
                let url = badge
                    .url_4x
                    .as_ref()
                    .or(badge.url_2x.as_ref())
                    .or(badge.url_1x.as_ref())?;
                Some(OverlayBadge {
                    url: url.to_string(),
                    title: badge.title.clone(),
                })
            })
            .collect();

        Self {
            id: message.id.as_str().to_owned(),
            login: message.author.login.as_str().to_owned(),
            name: message.author.display_name.clone(),
            color: message.author.color.map(|c| c.to_css()),
            badges,
            action: message.kind == MessageKind::Action,
            parts,
            at: (message.sent_at.unix_timestamp_nanos() / 1_000_000) as i64,
        }
    }
}

/// The settings, as the page applies them.
///
/// Everything here is a number, a `#rrggbb`, or a font stack built from a name
/// that `sanitize_font_name` already stripped of anything that could leave a
/// CSS value. The page sets them as custom properties, never as a stylesheet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayLook {
    pub layout: OverlayLayout,
    pub font: String,
    pub font_size: u8,
    pub text_color: String,
    pub bubble_color: String,
    /// 0.0 to 1.0.
    pub bubble_opacity: f32,
    pub radius: u8,
    /// `None` means each chatter's own colour.
    pub name_color: Option<String>,
    pub outline: bool,
    pub badges: bool,
    pub emote_scale: f32,
    pub max_messages: u8,
    /// Seconds; zero is never.
    pub fade_after: u16,
    pub animate: bool,
}

impl OverlayLook {
    pub fn from_settings(settings: &OverlaySettings) -> Self {
        Self {
            layout: settings.layout,
            font: font_stack(settings.font, &settings.custom_font),
            font_size: settings.font_size,
            text_color: settings.text_color.to_css(),
            bubble_color: settings.bubble_color.to_css(),
            bubble_opacity: f32::from(settings.bubble_opacity.min(100)) / 100.0,
            radius: settings.corner_radius,
            name_color: (!settings.twitch_name_colors).then(|| settings.name_color.to_css()),
            outline: settings.outline,
            badges: settings.show_badges,
            emote_scale: settings.emote_size.scale(),
            max_messages: settings.max_messages,
            fade_after: settings.fade_after_seconds,
            animate: settings.animate,
        }
    }
}

/// A CSS font stack for a font choice. Mirrors `chatFontStack` in the
/// frontend, minus the app's own variables, which the page does not have.
pub fn font_stack(font: ChatFont, custom: &str) -> String {
    match font {
        ChatFont::System => "system-ui, sans-serif".to_owned(),
        ChatFont::Sans => "ui-sans-serif, system-ui, sans-serif".to_owned(),
        ChatFont::Serif => "ui-serif, Georgia, serif".to_owned(),
        ChatFont::Monospace => "ui-monospace, monospace".to_owned(),
        // Already sanitised, so quoting cannot be escaped from; the fallback
        // covers a font that is not installed on the machine running OBS.
        ChatFont::Custom => match lurkn_core::settings::sanitize_font_name(custom) {
            Some(name) => format!("\"{name}\", system-ui, sans-serif"),
            None => "system-ui, sans-serif".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lurkn_core::{
        Badge, ChannelEvent, ChatAuthor, Emote, EmoteScope, Login, MessageId, ProviderId, Rgb,
        UserId,
    };

    fn message(text: &str) -> ChatMessage {
        ChatMessage {
            id: MessageId("m1".into()),
            channel_id: UserId("1".into()),
            author: ChatAuthor {
                id: UserId("2".into()),
                login: Login::parse("viewer").unwrap(),
                display_name: "Viewer".into(),
                color: Some(Rgb { r: 255, g: 0, b: 0 }),
                badges: Vec::new(),
            },
            tokens: vec![ChatToken::Text { text: text.into() }],
            raw_text: text.into(),
            sent_at: time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            kind: MessageKind::Chat,
            state: MessageState::Sent,
            reply_to: None,
            event: None,
            highlighted: false,
            highlight_color: None,
        }
    }

    fn event(kind: ChannelEventKind) -> Option<ChannelEvent> {
        Some(ChannelEvent {
            kind,
            text: String::new(),
            count: None,
        })
    }

    #[test]
    fn ordinary_chat_is_admitted() {
        assert!(admits(&message("hello"), &OverlaySettings::default()));
    }

    /// The point of the feature: chat, not activity.
    #[test]
    fn channel_events_are_left_out() {
        let settings = OverlaySettings::default();
        for kind in [
            ChannelEventKind::Sub,
            ChannelEventKind::Resub,
            ChannelEventKind::SubGift,
            ChannelEventKind::CommunityGift,
            ChannelEventKind::Raid,
            ChannelEventKind::Announcement,
            ChannelEventKind::Other,
        ] {
            let mut m = message("text");
            m.event = event(kind);
            assert!(!admits(&m, &settings), "{kind:?}");
        }

        let mut system = message("You are now in slow mode");
        system.kind = MessageKind::System;
        assert!(!admits(&system, &settings));
    }

    #[test]
    fn a_cheer_is_still_the_chatters_message() {
        let mut m = message("cheer100 great stream");
        m.event = event(ChannelEventKind::Cheer);
        assert!(admits(&m, &OverlaySettings::default()));
    }

    #[test]
    fn commands_and_bots_can_be_kept_off_stream() {
        let settings = OverlaySettings::default();
        assert!(!admits(&message("!song"), &settings));
        assert!(!admits(&message("  !discord"), &settings));

        let mut bot = message("Current song: something");
        bot.author.login = Login::parse("nightbot").unwrap();
        assert!(!admits(&bot, &settings));

        let open = OverlaySettings {
            hide_commands: false,
            hidden_users: Vec::new(),
            ..Default::default()
        };
        assert!(admits(&message("!song"), &open));
        assert!(admits(&bot, &open));
    }

    #[test]
    fn deleted_and_empty_messages_are_left_out() {
        let settings = OverlaySettings::default();
        let mut deleted = message("gone");
        deleted.state = MessageState::Deleted;
        assert!(!admits(&deleted, &settings));
        assert!(!admits(&message("   "), &settings));
    }

    #[test]
    fn a_message_becomes_parts_the_page_can_draw() {
        let mut m = message("");
        m.kind = MessageKind::Action;
        m.author.badges = vec![
            Badge {
                url_1x: Some(
                    "https://static-cdn.jtvnw.net/badges/v1/x/1"
                        .parse()
                        .unwrap(),
                ),
                url_2x: Some(
                    "https://static-cdn.jtvnw.net/badges/v1/x/2"
                        .parse()
                        .unwrap(),
                ),
                title: "Moderator".into(),
                ..Badge::unresolved("moderator".into(), "1".into(), String::new())
            },
            // Not resolved yet: there is nothing to draw, so it is skipped
            // rather than sent as a broken image.
            Badge::unresolved("vip".into(), "1".into(), String::new()),
        ];
        m.tokens = vec![
            ChatToken::Mention {
                id: UserId("3".into()),
                login: Login::parse("nymn").unwrap(),
                display: "NymN".into(),
            },
            ChatToken::Text {
                text: " look ".into(),
            },
            ChatToken::Emote {
                emote: Box::new(Emote {
                    id: "25".into(),
                    name: "Kappa".into(),
                    provider: ProviderId::Twitch,
                    scope: EmoteScope::Global,
                    url_1x: "https://static-cdn.jtvnw.net/e/1".parse().unwrap(),
                    url_2x: Some("https://static-cdn.jtvnw.net/e/2".parse().unwrap()),
                    url_3x: Some("https://static-cdn.jtvnw.net/e/3".parse().unwrap()),
                    animated: false,
                    aspect: Some((28, 28)),
                    zero_width: false,
                }),
            },
            ChatToken::Link {
                url: "https://example.com/".parse().unwrap(),
                display: "example.com".into(),
            },
        ];

        let out = OverlayMessage::from_chat(&m);
        assert_eq!(out.name, "Viewer");
        assert_eq!(out.color.as_deref(), Some("#ff0000"));
        assert!(out.action);
        assert_eq!(out.at, 1_700_000_000_000);
        assert_eq!(out.badges.len(), 1);
        assert!(
            out.badges[0].url.ends_with("/2"),
            "the largest image there is"
        );
        assert_eq!(
            out.parts,
            vec![
                Part::Mention {
                    text: "@NymN".into()
                },
                Part::Text {
                    text: " look ".into()
                },
                Part::Emote {
                    name: "Kappa".into(),
                    url: "https://static-cdn.jtvnw.net/e/3".into(),
                    aspect: Some((28, 28)),
                    zero_width: false,
                },
                Part::Text {
                    text: "example.com".into()
                },
            ]
        );
    }

    #[test]
    fn the_look_carries_only_safe_values() {
        let settings = OverlaySettings {
            font: ChatFont::Custom,
            custom_font: "Comic Neue".into(),
            twitch_name_colors: false,
            bubble_opacity: 50,
            ..Default::default()
        };
        let look = OverlayLook::from_settings(&settings);
        assert_eq!(look.font, "\"Comic Neue\", system-ui, sans-serif");
        assert_eq!(look.name_color.as_deref(), Some("#bf94ff"));
        assert!((look.bubble_opacity - 0.5).abs() < f32::EPSILON);

        let json = serde_json::to_string(&look).unwrap();
        assert!(json.contains(r#""layout":"bubbles""#));
    }

    #[test]
    fn a_hostile_font_name_cannot_leave_the_value() {
        let stack = font_stack(ChatFont::Custom, "x\"; } body { display: none");
        assert!(!stack.contains(';') && !stack.contains('}'));
        assert_eq!(
            font_stack(ChatFont::Custom, "\"\";"),
            "system-ui, sans-serif"
        );
    }
}

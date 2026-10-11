//! `channel.chat.message` -> [`ChatMessage`].
//!
//! EventSub delivers the message pre-split into fragments, so Twitch emotes are
//! a direct mapping rather than index arithmetic over a raw string. This is the
//! main reason EventSub was chosen over IRC (`docs/architecture.md` §5.3).

use lurkn_core::chat::{fallback_color, ChatAuthor, ChatMessage, ChatToken, MessageState, Rgb};
use lurkn_core::emote::twitch_emote;
use lurkn_core::{Badge, ChannelEvent, ChannelEventKind, Login, MessageId, MessageKind, UserId};
use serde::Deserialize;
use time::OffsetDateTime;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct ChatMessageEvent {
    pub broadcaster_user_id: String,
    pub chatter_user_id: String,
    pub chatter_user_login: String,
    pub chatter_user_name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub badges: Vec<BadgeRef>,
    pub message_id: String,
    pub message: MessageBody,
    #[serde(default)]
    pub message_type: String,
    #[serde(default)]
    pub reply: Option<Reply>,
    /// Present only when the message carried bits.
    #[serde(default)]
    pub cheer: Option<Cheer>,
}

#[derive(Debug, Deserialize)]
pub struct Cheer {
    #[serde(default)]
    pub bits: u32,
}

#[derive(Debug, Deserialize)]
pub struct BadgeRef {
    pub set_id: String,
    pub id: String,
    #[serde(default)]
    pub info: String,
}

#[derive(Debug, Deserialize)]
pub struct Reply {
    pub parent_message_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageBody {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub fragments: Vec<Fragment>,
}

#[derive(Debug, Deserialize)]
pub struct Fragment {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub emote: Option<FragmentEmote>,
    #[serde(default)]
    pub mention: Option<FragmentMention>,
}

#[derive(Debug, Deserialize)]
pub struct FragmentEmote {
    pub id: String,
    #[serde(default)]
    pub format: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FragmentMention {
    #[serde(default)]
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
}

/// Turn an event into a domain message.
///
/// Returns `None` only when the chatter's login is not something Twitch should
/// ever have produced — skipping beats guessing.
pub fn to_message(event: ChatMessageEvent, received_at: OffsetDateTime) -> Option<ChatMessage> {
    let login = Login::parse(&event.chatter_user_login).ok()?;

    let color = Rgb::parse(&event.color).unwrap_or_else(|| fallback_color(&login));
    let tokens = tokenize(&event.message);

    let kind = match event.message_type.as_str() {
        "channel_points_highlighted"
        | "power_ups_gigantified_emote"
        | "power_ups_message_effect" => MessageKind::Highlight,
        // Twitch reports /me only through the text itself.
        _ if event.message.text.starts_with("\u{1}ACTION ") => MessageKind::Action,
        _ => MessageKind::Chat,
    };

    Some(ChatMessage {
        id: MessageId(event.message_id),
        channel_id: UserId(event.broadcaster_user_id),
        author: ChatAuthor {
            id: UserId(event.chatter_user_id),
            login,
            display_name: event.chatter_user_name,
            color: Some(color),
            // Carried unresolved: turning a set/version pair into images needs
            // Twitch's badge endpoints, which this crate deliberately does not call.
            badges: event
                .badges
                .iter()
                .map(|b| Badge::unresolved(b.set_id.clone(), b.id.clone(), b.info.clone()))
                .collect(),
        },
        tokens,
        raw_text: event.message.text,
        sent_at: received_at,
        kind,
        state: MessageState::Sent,
        reply_to: event.reply.map(|r| MessageId(r.parent_message_id)),
        // Bits do not replace the message, they decorate it: the chatter's own
        // words are still the line, with the amount shown beside them.
        event: event.cheer.filter(|c| c.bits > 0).map(|c| ChannelEvent {
            kind: ChannelEventKind::Cheer,
            text: String::new(),
            count: Some(c.bits),
        }),
        // Decided by the pump, which is where the reader's own name and their
        // highlight words live; this crate knows neither.
        highlighted: false,
        highlight_color: None,
    })
}

/// Map fragments to tokens. Text fragments are split further so links and bare
/// mentions inside them become their own tokens.
pub(crate) fn tokenize(body: &MessageBody) -> Vec<ChatToken> {
    // A message with no fragments at all still has its text.
    if body.fragments.is_empty() {
        return split_text(&body.text);
    }

    let mut out = Vec::new();
    for fragment in &body.fragments {
        match fragment.kind.as_str() {
            "emote" => {
                let animated = fragment
                    .emote
                    .as_ref()
                    .is_some_and(|e| e.format.iter().any(|f| f == "animated"));
                match fragment
                    .emote
                    .as_ref()
                    .and_then(|e| twitch_emote(&e.id, &fragment.text, animated))
                {
                    Some(emote) => out.push(ChatToken::Emote {
                        emote: Box::new(emote),
                    }),
                    // An emote we cannot build a URL for still reads as its name.
                    None => out.push(ChatToken::Text {
                        text: fragment.text.clone(),
                    }),
                }
            }
            "mention" => match fragment.mention.as_ref().and_then(|m| {
                Login::parse(&m.user_login)
                    .ok()
                    .map(|login| (UserId(m.user_id.clone()), login, m.user_name.clone()))
            }) {
                Some((id, login, display)) => out.push(ChatToken::Mention { id, login, display }),
                None => out.push(ChatToken::Text {
                    text: fragment.text.clone(),
                }),
            },
            // Cheermotes render as their text; their images are not resolved.
            _ => out.extend(split_text(&fragment.text)),
        }
    }
    out
}

/// Split a run of plain text into text and link tokens.
fn split_text(text: &str) -> Vec<ChatToken> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<ChatToken> = Vec::new();
    let mut pending = String::new();

    for word in text.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        match parse_link(trimmed) {
            Some(url) => {
                if !pending.is_empty() {
                    out.push(ChatToken::Text {
                        text: std::mem::take(&mut pending),
                    });
                }
                out.push(ChatToken::Link {
                    url,
                    display: trimmed.to_owned(),
                });
                // Keep whatever whitespace followed the link.
                pending.push_str(&word[trimmed.len()..]);
            }
            None => pending.push_str(word),
        }
    }

    if !pending.is_empty() {
        out.push(ChatToken::Text { text: pending });
    }
    out
}

/// Accept only absolute http(s) URLs. Anything else stays text, so a
/// `javascript:` "link" can never reach the renderer (`docs/architecture.md` §11).
fn parse_link(word: &str) -> Option<Url> {
    if !(word.starts_with("http://") || word.starts_with("https://")) {
        return None;
    }
    let url = Url::parse(word).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host().is_some()).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: OffsetDateTime = time::macros::datetime!(2026-09-20 12:00:00 UTC);

    fn event(json: &str) -> ChatMessageEvent {
        serde_json::from_str(json).expect("fixture should parse")
    }

    fn plain(text: &str) -> ChatMessageEvent {
        event(&format!(
            r##"{{"broadcaster_user_id":"11111","chatter_user_id":"22222",
                 "chatter_user_login":"forsen","chatter_user_name":"Forsen",
                 "color":"#1E90FF","badges":[],"message_id":"M1",
                 "message":{{"text":{},"fragments":[{{"type":"text","text":{}}}]}},
                 "message_type":"text"}}"##,
            serde_json::to_string(text).unwrap(),
            serde_json::to_string(text).unwrap(),
        ))
    }

    #[test]
    fn maps_a_plain_message() {
        let msg = to_message(plain("hello there"), NOW).unwrap();
        assert_eq!(msg.id, MessageId("M1".into()));
        assert_eq!(msg.author.login.as_str(), "forsen");
        assert_eq!(msg.author.display_name, "Forsen");
        assert_eq!(msg.author.color, Rgb::parse("#1E90FF"));
        assert_eq!(
            msg.tokens,
            vec![ChatToken::Text {
                text: "hello there".into()
            }]
        );
        assert_eq!(msg.kind, MessageKind::Chat);
        assert_eq!(msg.state, MessageState::Sent);
    }

    #[test]
    fn a_chatter_without_a_colour_gets_a_stable_fallback() {
        let mut e = plain("hi");
        e.color = String::new();
        let msg = to_message(e, NOW).unwrap();
        let expected = fallback_color(&Login::parse("forsen").unwrap());
        assert_eq!(msg.author.color, Some(expected));
    }

    /// The whole point of EventSub over IRC: emotes arrive as their own fragments.
    #[test]
    fn emote_fragments_become_emote_tokens() {
        let e = event(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2",
                "chatter_user_login":"forsen","chatter_user_name":"Forsen",
                "color":"#FF0000","message_id":"M1","message_type":"text",
                "message":{"text":"hey Kappa there",
                  "fragments":[
                    {"type":"text","text":"hey "},
                    {"type":"emote","text":"Kappa","emote":{"id":"25","format":["static"]}},
                    {"type":"text","text":" there"}]}}"##,
        );
        let tokens = to_message(e, NOW).unwrap().tokens;
        assert_eq!(tokens.len(), 3);
        assert_eq!(
            tokens[0],
            ChatToken::Text {
                text: "hey ".into()
            }
        );
        match &tokens[1] {
            ChatToken::Emote { emote } => {
                assert_eq!(emote.name, "Kappa");
                assert_eq!(emote.provider, lurkn_core::ProviderId::Twitch);
                assert!(!emote.animated);
            }
            other => panic!("expected an emote, got {other:?}"),
        }
        assert_eq!(
            tokens[2],
            ChatToken::Text {
                text: " there".into()
            }
        );
    }

    #[test]
    fn an_animated_emote_is_marked_animated() {
        let e = event(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2",
                "chatter_user_login":"forsen","chatter_user_name":"F","message_id":"M1",
                "message":{"text":"x","fragments":[
                  {"type":"emote","text":"x","emote":{"id":"9","format":["static","animated"]}}]}}"##,
        );
        match &to_message(e, NOW).unwrap().tokens[0] {
            ChatToken::Emote { emote } => {
                assert!(emote.animated);
                assert!(emote.url_1x.as_str().contains("/animated/"));
            }
            other => panic!("expected an emote, got {other:?}"),
        }
    }

    #[test]
    fn mention_fragments_become_mention_tokens() {
        let e = event(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2",
                "chatter_user_login":"forsen","chatter_user_name":"F","message_id":"M1",
                "message":{"text":"@nymn hi","fragments":[
                  {"type":"mention","text":"@nymn",
                   "mention":{"user_id":"3","user_login":"nymn","user_name":"NymN"}},
                  {"type":"text","text":" hi"}]}}"##,
        );
        let tokens = to_message(e, NOW).unwrap().tokens;
        assert_eq!(
            tokens[0],
            ChatToken::Mention {
                id: UserId("3".into()),
                login: Login::parse("nymn").unwrap(),
                display: "NymN".into()
            }
        );
    }

    #[test]
    fn links_in_text_become_link_tokens() {
        let msg = to_message(plain("look at https://example.com/x now"), NOW).unwrap();
        assert_eq!(msg.tokens.len(), 3);
        assert_eq!(
            msg.tokens[0],
            ChatToken::Text {
                text: "look at ".into()
            }
        );
        match &msg.tokens[1] {
            ChatToken::Link { url, display } => {
                assert_eq!(url.as_str(), "https://example.com/x");
                assert_eq!(display, "https://example.com/x");
            }
            other => panic!("expected a link, got {other:?}"),
        }
        assert_eq!(
            msg.tokens[2],
            ChatToken::Text {
                text: " now".into()
            }
        );
    }

    /// A hostile scheme must never become a clickable link.
    #[test]
    fn dangerous_schemes_never_become_links() {
        for hostile in [
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "vbscript:msgbox",
            "jAvAsCrIpT:alert(1)",
        ] {
            let msg = to_message(plain(hostile), NOW).unwrap();
            assert!(
                msg.tokens
                    .iter()
                    .all(|t| !matches!(t, ChatToken::Link { .. })),
                "{hostile} became a link: {:?}",
                msg.tokens
            );
        }
    }

    #[test]
    fn markup_in_a_message_stays_plain_text() {
        let msg = to_message(plain("<script>alert('x')</script>"), NOW).unwrap();
        assert_eq!(
            msg.tokens,
            vec![ChatToken::Text {
                text: "<script>alert('x')</script>".into()
            }]
        );
        assert_eq!(msg.raw_text, "<script>alert('x')</script>");
    }

    #[test]
    fn multibyte_and_emoji_text_survives_intact() {
        let text = "héllo 🎉 日本語 ünïcode";
        let msg = to_message(plain(text), NOW).unwrap();
        assert_eq!(msg.tokens, vec![ChatToken::Text { text: text.into() }]);
    }

    #[test]
    fn a_channel_points_highlight_is_marked_as_one() {
        let mut e = plain("redeemed");
        e.message_type = "channel_points_highlighted".into();
        assert_eq!(to_message(e, NOW).unwrap().kind, MessageKind::Highlight);
    }

    #[test]
    fn a_reply_records_its_parent() {
        let e = event(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2",
                "chatter_user_login":"forsen","chatter_user_name":"F","message_id":"M2",
                "message":{"text":"yes","fragments":[{"type":"text","text":"yes"}]},
                "reply":{"parent_message_id":"M1","parent_user_id":"9",
                         "parent_user_login":"nymn","parent_user_name":"NymN",
                         "parent_message_body":"?","thread_message_id":"M1",
                         "thread_user_id":"9","thread_user_login":"nymn",
                         "thread_user_name":"NymN"}}"##,
        );
        assert_eq!(
            to_message(e, NOW).unwrap().reply_to,
            Some(MessageId("M1".into()))
        );
    }

    #[test]
    fn a_message_with_no_fragments_still_yields_its_text() {
        let e = event(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2",
                "chatter_user_login":"forsen","chatter_user_name":"F","message_id":"M1",
                "message":{"text":"bare text"}}"##,
        );
        assert_eq!(
            to_message(e, NOW).unwrap().tokens,
            vec![ChatToken::Text {
                text: "bare text".into()
            }]
        );
    }

    #[test]
    fn a_chatter_with_an_unusable_login_is_skipped() {
        let mut e = plain("hi");
        e.chatter_user_login = "not a login!".into();
        assert!(to_message(e, NOW).is_none());
    }

    #[test]
    fn an_empty_message_produces_no_tokens() {
        let e = event(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2",
                "chatter_user_login":"forsen","chatter_user_name":"F","message_id":"M1",
                "message":{"text":"","fragments":[]}}"##,
        );
        assert!(to_message(e, NOW).unwrap().tokens.is_empty());
    }

    #[test]
    fn badges_arrive_unresolved_and_in_order() {
        let e = event(
            r##"{"broadcaster_user_id":"1","chatter_user_id":"2",
                "chatter_user_login":"forsen","chatter_user_name":"F","message_id":"M1",
                "badges":[{"set_id":"moderator","id":"1","info":""},
                          {"set_id":"subscriber","id":"12","info":"24"}],
                "message":{"text":"x","fragments":[{"type":"text","text":"x"}]}}"##,
        );
        let badges = to_message(e, NOW).unwrap().author.badges;
        assert_eq!(badges.len(), 2);
        assert_eq!(badges[0].set_id, "moderator");
        assert_eq!(badges[0].version, "1");
        assert_eq!(badges[1].set_id, "subscriber");
        // The only published source of a subscription's length; see Badge::info.
        assert_eq!(badges[1].info, "24");
        assert!(!badges[0].is_resolved(), "images are filled in later");
    }
}

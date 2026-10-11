//! Messages sent before Lurkn joined a channel.
//!
//! Twitch serves no chat history. `recent-messages.robotty.de` does, and it
//! answers with raw IRCv3 lines — so reading it
//! means writing the tag and emote-offset parser that `docs/architecture.md`
//! §5.3 chose EventSub to avoid. It lives here, apart from the EventSub path,
//! and is used only for the backfill: live chat never goes through it.
//!
//! Third-party, therefore opt-in. The channel being read is disclosed to that
//! service on every join, which is the user's decision to make, not this
//! module's — see the `historyOnJoin` setting.

use lurkn_core::chat::{fallback_color, ChatAuthor, ChatMessage, ChatToken, MessageState, Rgb};
use lurkn_core::emote::twitch_emote;
use lurkn_core::{Badge, Login, MessageId, MessageKind, UserId};
use time::OffsetDateTime;

/// What the service answers with.
#[derive(Debug, serde::Deserialize)]
pub struct RecentMessages {
    #[serde(default)]
    pub messages: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Parse a batch, oldest first, dropping anything that is not a chat message.
///
/// The feed carries USERNOTICE, CLEARCHAT and ROOMSTATE lines too. Those are
/// live-only concerns — a timeout from an hour ago is not something to act on —
/// so only PRIVMSG survives.
pub fn parse_batch(lines: &[String], channel_id: &str) -> Vec<ChatMessage> {
    lines
        .iter()
        .filter_map(|line| parse_line(line, channel_id))
        .collect()
}

/// One IRC line to a domain message, or `None` if it is not one.
pub fn parse_line(line: &str, channel_id: &str) -> Option<ChatMessage> {
    let (tags, rest) = split_tags(line);

    // `:nick!user@host PRIVMSG #channel :text`
    let rest = rest.strip_prefix(':')?;
    let (prefix, rest) = rest.split_once(' ')?;
    let (command, rest) = rest.split_once(' ')?;
    if command != "PRIVMSG" {
        return None;
    }
    let (_channel, text) = rest.split_once(" :")?;

    let login = Login::parse(prefix.split('!').next()?).ok()?;

    let display_name = tag(&tags, "display-name")
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| login.to_string());

    let color = tag(&tags, "color")
        .and_then(|value| Rgb::parse(&value))
        .unwrap_or_else(|| fallback_color(&login));

    let id =
        tag(&tags, "id").unwrap_or_else(|| format!("history:{}:{}", login.as_str(), text.len()));

    let sent_at = tag(&tags, "tmi-sent-ts")
        .and_then(|ms| ms.parse::<i64>().ok())
        .and_then(|ms| OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000).ok())
        // A line with no timestamp is still worth showing; it just sorts by
        // where it arrived rather than when it was sent.
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);

    // `/me` arrives wrapped in the same control characters as on EventSub.
    let action = text.starts_with('\u{1}');
    let text = text
        .trim_start_matches('\u{1}')
        .trim_end_matches('\u{1}')
        .strip_prefix("ACTION ")
        .unwrap_or(text.trim_matches('\u{1}'))
        .to_owned();

    Some(ChatMessage {
        id: MessageId(id),
        channel_id: UserId(channel_id.to_owned()),
        author: ChatAuthor {
            id: UserId(tag(&tags, "user-id").unwrap_or_default()),
            login,
            display_name,
            color: Some(color),
            badges: parse_badges(&tags),
        },
        tokens: tokenize(&text, tag(&tags, "emotes").as_deref()),
        raw_text: text,
        sent_at,
        kind: if action {
            MessageKind::Action
        } else {
            MessageKind::Chat
        },
        state: MessageState::Sent,
        reply_to: tag(&tags, "reply-parent-msg-id").map(MessageId),
        event: None,
        // Decided by the pump for live messages; backfill is never announced,
        // so an old mention cannot ring an hour late.
        highlighted: false,
        highlight_color: None,
    })
}

/// Split the `@a=1;b=2 ` prefix from the rest of the line.
fn split_tags(line: &str) -> (Vec<(String, String)>, &str) {
    let Some(rest) = line.strip_prefix('@') else {
        return (Vec::new(), line);
    };
    let Some((raw, rest)) = rest.split_once(' ') else {
        return (Vec::new(), line);
    };

    let tags = raw
        .split(';')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((key.to_owned(), unescape(value)))
        })
        .collect();

    (tags, rest)
}

fn tag(tags: &[(String, String)], key: &str) -> Option<String> {
    tags.iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

/// IRCv3 tag values escape the characters that would otherwise end the tag.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();

    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some(':') => out.push(';'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => {}
        }
    }

    out
}

/// `badges=subscriber/12,moderator/1`, with months from `badge-info`.
fn parse_badges(tags: &[(String, String)]) -> Vec<Badge> {
    let Some(raw) = tag(tags, "badges") else {
        return Vec::new();
    };
    let info = tag(tags, "badge-info").unwrap_or_default();

    raw.split(',')
        .filter_map(|entry| {
            let (set, version) = entry.split_once('/')?;
            // badge-info carries the real month count for subscriber badges,
            // which the badge version rounds down to a tier.
            let detail = info
                .split(',')
                .filter_map(|pair| pair.split_once('/'))
                .find(|(name, _)| *name == set)
                .map(|(_, value)| value.to_owned())
                .unwrap_or_default();

            Some(Badge::unresolved(
                set.to_owned(),
                version.to_owned(),
                detail,
            ))
        })
        .collect()
}

/// Split text into tokens, substituting Twitch emotes at the offsets the
/// `emotes` tag gives.
///
/// The offsets count characters, not bytes, which is the whole reason this is
/// fiddly: a message with an emoji before an emote has different byte and
/// character positions, and using the wrong one splits a character in half.
fn tokenize(text: &str, emotes: Option<&str>) -> Vec<ChatToken> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = emote_spans(emotes);
    if spans.is_empty() {
        return text_tokens(text);
    }

    spans.sort_by_key(|(start, _, _)| *start);

    let mut tokens = Vec::new();
    let mut cursor = 0usize;

    for (start, end, id) in spans {
        if start > chars.len() || end >= chars.len() || start < cursor {
            continue;
        }

        if start > cursor {
            let plain: String = chars[cursor..start].iter().collect();
            tokens.extend(text_tokens(&plain));
        }

        let name: String = chars[start..=end].iter().collect();
        // Animation is not in the tag; the static URL is right either way and
        // the animated one would 404 for a still emote.
        match twitch_emote(&id, &name, false) {
            Some(emote) => tokens.push(ChatToken::Emote {
                emote: Box::new(emote),
            }),
            // An emote whose URL cannot be built still reads as its name.
            None => tokens.push(ChatToken::Text { text: name }),
        }

        cursor = end + 1;
    }

    if cursor < chars.len() {
        let plain: String = chars[cursor..].iter().collect();
        tokens.extend(text_tokens(&plain));
    }

    tokens
}

/// `emotes=25:0-4,12-16/1902:6-10` -> `(start, end, id)` per occurrence.
fn emote_spans(emotes: Option<&str>) -> Vec<(usize, usize, String)> {
    let Some(raw) = emotes.filter(|value| !value.is_empty()) else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    for group in raw.split('/') {
        let Some((id, ranges)) = group.split_once(':') else {
            continue;
        };
        for range in ranges.split(',') {
            let Some((start, end)) = range.split_once('-') else {
                continue;
            };
            if let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) {
                if end >= start {
                    spans.push((start, end, id.to_owned()));
                }
            }
        }
    }

    spans
}

/// Plain text, kept as one token.
///
/// Links and mentions are deliberately not detected here: the EventSub path
/// gets them as fragments from Twitch, and guessing at them with a second set
/// of rules would make backfilled lines behave differently from live ones.
fn text_tokens(text: &str) -> Vec<ChatToken> {
    if text.is_empty() {
        return Vec::new();
    }
    vec![ChatToken::Text {
        text: text.to_owned(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANNEL: &str = "171103531";

    fn privmsg(tags: &str, text: &str) -> String {
        format!("@{tags} :forsen!forsen@forsen.tmi.twitch.tv PRIVMSG #sieglinde :{text}")
    }

    #[test]
    fn a_plain_line_becomes_a_message() {
        let message = parse_line(
            &privmsg(
                "id=M1;display-name=Forsen;user-id=22484632;color=#FF0000;tmi-sent-ts=1758456000000",
                "hello chat",
            ),
            CHANNEL,
        )
        .expect("a PRIVMSG parses");

        assert_eq!(message.raw_text, "hello chat");
        assert_eq!(message.author.display_name, "Forsen");
        assert_eq!(message.author.id.as_str(), "22484632");
        assert_eq!(message.id.0, "M1");
        assert_eq!(message.channel_id.as_str(), CHANNEL);
        assert_eq!(message.author.color, Some(Rgb { r: 255, g: 0, b: 0 }));
    }

    #[test]
    fn only_chat_messages_survive() {
        assert!(parse_line(
            "@msg-id=sub :tmi.twitch.tv USERNOTICE #sieglinde :subbed",
            CHANNEL
        )
        .is_none());
        assert!(parse_line(":tmi.twitch.tv CLEARCHAT #sieglinde", CHANNEL).is_none());
        assert!(parse_line("nonsense", CHANNEL).is_none());
    }

    /// The tag escapes exist so a display name can contain a space or a
    /// semicolon without ending the tag list.
    #[test]
    fn tag_values_are_unescaped() {
        let message = parse_line(
            &privmsg(
                "id=M2;display-name=A\\sB\\:C;tmi-sent-ts=1758456000000",
                "hi",
            ),
            CHANNEL,
        )
        .unwrap();
        assert_eq!(message.author.display_name, "A B;C");
    }

    #[test]
    fn badges_carry_their_month_count() {
        let message = parse_line(
            &privmsg(
                "id=M3;badges=subscriber/12,moderator/1;badge-info=subscriber/26;tmi-sent-ts=1758456000000",
                "hi",
            ),
            CHANNEL,
        )
        .unwrap();

        let badges = &message.author.badges;
        assert_eq!(badges.len(), 2);
        assert_eq!(badges[0].set_id, "subscriber");
        assert_eq!(badges[0].version, "12");
        assert_eq!(badges[0].info, "26", "the real month count, not the tier");
        assert_eq!(badges[1].set_id, "moderator");
        assert!(badges[1].info.is_empty());
    }

    #[test]
    fn an_emote_is_cut_out_of_the_text() {
        let message = parse_line(
            &privmsg(
                "id=M4;emotes=25:6-10;tmi-sent-ts=1758456000000",
                "hello Kappa now",
            ),
            CHANNEL,
        )
        .unwrap();

        assert_eq!(message.tokens.len(), 3);
        assert!(matches!(&message.tokens[0], ChatToken::Text { text } if text == "hello "));
        assert!(matches!(&message.tokens[1], ChatToken::Emote { emote } if emote.name == "Kappa"));
        assert!(matches!(&message.tokens[2], ChatToken::Text { text } if text == " now"));
    }

    /// The offsets count characters. With a multi-byte character before the
    /// emote, byte offsets would cut it in half.
    #[test]
    fn emote_offsets_are_counted_in_characters() {
        let message = parse_line(
            &privmsg("id=M5;emotes=25:2-6;tmi-sent-ts=1758456000000", "🎉 Kappa"),
            CHANNEL,
        )
        .unwrap();

        assert!(matches!(&message.tokens[1], ChatToken::Emote { emote } if emote.name == "Kappa"));
    }

    #[test]
    fn several_emotes_are_placed_in_order() {
        let message = parse_line(
            &privmsg(
                "id=M6;emotes=25:0-4,6-10;tmi-sent-ts=1758456000000",
                "Kappa Kappa",
            ),
            CHANNEL,
        )
        .unwrap();

        let emotes = message
            .tokens
            .iter()
            .filter(|t| matches!(t, ChatToken::Emote { .. }))
            .count();
        assert_eq!(emotes, 2);
    }

    #[test]
    fn an_action_keeps_its_kind_and_loses_its_wrapper() {
        let message = parse_line(
            &privmsg("id=M7;tmi-sent-ts=1758456000000", "\u{1}ACTION waves\u{1}"),
            CHANNEL,
        )
        .unwrap();

        assert_eq!(message.kind, MessageKind::Action);
        assert_eq!(message.raw_text, "waves");
    }

    #[test]
    fn a_reply_keeps_its_parent() {
        let message = parse_line(
            &privmsg(
                "id=M8;reply-parent-msg-id=M1;tmi-sent-ts=1758456000000",
                "yes",
            ),
            CHANNEL,
        )
        .unwrap();
        assert_eq!(message.reply_to, Some(MessageId("M1".into())));
    }

    /// Backfill must never ring a notification for an hour-old mention.
    #[test]
    fn history_never_counts_as_a_mention() {
        let message = parse_line(
            &privmsg("id=M9;tmi-sent-ts=1758456000000", "hey lurker"),
            CHANNEL,
        )
        .unwrap();
        assert!(!message.highlighted);
    }

    #[test]
    fn a_batch_keeps_what_it_can_and_drops_the_rest() {
        let lines = vec![
            privmsg("id=A;tmi-sent-ts=1758456000000", "one"),
            ":tmi.twitch.tv CLEARCHAT #sieglinde".to_owned(),
            privmsg("id=B;tmi-sent-ts=1758456001000", "two"),
        ];

        let parsed = parse_batch(&lines, CHANNEL);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].raw_text, "one");
        assert_eq!(parsed[1].raw_text, "two");
    }
}

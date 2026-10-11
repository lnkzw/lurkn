//! Highlights and ignores: which messages are marked, hidden, or rewritten.
//!
//! Compiled once from [`Settings`] and then asked about every message, so a
//! regex is built when the reader saves a rule rather than 30 times a second.
//! A pattern that does not compile is skipped, not fatal: one typo must not
//! switch off every other rule. The settings screen asks [`check`] first, so
//! the reader sees the mistake where they made it.
//!
//! Patterns go through the `regex` crate, whose matching is linear in the
//! input. That matters here more than usual: the pattern is the reader's, but
//! the text it runs against is whatever a stranger in chat chose to send.

use std::collections::{HashMap, HashSet};

use regex::{NoExpand, Regex, RegexBuilder};

use crate::chat::{ChatMessage, ChatToken, Rgb};
use crate::settings::{normalize_login, IgnoreAction, Settings};

/// Generous for anything typed by hand; a ceiling on what a pasted monster can
/// cost in memory.
const SIZE_LIMIT: usize = 1 << 20;

/// How a matched message is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mark {
    /// `None` means the accent.
    pub color: Option<Rgb>,
    /// Raise a desktop notification as well.
    pub notify: bool,
}

#[derive(Debug)]
struct Replace {
    regex: Regex,
    /// `None` hides the whole message.
    with: Option<String>,
    /// Whether `$1` in the replacement refers to a group. Only for a regex: in
    /// plain text, a `$` is a dollar sign.
    expand: bool,
}

#[derive(Debug, Default)]
pub struct ChatRules {
    phrases: Vec<(Regex, Mark)>,
    users: HashMap<String, Mark>,
    badges: Vec<(String, Mark)>,
    excluded: HashSet<String>,
    ignored_users: HashSet<String>,
    ignores: Vec<Replace>,
}

/// Build the regex a rule stands for.
///
/// `whole_word` is the highlight rule for plain text: the phrase must not be
/// flanked by a letter or digit, so `@forsen,` counts and `forsenE` does not.
/// The flanks are matched rather than looked around, which the `regex` crate
/// cannot do; only *whether* it matches is ever asked of a highlight, so the
/// extra character consumed does not matter.
fn compile(
    pattern: &str,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
) -> Result<Regex, regex::Error> {
    let source = if regex {
        pattern.to_owned()
    } else if whole_word {
        format!(
            r"(?:^|[^\p{{Alphabetic}}\p{{N}}]){}(?:$|[^\p{{Alphabetic}}\p{{N}}])",
            regex::escape(pattern.trim())
        )
    } else {
        regex::escape(pattern)
    };

    RegexBuilder::new(&source)
        .case_insensitive(!case_sensitive)
        .size_limit(SIZE_LIMIT)
        .dfa_size_limit(SIZE_LIMIT)
        .build()
}

/// Whether a pattern compiles, and why not, in words for the settings screen.
pub fn check(pattern: &str, regex: bool) -> Result<(), String> {
    if !regex || pattern.is_empty() {
        return Ok(());
    }
    compile(pattern, true, true, false)
        .map(|_| ())
        .map_err(|error| match error {
            regex::Error::Syntax(detail) => {
                // The crate's message is a multi-line diagram; its last line
                // is the sentence that says what is wrong.
                detail
                    .lines()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("not a valid pattern")
                    .trim()
                    .trim_start_matches("error: ")
                    .to_owned()
            }
            regex::Error::CompiledTooBig(_) => "too large".to_owned(),
            _ => "not a valid pattern".to_owned(),
        })
}

impl ChatRules {
    pub fn compile(settings: &Settings) -> Self {
        let mut rules = Self::default();
        let highlights = &settings.highlights;

        for phrase in &highlights.phrases {
            if phrase.pattern.trim().is_empty() {
                continue;
            }
            match compile(&phrase.pattern, phrase.regex, phrase.case_sensitive, true) {
                Ok(regex) => rules.phrases.push((
                    regex,
                    Mark {
                        color: phrase.color,
                        notify: phrase.notify,
                    },
                )),
                Err(_) => continue,
            }
        }

        for user in &highlights.users {
            let login = normalize_login(&user.login);
            if !login.is_empty() {
                rules.users.entry(login).or_insert(Mark {
                    color: user.color,
                    notify: user.notify,
                });
            }
        }

        for badge in &highlights.badges {
            let id = badge.badge.trim().to_ascii_lowercase();
            if !id.is_empty() {
                rules.badges.push((
                    id,
                    Mark {
                        color: badge.color,
                        notify: badge.notify,
                    },
                ));
            }
        }

        rules.excluded = highlights
            .excluded_users
            .iter()
            .map(|l| normalize_login(l))
            .collect();
        rules.ignored_users = settings
            .ignores
            .users
            .iter()
            .map(|l| normalize_login(l))
            .collect();

        for phrase in &settings.ignores.phrases {
            if phrase.pattern.is_empty() {
                continue;
            }
            let Ok(regex) = compile(&phrase.pattern, phrase.regex, phrase.case_sensitive, false)
            else {
                continue;
            };
            rules.ignores.push(Replace {
                regex,
                with: match phrase.action {
                    IgnoreAction::Hide => None,
                    IgnoreAction::Replace => Some(phrase.replacement.clone()),
                },
                expand: phrase.regex,
            });
        }

        rules
    }

    /// Whether a chatter's messages are never highlighted.
    pub fn is_excluded(&self, login: &str) -> bool {
        self.excluded.contains(&login.to_ascii_lowercase())
    }

    /// Apply the ignores. `false` means the message is not to be shown at all;
    /// otherwise any replacements have been made in place.
    ///
    /// Run before emotes are resolved, so a replacement that produces an emote
    /// name renders as the emote, and one that removes a name removes it.
    pub fn screen(&self, message: &mut ChatMessage) -> bool {
        if self
            .ignored_users
            .contains(&message.author.login.as_str().to_ascii_lowercase())
        {
            return false;
        }

        for rule in &self.ignores {
            if !rule.regex.is_match(&message.raw_text) {
                continue;
            }
            let Some(with) = &rule.with else {
                return false;
            };

            let swap = |text: &str| -> String {
                if rule.expand {
                    rule.regex.replace_all(text, with.as_str()).into_owned()
                } else {
                    rule.regex.replace_all(text, NoExpand(with)).into_owned()
                }
            };

            message.raw_text = swap(&message.raw_text);
            // Text only: an emote, a link or a mention is a thing Twitch or
            // the parser already decided on, not a run of characters.
            for token in &mut message.tokens {
                if let ChatToken::Text { text } = token {
                    *text = swap(text);
                }
            }
        }

        true
    }

    /// The rule a message matches, if any. Phrases first, then the chatter,
    /// then their badges — the most specific thing the reader wrote wins its
    /// colour.
    pub fn highlight(&self, message: &ChatMessage) -> Option<Mark> {
        let login = message.author.login.as_str().to_ascii_lowercase();
        if self.excluded.contains(&login) {
            return None;
        }

        if let Some((_, mark)) = self
            .phrases
            .iter()
            .find(|(regex, _)| regex.is_match(&message.raw_text))
        {
            return Some(*mark);
        }

        if let Some(mark) = self.users.get(&login) {
            return Some(*mark);
        }

        self.badges.iter().find_map(|(id, mark)| {
            message
                .author
                .badges
                .iter()
                .any(|badge| badge.set_id.eq_ignore_ascii_case(id))
                .then_some(*mark)
        })
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;
    use crate::chat::{Badge, ChatAuthor, MessageKind, MessageState};
    use crate::ids::{Login, MessageId, UserId};
    use crate::settings::{
        HighlightBadge, HighlightPhrase, HighlightUser, IgnorePhrase, IgnoreSettings,
    };

    fn message(login: &str, text: &str) -> ChatMessage {
        ChatMessage {
            id: MessageId("m".into()),
            channel_id: UserId("1".into()),
            author: ChatAuthor {
                id: UserId("2".into()),
                login: Login::parse(login).unwrap(),
                display_name: login.into(),
                color: None,
                badges: Vec::new(),
            },
            tokens: vec![ChatToken::Text { text: text.into() }],
            raw_text: text.into(),
            sent_at: OffsetDateTime::UNIX_EPOCH,
            kind: MessageKind::Chat,
            state: MessageState::Sent,
            reply_to: None,
            event: None,
            highlighted: false,
            highlight_color: None,
        }
    }

    fn phrase(pattern: &str) -> HighlightPhrase {
        HighlightPhrase {
            pattern: pattern.into(),
            ..Default::default()
        }
    }

    fn with_phrases(phrases: Vec<HighlightPhrase>) -> ChatRules {
        let mut settings = Settings::default();
        settings.highlights.phrases = phrases;
        ChatRules::compile(&settings)
    }

    fn with_ignores(ignores: IgnoreSettings) -> ChatRules {
        let settings = Settings {
            ignores,
            ..Default::default()
        };
        ChatRules::compile(&settings)
    }

    #[test]
    fn a_plain_phrase_matches_whole_words_in_any_case() {
        let rules = with_phrases(vec![phrase("poe")]);
        assert!(rules
            .highlight(&message("abc", "anyone up for POE?"))
            .is_some());
        assert!(rules.highlight(&message("abc", "poe")).is_some());
        assert!(rules.highlight(&message("abc", "poem night")).is_none());
        assert!(rules.highlight(&message("abc", "nothing")).is_none());
    }

    #[test]
    fn case_sensitivity_is_the_readers_choice() {
        let rules = with_phrases(vec![HighlightPhrase {
            case_sensitive: true,
            ..phrase("PoE")
        }]);
        assert!(rules.highlight(&message("abc", "PoE league")).is_some());
        assert!(rules.highlight(&message("abc", "poe league")).is_none());
    }

    #[test]
    fn a_regex_phrase_is_taken_as_written() {
        let rules = with_phrases(vec![HighlightPhrase {
            regex: true,
            ..phrase(r"^!\w+")
        }]);
        assert!(rules.highlight(&message("abc", "!drops")).is_some());
        assert!(rules.highlight(&message("abc", "say !drops")).is_none());
    }

    /// Regex syntax in a plain phrase is just characters.
    #[test]
    fn a_plain_phrase_is_not_a_regex() {
        let rules = with_phrases(vec![phrase("c++")]);
        assert!(rules
            .highlight(&message("abc", "learning c++ today"))
            .is_some());
        assert!(rules
            .highlight(&message("abc", "learning c today"))
            .is_none());
    }

    /// One broken pattern must not take the others with it.
    #[test]
    fn a_broken_pattern_is_skipped() {
        let rules = with_phrases(vec![
            HighlightPhrase {
                regex: true,
                ..phrase("(unclosed")
            },
            phrase("poe"),
        ]);
        assert!(rules.highlight(&message("abc", "poe")).is_some());
        assert!(check("(unclosed", true).is_err());
        assert!(check("(unclosed", false).is_ok(), "plain text never fails");
        assert!(check(r"^!\w+", true).is_ok());
    }

    #[test]
    fn users_and_badges_carry_their_colour() {
        let green = Rgb { r: 0, g: 200, b: 0 };
        let mut settings = Settings::default();
        settings.highlights.users = vec![HighlightUser {
            login: "Forsen".into(),
            color: Some(green),
            notify: true,
        }];
        settings.highlights.badges = vec![HighlightBadge {
            badge: "moderator".into(),
            color: None,
            notify: false,
        }];
        let rules = ChatRules::compile(&settings);

        let mark = rules.highlight(&message("forsen", "hi")).unwrap();
        assert_eq!(mark.color, Some(green));
        assert!(mark.notify);

        let mut modded = message("someone", "hi");
        modded.author.badges.push(Badge::unresolved(
            "moderator".into(),
            "1".into(),
            String::new(),
        ));
        assert!(rules.highlight(&modded).is_some());
        assert!(rules.highlight(&message("someone", "hi")).is_none());
    }

    #[test]
    fn an_excluded_user_never_highlights() {
        let mut settings = Settings::default();
        settings.highlights.phrases = vec![phrase("poe")];
        settings.highlights.excluded_users = vec!["nightbot".into()];
        let rules = ChatRules::compile(&settings);

        assert!(rules.highlight(&message("NightBot", "poe")).is_none());
        assert!(rules.is_excluded("NightBot"));
    }

    #[test]
    fn an_ignored_user_is_dropped() {
        let rules = with_ignores(IgnoreSettings {
            users: vec!["spammer".into()],
            ..Default::default()
        });
        assert!(!rules.screen(&mut message("Spammer", "hi")));
        assert!(rules.screen(&mut message("someone", "hi")));
    }

    /// Unlike a highlight, an ignore matches anywhere in the text.
    #[test]
    fn a_hide_phrase_drops_the_message_wherever_it_appears() {
        let rules = with_ignores(IgnoreSettings {
            phrases: vec![IgnorePhrase {
                pattern: "spoiler".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(!rules.screen(&mut message("abc", "big SPOILERS ahead")));
        assert!(rules.screen(&mut message("abc", "nothing to see")));
    }

    #[test]
    fn a_replace_phrase_rewrites_text_and_keeps_the_message() {
        let rules = with_ignores(IgnoreSettings {
            phrases: vec![IgnorePhrase {
                pattern: "$badword".into(),
                action: IgnoreAction::Replace,
                replacement: "$1***".into(),
                ..Default::default()
            }],
            ..Default::default()
        });

        let mut m = message("abc", "you $BADWORD you");
        assert!(rules.screen(&mut m));
        // Plain text: the `$` in both is a dollar sign, not a group.
        assert_eq!(m.raw_text, "you $1*** you");
        assert_eq!(
            m.tokens,
            vec![ChatToken::Text {
                text: "you $1*** you".into()
            }]
        );
    }

    #[test]
    fn a_regex_replacement_can_use_groups() {
        let rules = with_ignores(IgnoreSettings {
            phrases: vec![IgnorePhrase {
                pattern: r"(\w+)@example\.com".into(),
                regex: true,
                action: IgnoreAction::Replace,
                replacement: "$1@…".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let mut m = message("abc", "mail bob@example.com now");
        assert!(rules.screen(&mut m));
        assert_eq!(m.raw_text, "mail bob@… now");
    }

    /// Emotes, links and mentions are decided before this runs; only the
    /// words between them are the reader's to rewrite.
    #[test]
    fn a_replacement_leaves_non_text_tokens_alone() {
        let rules = with_ignores(IgnoreSettings {
            phrases: vec![IgnorePhrase {
                pattern: "forsen".into(),
                action: IgnoreAction::Replace,
                replacement: "x".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let mut m = message("abc", "@forsen forsen");
        m.tokens = vec![
            ChatToken::Mention {
                id: UserId("9".into()),
                login: Login::parse("forsen").unwrap(),
                display: "forsen".into(),
            },
            ChatToken::Text {
                text: " forsen".into(),
            },
        ];
        assert!(rules.screen(&mut m));
        assert!(matches!(&m.tokens[0], ChatToken::Mention { display, .. } if display == "forsen"));
        assert_eq!(m.tokens[1], ChatToken::Text { text: " x".into() });
    }

    /// The reason for the `regex` crate: a pattern written to backtrack is
    /// still linear, so a hostile message cannot hang the chat.
    #[test]
    fn a_pathological_pattern_does_not_hang() {
        let rules = with_phrases(vec![HighlightPhrase {
            regex: true,
            ..phrase("(a+)+$")
        }]);
        let hostile = format!("{}!", "a".repeat(500));
        let started = std::time::Instant::now();
        let _ = rules.highlight(&message("abc", &hostile));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}

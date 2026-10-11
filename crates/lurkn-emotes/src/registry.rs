//! Name resolution across providers.
//!
//! Twitch-native emotes are never name-matched — EventSub fragments are
//! authoritative. This map covers only the third-party providers, in the order
//! fixed by `docs/emote-precedence.md`.

use std::collections::HashMap;

use lurkn_core::{ChatToken, Emote};

use crate::precedence::rank;

/// A channel's resolved emotes: one lookup table, built once per channel.
#[derive(Debug, Clone, Default)]
pub struct EmoteRegistry {
    by_name: HashMap<String, Emote>,
}

impl EmoteRegistry {
    /// Build from every provider's emotes. Higher-precedence entries win, whatever
    /// order the providers happened to return in.
    pub fn build(sets: Vec<Vec<Emote>>) -> Self {
        let mut by_name: HashMap<String, Emote> = HashMap::new();

        for emote in sets.into_iter().flatten() {
            let Some(incoming) = rank(emote.provider, &emote.scope) else {
                // Twitch is not name-resolved; nothing else should be here.
                continue;
            };

            match by_name.get(&emote.name) {
                Some(existing) => {
                    let current = rank(existing.provider, &existing.scope).unwrap_or(usize::MAX);
                    if incoming < current {
                        by_name.insert(emote.name.clone(), emote);
                    }
                }
                None => {
                    by_name.insert(emote.name.clone(), emote);
                }
            }
        }

        Self { by_name }
    }

    pub fn get(&self, name: &str) -> Option<&Emote> {
        self.by_name.get(name)
    }

    /// Every emote, name-sorted, for the composer's autocomplete.
    ///
    /// Sorted here rather than at the call site so the order is stable across
    /// calls: a HashMap iterates differently every run, and a suggestion list
    /// that reshuffles between keystrokes is unusable.
    pub fn all(&self) -> Vec<Emote> {
        let mut out: Vec<Emote> = self.by_name.values().cloned().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Replace emote names inside text tokens with emote tokens.
    ///
    /// Only `Text` tokens are touched: an emote or link token is already
    /// authoritative and must not be reinterpreted.
    pub fn apply(&self, tokens: Vec<ChatToken>) -> Vec<ChatToken> {
        if self.is_empty() {
            return tokens;
        }

        let mut out = Vec::with_capacity(tokens.len());
        for token in tokens {
            match token {
                ChatToken::Text { text } => self.split_text(&text, &mut out),
                other => out.push(other),
            }
        }
        out
    }

    fn split_text(&self, text: &str, out: &mut Vec<ChatToken>) {
        let mut pending = String::new();

        for word in text.split_inclusive(char::is_whitespace) {
            let trimmed = word.trim_end();
            match self.get(trimmed) {
                Some(emote) => {
                    if !pending.is_empty() {
                        out.push(ChatToken::Text {
                            text: std::mem::take(&mut pending),
                        });
                    }
                    out.push(ChatToken::Emote {
                        emote: Box::new(emote.clone()),
                    });
                    // Preserve the whitespace that followed the emote.
                    pending.push_str(&word[trimmed.len()..]);
                }
                None => pending.push_str(word),
            }
        }

        if !pending.is_empty() {
            out.push(ChatToken::Text { text: pending });
        }
    }
}

#[cfg(test)]
mod tests {
    use lurkn_core::{EmoteScope, ProviderId};
    use url::Url;

    use super::*;

    fn emote(name: &str, provider: ProviderId, scope: EmoteScope) -> Emote {
        Emote {
            id: format!("{name}-{provider:?}"),
            name: name.to_owned(),
            provider,
            scope,
            url_1x: Url::parse("https://cdn.7tv.app/emote/1/1x.webp").unwrap(),
            url_2x: None,
            url_3x: None,
            animated: false,
            aspect: None,
            zero_width: false,
        }
    }

    fn text(s: &str) -> ChatToken {
        ChatToken::Text { text: s.to_owned() }
    }

    #[test]
    fn a_channel_emote_beats_the_same_name_globally() {
        let registry = EmoteRegistry::build(vec![
            vec![emote("Pog", ProviderId::SevenTv, EmoteScope::Global)],
            vec![emote("Pog", ProviderId::SevenTv, EmoteScope::Channel)],
        ]);
        assert_eq!(registry.get("Pog").unwrap().scope, EmoteScope::Channel);
    }

    /// The documented order must hold whatever sequence the providers return in.
    #[test]
    fn seventv_beats_bttv_beats_ffz_regardless_of_insertion_order() {
        let registry = EmoteRegistry::build(vec![
            vec![emote("OMEGALUL", ProviderId::Ffz, EmoteScope::Channel)],
            vec![emote("OMEGALUL", ProviderId::Bttv, EmoteScope::Channel)],
            vec![emote("OMEGALUL", ProviderId::SevenTv, EmoteScope::Channel)],
        ]);
        assert_eq!(
            registry.get("OMEGALUL").unwrap().provider,
            ProviderId::SevenTv
        );

        let reversed = EmoteRegistry::build(vec![
            vec![emote("OMEGALUL", ProviderId::SevenTv, EmoteScope::Channel)],
            vec![emote("OMEGALUL", ProviderId::Bttv, EmoteScope::Channel)],
            vec![emote("OMEGALUL", ProviderId::Ffz, EmoteScope::Channel)],
        ]);
        assert_eq!(
            reversed.get("OMEGALUL").unwrap().provider,
            ProviderId::SevenTv
        );
    }

    /// A global 7TV emote must not beat a channel's own BTTV emote.
    #[test]
    fn scope_outranks_provider() {
        let registry = EmoteRegistry::build(vec![
            vec![emote("Pog", ProviderId::SevenTv, EmoteScope::Global)],
            vec![emote("Pog", ProviderId::Bttv, EmoteScope::Channel)],
        ]);
        assert_eq!(registry.get("Pog").unwrap().provider, ProviderId::Bttv);
    }

    #[test]
    fn twitch_emotes_are_never_added_to_the_name_map() {
        let registry = EmoteRegistry::build(vec![vec![emote(
            "Kappa",
            ProviderId::Twitch,
            EmoteScope::Global,
        )]]);
        assert!(registry.is_empty());
        assert!(registry.get("Kappa").is_none());
    }

    #[test]
    fn replaces_whole_words_only() {
        let registry = EmoteRegistry::build(vec![vec![emote(
            "Pog",
            ProviderId::SevenTv,
            EmoteScope::Channel,
        )]]);

        let out = registry.apply(vec![text("wow Pog nice")]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], text("wow "));
        assert!(matches!(&out[1], ChatToken::Emote { emote } if emote.name == "Pog"));
        assert_eq!(out[2], text(" nice"));
    }

    #[test]
    fn a_substring_is_not_an_emote() {
        let registry = EmoteRegistry::build(vec![vec![emote(
            "Pog",
            ProviderId::SevenTv,
            EmoteScope::Channel,
        )]]);
        let out = registry.apply(vec![text("Pogchamp Pogging")]);
        assert_eq!(out, vec![text("Pogchamp Pogging")], "no whole-word match");
    }

    #[test]
    fn matching_is_case_sensitive_as_twitch_emote_names_are() {
        let registry = EmoteRegistry::build(vec![vec![emote(
            "Pog",
            ProviderId::SevenTv,
            EmoteScope::Channel,
        )]]);
        assert_eq!(registry.apply(vec![text("pog POG")]), vec![text("pog POG")]);
    }

    /// Link and emote tokens are already authoritative; re-scanning them would
    /// let a URL containing an emote name be torn apart.
    #[test]
    fn only_text_tokens_are_rescanned() {
        let registry = EmoteRegistry::build(vec![vec![emote(
            "Pog",
            ProviderId::SevenTv,
            EmoteScope::Channel,
        )]]);

        let link = ChatToken::Link {
            url: Url::parse("https://example.com/Pog").unwrap(),
            display: "https://example.com/Pog".into(),
        };
        let twitch = ChatToken::Emote {
            emote: Box::new(emote("Kappa", ProviderId::Twitch, EmoteScope::Global)),
        };

        let out = registry.apply(vec![link.clone(), twitch.clone()]);
        assert_eq!(out, vec![link, twitch]);
    }

    #[test]
    fn an_empty_registry_leaves_tokens_untouched() {
        let registry = EmoteRegistry::default();
        let tokens = vec![text("Pog nice")];
        assert_eq!(registry.apply(tokens.clone()), tokens);
    }

    #[test]
    fn consecutive_emotes_keep_their_spacing() {
        let registry = EmoteRegistry::build(vec![vec![
            emote("Pog", ProviderId::SevenTv, EmoteScope::Channel),
            emote("Sadge", ProviderId::SevenTv, EmoteScope::Channel),
        ]]);

        let out = registry.apply(vec![text("Pog Sadge")]);
        assert_eq!(out.len(), 3);
        assert!(matches!(&out[0], ChatToken::Emote { emote } if emote.name == "Pog"));
        assert_eq!(out[1], text(" "));
        assert!(matches!(&out[2], ChatToken::Emote { emote } if emote.name == "Sadge"));
    }
}

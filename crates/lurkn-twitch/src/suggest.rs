//! Suggested channels, built from the reader's own follows.
//!
//! Twitch's personalised recommendations live on an internal API that third
//! parties are not meant to use, so these are made here from what the public
//! API does offer: the categories the reader's follows stream in, and the
//! languages they stream in. Neither is a setting — both are read off the
//! follow list, so someone who watches English and German gets both without
//! saying so, and nobody has a language filter to get wrong.
//!
//! Pure: the follow list goes in, the query to make comes out. The request and
//! the answer are the caller's business.

use std::collections::HashMap;

use lurkn_core::FollowedChannel;

/// How many categories to ask about.
///
/// Get Streams takes up to 100 `game_id`s, but three is where "what this person
/// watches" stops and "everything they ever looked at once" starts.
pub const TOP_CATEGORIES: usize = 3;

/// How many languages to ask for, which likewise stops at the ones that matter.
pub const TOP_LANGUAGES: usize = 3;

/// A category worth suggesting from, and how many follows stream in it — the
/// "because" shown beside a suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Basis {
    pub id: String,
    pub name: String,
    pub follows: usize,
}

/// The categories the reader's follows stream in, most common first.
///
/// Every follow counts, live or not: an offline channel still carries the
/// category it last streamed, and at three in the morning the handful who are
/// live would be a poor sample of what someone actually watches.
pub fn categories(follows: &[FollowedChannel]) -> Vec<Basis> {
    let mut counts: HashMap<String, (String, usize)> = HashMap::new();

    for entry in follows {
        let Some(category) = &entry.channel.category else {
            continue;
        };
        if category.id.is_empty() {
            continue;
        }
        counts
            .entry(category.id.clone())
            .and_modify(|(_, n)| *n += 1)
            .or_insert((category.name.clone(), 1));
    }

    let mut ranked: Vec<Basis> = counts
        .into_iter()
        .map(|(id, (name, follows))| Basis { id, name, follows })
        .collect();

    // Most followed first; ties broken by name so the order is stable between
    // calls rather than following the hash map.
    ranked.sort_by(|a, b| b.follows.cmp(&a.follows).then_with(|| a.name.cmp(&b.name)));

    // A category one follow streams in once is noise, not a taste — unless it
    // is all there is to go on.
    if ranked.first().is_some_and(|top| top.follows > 1) {
        ranked.retain(|b| b.follows > 1);
    }

    ranked.truncate(TOP_CATEGORIES);
    ranked
}

/// The languages the reader's follows stream in, most common first.
pub fn languages(follows: &[FollowedChannel]) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for entry in follows {
        if let Some(language) = &entry.channel.language {
            let language = language.trim().to_lowercase();
            // "other" is Twitch's own catch-all; asking for it returns noise.
            if !language.is_empty() && language != "other" {
                *counts.entry(language).or_default() += 1;
            }
        }
    }

    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(TOP_LANGUAGES);
    ranked.into_iter().map(|(language, _)| language).collect()
}

#[cfg(test)]
mod tests {
    use lurkn_core::{Category, Login, TwitchChannel, User, UserId};

    use super::*;

    fn follow(id: &str, category: Option<(&str, &str)>, language: Option<&str>) -> FollowedChannel {
        FollowedChannel {
            channel: TwitchChannel {
                user: User {
                    id: UserId(id.into()),
                    login: Login::parse(&format!("chan{id}")).unwrap(),
                    display_name: format!("Chan{id}"),
                    avatar_url: None,
                },
                title: None,
                category: category.map(|(id, name)| Category {
                    id: id.into(),
                    name: name.into(),
                    box_art_url: None,
                }),
                language: language.map(str::to_owned),
                followed_at: None,
            },
            stream: None,
        }
    }

    const WOW: (&str, &str) = ("18122", "World of Warcraft");
    const CHAT: (&str, &str) = ("509658", "Just Chatting");
    const POE: (&str, &str) = ("29307", "Path of Exile");
    const CHESS: (&str, &str) = ("743", "Chess");

    #[test]
    fn nothing_to_go_on_suggests_nothing() {
        assert!(categories(&[]).is_empty());
        assert!(languages(&[]).is_empty());
        assert!(categories(&[follow("1", None, None)]).is_empty());
    }

    #[test]
    fn the_most_followed_categories_come_first() {
        let follows = [
            follow("1", Some(WOW), None),
            follow("2", Some(WOW), None),
            follow("3", Some(WOW), None),
            follow("4", Some(CHAT), None),
            follow("5", Some(CHAT), None),
        ];

        let picked = categories(&follows);
        assert_eq!(picked[0].name, "World of Warcraft");
        assert_eq!(picked[0].follows, 3, "the count is the reason shown");
        assert_eq!(picked[1].name, "Just Chatting");
    }

    /// One channel that tried a game once is not a taste.
    #[test]
    fn a_one_off_category_is_dropped_when_there_is_something_better() {
        let follows = [
            follow("1", Some(WOW), None),
            follow("2", Some(WOW), None),
            follow("3", Some(CHESS), None),
        ];

        let picked: Vec<_> = categories(&follows).into_iter().map(|b| b.name).collect();
        assert_eq!(picked, ["World of Warcraft"]);
    }

    /// With only one-offs to go on, they are still better than nothing.
    #[test]
    fn one_offs_are_kept_when_they_are_all_there_is() {
        let follows = [follow("1", Some(WOW), None), follow("2", Some(POE), None)];
        assert_eq!(categories(&follows).len(), 2);
    }

    #[test]
    fn at_most_three_categories_are_asked_about() {
        let mut follows = Vec::new();
        for (i, category) in [WOW, CHAT, POE, CHESS].into_iter().enumerate() {
            for j in 0..3 {
                follows.push(follow(&format!("{i}{j}"), Some(category), None));
            }
        }
        assert_eq!(categories(&follows).len(), TOP_CATEGORIES);
    }

    /// Two languages, both kept — the case the filter setting would have got
    /// wrong for someone who watches in English and German.
    #[test]
    fn every_language_the_follows_use_is_kept() {
        let follows = [
            follow("1", None, Some("de")),
            follow("2", None, Some("de")),
            follow("3", None, Some("en")),
        ];
        assert_eq!(languages(&follows), ["de", "en"]);
    }

    #[test]
    fn twitch_s_catch_all_language_is_ignored() {
        let follows = [
            follow("1", None, Some("other")),
            follow("2", None, Some("EN")),
        ];
        assert_eq!(languages(&follows), ["en"]);
    }

    /// Ties must not reorder between calls, or the list would shuffle.
    #[test]
    fn ties_are_broken_the_same_way_every_time() {
        let follows = [follow("1", Some(POE), None), follow("2", Some(CHESS), None)];
        let first: Vec<_> = categories(&follows).into_iter().map(|b| b.name).collect();
        for _ in 0..10 {
            let again: Vec<_> = categories(&follows).into_iter().map(|b| b.name).collect();
            assert_eq!(again, first);
        }
    }
}

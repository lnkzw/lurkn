use url::Url;

pub const HELIX_BASE: &str = "https://api.twitch.tv/helix/";
pub const ID_BASE: &str = "https://id.twitch.tv/";

/// Build a Helix URL from a relative path and query pairs.
fn helix(path: &str, query: &[(&str, &str)]) -> Url {
    let mut url = Url::parse(HELIX_BASE)
        .and_then(|base| base.join(path))
        .unwrap_or_else(|_| unreachable!("HELIX_BASE joins with a static path"));
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in query {
            pairs.append_pair(k, v);
        }
    }
    url
}

/// `Get Followed Streams` — returns only channels that are currently live, so no
/// separate id-batching step is needed. See `docs/architecture.md` §4.3.
pub fn followed_streams(user_id: &str, first: u32) -> Url {
    helix(
        "streams/followed",
        &[("user_id", user_id), ("first", &first.to_string())],
    )
}

/// `Get Followed Channels` — the full follow list, live or not.
pub fn followed_channels(user_id: &str, first: u32) -> Url {
    helix(
        "channels/followed",
        &[("user_id", user_id), ("first", &first.to_string())],
    )
}

pub fn followed_streams_page(user_id: &str, first: u32, after: Option<&str>) -> Url {
    paged(followed_streams(user_id, first), after)
}

pub fn followed_channels_page(user_id: &str, first: u32, after: Option<&str>) -> Url {
    paged(followed_channels(user_id, first), after)
}

/// `Get Users` — up to 100 ids per call. Used for avatars and display names.
pub fn users_by_id(ids: &[String]) -> Url {
    let pairs: Vec<(&str, &str)> = ids.iter().map(|id| ("id", id.as_str())).collect();
    helix("users", &pairs)
}

/// `Get Users` by login, for looking someone up by the name you know them by.
/// No scope required, same as the by-id form.
pub fn users_by_login(logins: &[String]) -> Url {
    let pairs: Vec<(&str, &str)> = logins
        .iter()
        .map(|login| ("login", login.as_str()))
        .collect();
    helix("users", &pairs)
}

/// `Search Channels`: channels whose name matches, live ones first. No scope.
///
/// Not personalised — Twitch keeps its recommendations on an internal API
/// that third parties are not meant to use — so this is lookup, not discovery.
pub fn search_channels(query: &str, first: u32) -> Url {
    let first = first.to_string();
    helix(
        "search/channels",
        &[("query", query), ("first", first.as_str())],
    )
}

/// `GET helix/moderation/channels` — the channels this user moderates.
pub fn moderated_channels(user_id: &str, first: u32) -> Url {
    let first = first.to_string();
    helix(
        "moderation/channels",
        &[("user_id", user_id), ("first", first.as_str())],
    )
}

/// `POST`/`DELETE helix/moderation/bans`.
///
/// `moderator_id` is required on every moderation call and must be the user the
/// token belongs to; Twitch rejects the request outright without it.
pub fn bans(broadcaster_id: &str, moderator_id: &str) -> Url {
    helix(
        "moderation/bans",
        &[
            ("broadcaster_id", broadcaster_id),
            ("moderator_id", moderator_id),
        ],
    )
}

/// `DELETE helix/moderation/bans` for one user.
pub fn unban(broadcaster_id: &str, moderator_id: &str, user_id: &str) -> Url {
    helix(
        "moderation/bans",
        &[
            ("broadcaster_id", broadcaster_id),
            ("moderator_id", moderator_id),
            ("user_id", user_id),
        ],
    )
}

/// `DELETE helix/moderation/chat`. Without `message_id` it clears the room.
pub fn delete_chat(broadcaster_id: &str, moderator_id: &str, message_id: Option<&str>) -> Url {
    let mut pairs: Vec<(&str, &str)> = vec![
        ("broadcaster_id", broadcaster_id),
        ("moderator_id", moderator_id),
    ];
    if let Some(id) = message_id {
        pairs.push(("message_id", id));
    }
    helix("moderation/chat", &pairs)
}

/// `PATCH helix/chat/settings` — slow mode, followers-only and friends.
///
/// Named apart from [`chat_settings`], which reads them: the write needs the
/// acting moderator and the read does not.
pub fn update_chat_settings(broadcaster_id: &str, moderator_id: &str) -> Url {
    helix(
        "chat/settings",
        &[
            ("broadcaster_id", broadcaster_id),
            ("moderator_id", moderator_id),
        ],
    )
}

/// `POST helix/chat/announcements`.
pub fn announcements(broadcaster_id: &str, moderator_id: &str) -> Url {
    helix(
        "chat/announcements",
        &[
            ("broadcaster_id", broadcaster_id),
            ("moderator_id", moderator_id),
        ],
    )
}

/// `Get Streams` for particular channels: whether they are live, and how.
pub fn streams_by_user(ids: &[String]) -> Url {
    let pairs: Vec<(&str, &str)> = ids.iter().map(|id| ("user_id", id.as_str())).collect();
    helix("streams", &pairs)
}

/// `Get Streams` in some categories and languages, busiest first. No scope.
///
/// Both parameters repeat, so every category and every language goes in one
/// request rather than one per pair.
pub fn streams_in(game_ids: &[String], languages: &[String], first: u32) -> Url {
    let first = first.to_string();
    let mut pairs: Vec<(&str, &str)> = vec![("first", first.as_str())];
    pairs.extend(game_ids.iter().map(|id| ("game_id", id.as_str())));
    pairs.extend(languages.iter().map(|l| ("language", l.as_str())));
    helix("streams", &pairs)
}

/// `Get Channels` — up to 100 ids per call. Title and category for offline channels.
pub fn channels_by_id(ids: &[String]) -> Url {
    let pairs: Vec<(&str, &str)> = ids
        .iter()
        .map(|id| ("broadcaster_id", id.as_str()))
        .collect();
    helix("channels", &pairs)
}

/// The authenticated user, identified by the token alone.
pub fn current_user() -> Url {
    helix("users", &[])
}

fn paged(mut url: Url, after: Option<&str>) -> Url {
    if let Some(cursor) = after {
        url.query_pairs_mut().append_pair("after", cursor);
    }
    url
}

/// `Get Global Chat Badges` — no scope required.
pub fn global_badges() -> Url {
    helix("chat/badges/global", &[])
}

/// `Get Channel Chat Badges` — no scope required.
pub fn channel_badges(broadcaster_id: &str) -> Url {
    helix("chat/badges", &[("broadcaster_id", broadcaster_id)])
}

/// `Get Chat Settings` — the room state at join time. EventSub only reports
/// changes, so this fills in what is already true.
pub fn chat_settings(broadcaster_id: &str) -> Url {
    helix("chat/settings", &[("broadcaster_id", broadcaster_id)])
}

pub fn eventsub_subscriptions() -> Url {
    helix("eventsub/subscriptions", &[])
}

pub fn send_chat_message() -> Url {
    helix("chat/messages", &[])
}

/// `POST helix/whispers`. Both ids are query parameters, not body fields, and
/// `from_user_id` must be the authorising user.
pub fn send_whisper(from_user_id: &str, to_user_id: &str) -> Url {
    helix(
        "whispers",
        &[("from_user_id", from_user_id), ("to_user_id", to_user_id)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_ask_for_every_category_and_language_at_once() {
        let url = streams_in(
            &["18122".to_owned(), "509658".to_owned()],
            &["de".to_owned(), "en".to_owned()],
            40,
        );
        assert_eq!(
            url.as_str(),
            "https://api.twitch.tv/helix/streams?first=40&game_id=18122&game_id=509658&language=de&language=en"
        );
    }

    /// Every moderation call carries the acting moderator, and Twitch refuses
    /// the request without it.
    #[test]
    fn moderation_urls_name_the_acting_moderator() {
        assert_eq!(
            bans("111", "222").as_str(),
            "https://api.twitch.tv/helix/moderation/bans?broadcaster_id=111&moderator_id=222"
        );
        assert_eq!(
            unban("111", "222", "333").as_str(),
            "https://api.twitch.tv/helix/moderation/bans?broadcaster_id=111&moderator_id=222&user_id=333"
        );
        assert!(update_chat_settings("111", "222")
            .as_str()
            .contains("moderator_id=222"));
        assert!(announcements("111", "222")
            .as_str()
            .contains("moderator_id=222"));
    }

    /// One message, or the whole room when none is named.
    #[test]
    fn deleting_chat_names_a_message_only_when_there_is_one() {
        assert!(delete_chat("111", "222", Some("M1"))
            .as_str()
            .ends_with("&message_id=M1"));
        assert!(!delete_chat("111", "222", None)
            .as_str()
            .contains("message_id"));
    }

    #[test]
    fn a_search_is_encoded_and_bounded() {
        assert_eq!(
            search_channels("sieg linde", 10).as_str(),
            "https://api.twitch.tv/helix/search/channels?query=sieg+linde&first=10"
        );
    }

    #[test]
    fn a_user_can_be_looked_up_by_login() {
        assert_eq!(
            users_by_login(&["forsen".to_owned()]).as_str(),
            "https://api.twitch.tv/helix/users?login=forsen"
        );
    }

    #[test]
    fn a_whisper_carries_both_ids_in_the_query() {
        let url = send_whisper("12826", "141981764");
        assert_eq!(
            url.as_str(),
            "https://api.twitch.tv/helix/whispers?from_user_id=12826&to_user_id=141981764"
        );
    }

    #[test]
    fn builds_followed_streams_url() {
        assert_eq!(
            followed_streams("12345", 100).as_str(),
            "https://api.twitch.tv/helix/streams/followed?user_id=12345&first=100"
        );
    }

    #[test]
    fn builds_followed_channels_url() {
        assert_eq!(
            followed_channels("12345", 100).as_str(),
            "https://api.twitch.tv/helix/channels/followed?user_id=12345&first=100"
        );
    }

    #[test]
    fn chat_and_eventsub_endpoints_take_no_query() {
        assert_eq!(
            send_chat_message().as_str(),
            "https://api.twitch.tv/helix/chat/messages"
        );
        assert_eq!(
            eventsub_subscriptions().as_str(),
            "https://api.twitch.tv/helix/eventsub/subscriptions"
        );
    }

    #[test]
    fn batch_endpoints_repeat_the_key_once_per_id() {
        let ids = vec!["1".to_owned(), "2".to_owned(), "3".to_owned()];
        assert_eq!(
            users_by_id(&ids).as_str(),
            "https://api.twitch.tv/helix/users?id=1&id=2&id=3"
        );
        assert_eq!(
            channels_by_id(&ids).as_str(),
            "https://api.twitch.tv/helix/channels?broadcaster_id=1&broadcaster_id=2&broadcaster_id=3"
        );
    }

    #[test]
    fn current_user_takes_no_parameters() {
        assert_eq!(current_user().as_str(), "https://api.twitch.tv/helix/users");
    }

    #[test]
    fn a_cursor_is_appended_only_when_present() {
        assert_eq!(
            followed_channels_page("1", 100, None).as_str(),
            "https://api.twitch.tv/helix/channels/followed?user_id=1&first=100"
        );
        assert_eq!(
            followed_channels_page("1", 100, Some("abc")).as_str(),
            "https://api.twitch.tv/helix/channels/followed?user_id=1&first=100&after=abc"
        );
    }

    #[test]
    fn a_hostile_cursor_cannot_break_out_of_the_query() {
        let url = followed_streams_page("1", 100, Some("x&user_id=999"));
        assert!(url.as_str().contains("after=x%26user_id%3D999"), "{url}");
        assert_eq!(
            url.query_pairs().filter(|(k, _)| k == "user_id").count(),
            1,
            "user_id must not be overridable via the cursor"
        );
    }

    #[test]
    fn query_values_are_percent_encoded() {
        // A hostile id can never break out of the query string.
        let url = followed_streams("1&first=9999#x", 100);
        assert!(
            url.as_str().contains("user_id=1%26first%3D9999%23x"),
            "{url}"
        );
        assert_eq!(url.path(), "/helix/streams/followed");
    }
}

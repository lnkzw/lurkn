//! Wire shapes as Twitch sends them, kept separate from the domain model so a
//! Helix field rename never leaks past this module.

use lurkn_core::{Category, ChannelHit, Login, Stream, TwitchChannel, User, UserId, UserProfile};
use serde::Deserialize;
use time::OffsetDateTime;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct UserDto {
    pub id: String,
    pub login: String,
    pub display_name: String,
    #[serde(default)]
    pub profile_image_url: String,
    // The follow list ignores these three; the user card is what wants them.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub broadcaster_type: String,
}

impl UserDto {
    pub fn into_user(self) -> Option<User> {
        Some(User {
            id: UserId(self.id),
            // A login Twitch itself cannot produce validly is one we skip rather
            // than guess at.
            login: Login::parse(&self.login).ok()?,
            display_name: self.display_name,
            avatar_url: Url::parse(&self.profile_image_url).ok(),
        })
    }

    pub fn into_profile(self) -> Option<UserProfile> {
        let description = self.description.clone();
        let broadcaster_type = self.broadcaster_type.clone();
        let created_at = self.created_at.clone().and_then(|t| {
            OffsetDateTime::parse(&t, &time::format_description::well_known::Rfc3339).ok()
        });

        Some(UserProfile {
            user: self.into_user()?,
            description,
            created_at,
            broadcaster_type,
        })
    }
}

/// `GET helix/channels/followed`
#[derive(Debug, Deserialize)]
pub struct FollowedChannelDto {
    pub broadcaster_id: String,
    pub broadcaster_login: String,
    pub broadcaster_name: String,
    #[serde(default)]
    pub followed_at: Option<String>,
}

impl FollowedChannelDto {
    pub fn into_channel(self) -> Option<TwitchChannel> {
        Some(TwitchChannel {
            user: User {
                id: UserId(self.broadcaster_id),
                login: Login::parse(&self.broadcaster_login).ok()?,
                display_name: self.broadcaster_name,
                avatar_url: None, // filled in by a separate Get Users call
            },
            title: None,
            category: None,
            language: None,
            followed_at: self.followed_at.and_then(|t| {
                OffsetDateTime::parse(&t, &time::format_description::well_known::Rfc3339).ok()
            }),
        })
    }
}

/// `GET helix/streams/followed`
#[derive(Debug, Deserialize)]
pub struct StreamDto {
    pub user_id: String,
    pub user_login: String,
    pub user_name: String,
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub game_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub viewer_count: u32,
    pub started_at: String,
    #[serde(default)]
    pub thumbnail_url: String,
}

impl StreamDto {
    pub fn into_stream(&self) -> Option<Stream> {
        Some(Stream {
            channel_id: UserId(self.user_id.clone()),
            title: self.title.clone(),
            category: (!self.game_id.is_empty()).then(|| Category {
                id: self.game_id.clone(),
                name: self.game_name.clone(),
                box_art_url: None,
            }),
            viewer_count: self.viewer_count,
            started_at: OffsetDateTime::parse(
                &self.started_at,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()?,
            thumbnail_url_template: (!self.thumbnail_url.is_empty())
                .then(|| self.thumbnail_url.clone()),
        })
    }

    /// A live stream also tells us who the broadcaster is, which saves a lookup
    /// for channels not yet in the cache.
    pub fn as_user(&self) -> Option<User> {
        Some(User {
            id: UserId(self.user_id.clone()),
            login: Login::parse(&self.user_login).ok()?,
            display_name: self.user_name.clone(),
            avatar_url: None,
        })
    }
}

/// `GET helix/channels`
#[derive(Debug, Deserialize)]
pub struct ChannelInfoDto {
    pub broadcaster_id: String,
    #[serde(default)]
    pub broadcaster_language: String,
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub game_name: String,
    #[serde(default)]
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_followed_channel() {
        let dto: FollowedChannelDto = serde_json::from_str(
            r#"{"broadcaster_id":"11111","broadcaster_login":"forsen",
                "broadcaster_name":"Forsen","followed_at":"2020-01-02T03:04:05Z"}"#,
        )
        .unwrap();
        let channel = dto.into_channel().unwrap();
        assert_eq!(channel.user.id.as_str(), "11111");
        assert_eq!(channel.user.login.as_str(), "forsen");
        assert_eq!(channel.user.display_name, "Forsen");
        assert!(channel.followed_at.is_some());
    }

    #[test]
    fn a_missing_followed_at_is_tolerated() {
        let dto: FollowedChannelDto = serde_json::from_str(
            r#"{"broadcaster_id":"1","broadcaster_login":"foo","broadcaster_name":"Foo"}"#,
        )
        .unwrap();
        assert_eq!(dto.into_channel().unwrap().followed_at, None);
    }

    #[test]
    fn a_broadcaster_with_an_unusable_login_is_skipped_not_guessed_at() {
        let dto: FollowedChannelDto = serde_json::from_str(
            r#"{"broadcaster_id":"1","broadcaster_login":"bad login!","broadcaster_name":"X"}"#,
        )
        .unwrap();
        assert!(dto.into_channel().is_none());
    }

    #[test]
    fn parses_a_live_stream() {
        let dto: StreamDto = serde_json::from_str(
            r#"{"id":"1","user_id":"11111","user_login":"forsen","user_name":"Forsen",
                "game_id":"509658","game_name":"Just Chatting","type":"live",
                "title":"hello","viewer_count":1234,"started_at":"2026-09-20T12:00:00Z",
                "language":"en",
                "thumbnail_url":"https://static-cdn.jtvnw.net/x-{width}x{height}.jpg"}"#,
        )
        .unwrap();
        let stream = dto.into_stream().unwrap();
        assert_eq!(stream.viewer_count, 1234);
        assert_eq!(stream.category.as_ref().unwrap().name, "Just Chatting");
        assert_eq!(
            stream.thumbnail(440, 248).unwrap().as_str(),
            "https://static-cdn.jtvnw.net/x-440x248.jpg"
        );
        assert_eq!(dto.as_user().unwrap().login.as_str(), "forsen");
    }

    #[test]
    fn a_stream_with_no_category_has_none() {
        let dto: StreamDto = serde_json::from_str(
            r#"{"user_id":"1","user_login":"foo","user_name":"Foo","title":"t",
                "started_at":"2026-09-20T12:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(dto.into_stream().unwrap().category, None);
    }

    /// Untrusted fields must survive parsing verbatim — escaping is the renderer's
    /// job, and silently mangling them here would hide that.
    #[test]
    fn hostile_titles_are_preserved_verbatim() {
        let dto: StreamDto = serde_json::from_str(
            r#"{"user_id":"1","user_login":"foo","user_name":"Foo",
                "title":"<script>alert(1)</script>","started_at":"2026-09-20T12:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(
            dto.into_stream().unwrap().title,
            "<script>alert(1)</script>"
        );
    }
}

/// `GET helix/search/channels`
#[derive(Debug, Deserialize)]
pub struct SearchChannelDto {
    pub id: String,
    pub broadcaster_login: String,
    pub display_name: String,
    #[serde(default)]
    pub thumbnail_url: String,
    #[serde(default)]
    pub is_live: bool,
    #[serde(default)]
    pub game_name: String,
    #[serde(default)]
    pub title: String,
}

impl SearchChannelDto {
    pub fn into_hit(self) -> Option<ChannelHit> {
        let non_empty = |s: String| if s.trim().is_empty() { None } else { Some(s) };

        Some(ChannelHit {
            id: UserId(self.id),
            login: Login::parse(&self.broadcaster_login).ok()?,
            display_name: self.display_name,
            // The search result's "thumbnail" is the profile image, not a stream
            // preview, despite the name.
            avatar_url: Url::parse(&self.thumbnail_url).ok(),
            live: self.is_live,
            category: non_empty(self.game_name),
            title: non_empty(self.title),
            // Search Channels does not report an audience.
            viewers: None,
        })
    }
}

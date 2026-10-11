use serde::{Deserialize, Serialize};
use specta::Type;
use time::OffsetDateTime;
use url::Url;

use crate::ids::{Login, UserId};

/// A Twitch account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: UserId,
    pub login: Login,
    /// May differ from `login` only in case, or be a CJK/Cyrillic alias.
    pub display_name: String,
    pub avatar_url: Option<Url>,
}

/// What `Get Users` knows about an account, for the card behind a chat name.
///
/// Separate from [`User`] rather than optional fields on it: the follow list
/// carries thousands of users and none of this, while a card is one account the
/// reader actually asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user: User,
    /// UNTRUSTED: free text the account holder writes. Never rendered as markup.
    pub description: String,
    /// When the account was made. `None` when Twitch sends something unparseable.
    #[serde(with = "time::serde::rfc3339::option")]
    #[specta(type = Option<String>)]
    pub created_at: Option<OffsetDateTime>,
    /// `partner`, `affiliate`, or empty for an ordinary account.
    pub broadcaster_type: String,
}

/// A channel found by name, which may or may not be one the reader follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHit {
    pub id: UserId,
    pub login: Login,
    /// UNTRUSTED.
    pub display_name: String,
    pub avatar_url: Option<Url>,
    pub live: bool,
    /// UNTRUSTED: what they are streaming, or last streamed.
    pub category: Option<String>,
    /// UNTRUSTED.
    pub title: Option<String>,
    /// Viewers right now, when the source knows. Search results do not.
    ///
    /// Defaulted so recent-channel lists stored before this field existed still
    /// load rather than being thrown away.
    #[serde(default)]
    pub viewers: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    /// UNTRUSTED: broadcaster-influenced. Never rendered as markup.
    pub name: String,
    pub box_art_url: Option<String>,
}

/// A broadcaster, independent of whether they are live.
///
/// Named `TwitchChannel` rather than `Channel` because the latter collides with
/// the Tauri Specta runtime's own type when exported to TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TwitchChannel {
    pub user: User,
    /// UNTRUSTED: free text set by the broadcaster.
    pub title: Option<String>,
    pub category: Option<Category>,
    pub language: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[specta(type = Option<String>)]
    pub followed_at: Option<OffsetDateTime>,
}

/// Exists only while a channel is live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub channel_id: UserId,
    /// UNTRUSTED.
    pub title: String,
    pub category: Option<Category>,
    pub viewer_count: u32,
    #[serde(with = "time::serde::rfc3339")]
    #[specta(type = String)]
    pub started_at: OffsetDateTime,
    /// Contains `{width}` and `{height}` placeholders. Twitch caches these, so
    /// they lag the live picture by minutes.
    pub thumbnail_url_template: Option<String>,
}

impl Stream {
    /// Fill the thumbnail placeholders, validating the host first.
    pub fn thumbnail(&self, width: u32, height: u32) -> Option<Url> {
        let filled = self
            .thumbnail_url_template
            .as_ref()?
            .replace("{width}", &width.to_string())
            .replace("{height}", &height.to_string());
        let url = Url::parse(&filled).ok()?;
        (url.scheme() == "https" && url.host_str() == Some("static-cdn.jtvnw.net")).then_some(url)
    }

    pub fn uptime(&self, now: OffsetDateTime) -> time::Duration {
        now - self.started_at
    }
}

/// A followed channel joined with its live state — what the sidebar renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FollowedChannel {
    pub channel: TwitchChannel,
    /// `None` means offline.
    pub stream: Option<Stream>,
}

impl FollowedChannel {
    pub fn is_live(&self) -> bool {
        self.stream.is_some()
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    fn stream(template: Option<&str>) -> Stream {
        Stream {
            channel_id: UserId("1".into()),
            title: "title".into(),
            category: None,
            viewer_count: 10,
            started_at: datetime!(2026-09-20 12:00:00 UTC),
            thumbnail_url_template: template.map(str::to_owned),
        }
    }

    #[test]
    fn thumbnail_fills_the_placeholders() {
        let s = stream(Some(
            "https://static-cdn.jtvnw.net/previews-ttv/live_user_foo-{width}x{height}.jpg",
        ));
        assert_eq!(
            s.thumbnail(440, 248).unwrap().as_str(),
            "https://static-cdn.jtvnw.net/previews-ttv/live_user_foo-440x248.jpg"
        );
    }

    #[test]
    fn thumbnail_rejects_a_foreign_host() {
        assert_eq!(
            stream(Some("https://evil.example/{width}x{height}.jpg")).thumbnail(440, 248),
            None
        );
        assert_eq!(
            stream(Some("http://static-cdn.jtvnw.net/{width}x{height}.jpg")).thumbnail(440, 248),
            None,
            "plaintext must be refused"
        );
    }

    #[test]
    fn thumbnail_is_none_without_a_template() {
        assert_eq!(stream(None).thumbnail(440, 248), None);
    }

    #[test]
    fn uptime_counts_from_the_start() {
        assert_eq!(
            stream(None).uptime(datetime!(2026-09-20 14:30:00 UTC)),
            time::Duration::minutes(150)
        );
    }

    #[test]
    fn live_state_comes_from_the_stream_being_present() {
        let channel = TwitchChannel {
            user: User {
                id: UserId("1".into()),
                login: Login::parse("foo").unwrap(),
                display_name: "Foo".into(),
                avatar_url: None,
            },
            title: None,
            category: None,
            language: None,
            followed_at: None,
        };
        assert!(!FollowedChannel {
            channel: channel.clone(),
            stream: None
        }
        .is_live());
        assert!(FollowedChannel {
            channel,
            stream: Some(stream(None))
        }
        .is_live());
    }
}

//! Chat badge sets. No scope is required for either endpoint.

use std::collections::HashMap;

use lurkn_core::Badge;
use serde::Deserialize;
use url::Url;

use crate::client::{HelixClient, Page};
use crate::endpoints;
use crate::error::Result;

#[derive(Debug, Deserialize)]
pub struct BadgeSetDto {
    pub set_id: String,
    #[serde(default)]
    pub versions: Vec<BadgeVersionDto>,
}

#[derive(Debug, Deserialize)]
pub struct BadgeVersionDto {
    pub id: String,
    #[serde(default)]
    pub image_url_1x: String,
    #[serde(default)]
    pub image_url_2x: String,
    #[serde(default)]
    pub image_url_4x: String,
    #[serde(default)]
    pub title: String,
}

/// `(set_id, version)` -> resolved badge.
pub type BadgeMap = HashMap<(String, String), Badge>;

pub fn to_map(sets: Vec<BadgeSetDto>) -> BadgeMap {
    let mut map = HashMap::new();

    for set in sets {
        for version in set.versions {
            let Some(url_1x) = cdn_url(&version.image_url_1x) else {
                continue;
            };
            map.insert(
                (set.set_id.clone(), version.id.clone()),
                Badge {
                    set_id: set.set_id.clone(),
                    version: version.id,
                    title: version.title,
                    // Per-message detail, so the badge set has none to give.
                    info: String::new(),
                    url_1x: Some(url_1x),
                    url_2x: cdn_url(&version.image_url_2x),
                    url_4x: cdn_url(&version.image_url_4x),
                },
            );
        }
    }

    map
}

/// Badge images must come from Twitch's CDN, same rule as emotes (§6.3).
fn cdn_url(raw: &str) -> Option<Url> {
    let url = Url::parse(raw).ok()?;
    (url.scheme() == "https" && url.host_str() == Some("static-cdn.jtvnw.net")).then_some(url)
}

/// Fill in a message's badge references from the resolved sets.
///
/// Channel badges win over global ones: a channel's subscriber badge is its own.
pub fn resolve(badges: &mut [Badge], channel: &BadgeMap, global: &BadgeMap) {
    for badge in badges {
        let key = (badge.set_id.clone(), badge.version.clone());
        if let Some(found) = channel.get(&key).or_else(|| global.get(&key)) {
            badge.title = found.title.clone();
            badge.url_1x = found.url_1x.clone();
            badge.url_2x = found.url_2x.clone();
            badge.url_4x = found.url_4x.clone();
        }
    }
}

#[derive(Debug, Clone)]
pub struct BadgeService {
    client: HelixClient,
}

impl BadgeService {
    pub fn new(client: HelixClient) -> Self {
        Self { client }
    }

    pub async fn global(&self) -> Result<BadgeMap> {
        let page: Page<BadgeSetDto> = self.client.get(&endpoints::global_badges()).await?;
        Ok(to_map(page.data))
    }

    pub async fn channel(&self, broadcaster_id: &str) -> Result<BadgeMap> {
        let page: Page<BadgeSetDto> = self
            .client
            .get(&endpoints::channel_badges(broadcaster_id))
            .await?;
        Ok(to_map(page.data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sets(json: &str) -> Vec<BadgeSetDto> {
        serde_json::from_str(json).unwrap()
    }

    const GLOBAL: &str = r#"[
      {"set_id":"moderator","versions":[
        {"id":"1","title":"Moderator",
         "image_url_1x":"https://static-cdn.jtvnw.net/badges/v1/mod/1",
         "image_url_2x":"https://static-cdn.jtvnw.net/badges/v1/mod/2",
         "image_url_4x":"https://static-cdn.jtvnw.net/badges/v1/mod/3"}]},
      {"set_id":"subscriber","versions":[
        {"id":"0","title":"Subscriber",
         "image_url_1x":"https://static-cdn.jtvnw.net/badges/v1/subglobal/1"}]}
    ]"#;

    #[test]
    fn builds_a_lookup_keyed_by_set_and_version() {
        let map = to_map(sets(GLOBAL));
        let mod_badge = map.get(&("moderator".into(), "1".into())).unwrap();
        assert_eq!(mod_badge.title, "Moderator");
        assert_eq!(
            mod_badge.url_1x.as_ref().unwrap().as_str(),
            "https://static-cdn.jtvnw.net/badges/v1/mod/1"
        );
        assert!(mod_badge.is_resolved());
    }

    #[test]
    fn a_badge_image_off_the_twitch_cdn_is_dropped() {
        let map = to_map(sets(
            r#"[{"set_id":"x","versions":[
                 {"id":"1","title":"X","image_url_1x":"https://evil.example/b.png"}]}]"#,
        ));
        assert!(map.is_empty());
    }

    /// A channel's own subscriber badge must beat the global placeholder.
    #[test]
    fn channel_badges_win_over_global_ones() {
        let global = to_map(sets(GLOBAL));
        let channel = to_map(sets(
            r#"[{"set_id":"subscriber","versions":[
                 {"id":"0","title":"Channel Subscriber",
                  "image_url_1x":"https://static-cdn.jtvnw.net/badges/v1/subchannel/1"}]}]"#,
        ));

        let mut badges = vec![
            Badge::unresolved("subscriber".into(), "0".into(), String::new()),
            Badge::unresolved("moderator".into(), "1".into(), String::new()),
        ];
        resolve(&mut badges, &channel, &global);

        assert_eq!(badges[0].title, "Channel Subscriber");
        assert!(badges[0]
            .url_1x
            .as_ref()
            .unwrap()
            .as_str()
            .contains("subchannel"));
        // Not overridden by the channel, so it falls back to global.
        assert_eq!(badges[1].title, "Moderator");
    }

    /// An unknown badge stays unresolved rather than rendering a broken image.
    #[test]
    fn an_unknown_badge_is_left_unresolved() {
        let mut badges = vec![Badge::unresolved(
            "mystery".into(),
            "9".into(),
            String::new(),
        )];
        resolve(&mut badges, &HashMap::new(), &to_map(sets(GLOBAL)));
        assert!(!badges[0].is_resolved());
        assert!(badges[0].title.is_empty());
    }
}

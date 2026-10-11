//! 7TV v3. Shapes verified against live responses on 2026-09-20.

use lurkn_core::{Emote, EmoteScope, ProviderId};
use serde::Deserialize;
use url::Url;

/// `ActiveEmote.flags` bit 0 marks an overlay emote.
const FLAG_ZERO_WIDTH: u32 = 1;
/// The widest format every 7TV emote ships, animated or not.
const PREFERRED_FORMAT: &str = "webp";

#[derive(Debug, Deserialize)]
pub struct UserResponse {
    #[serde(default)]
    pub emote_set: Option<EmoteSet>,
}

#[derive(Debug, Deserialize)]
pub struct EmoteSet {
    #[serde(default)]
    pub emotes: Vec<ActiveEmote>,
}

#[derive(Debug, Deserialize)]
pub struct ActiveEmote {
    pub id: String,
    /// The name *as used in this channel*, which may differ from the emote's own.
    pub name: String,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub data: Option<EmoteData>,
}

#[derive(Debug, Deserialize)]
pub struct EmoteData {
    #[serde(default)]
    pub animated: bool,
    #[serde(default)]
    pub host: Option<Host>,
}

#[derive(Debug, Deserialize)]
pub struct Host {
    /// Protocol-relative, e.g. `//cdn.7tv.app/emote/<id>`.
    pub url: String,
    #[serde(default)]
    pub files: Vec<HostFile>,
}

#[derive(Debug, Deserialize)]
pub struct HostFile {
    pub name: String,
    #[serde(default)]
    pub width: u16,
    #[serde(default)]
    pub height: u16,
    #[serde(default)]
    pub format: String,
}

/// Turn a 7TV emote set into domain emotes, skipping any we cannot build a URL for.
pub fn to_emotes(set: &EmoteSet, scope: EmoteScope) -> Vec<Emote> {
    set.emotes
        .iter()
        .filter_map(|e| to_emote(e, scope.clone()))
        .collect()
}

fn to_emote(active: &ActiveEmote, scope: EmoteScope) -> Option<Emote> {
    let data = active.data.as_ref()?;
    let host = data.host.as_ref()?;

    let pick = |scale: &str| -> Option<Url> {
        let wanted = format!("{scale}.{PREFERRED_FORMAT}");
        host.files
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&wanted))
            .and_then(|f| absolute(&host.url, &f.name))
    };

    let aspect = host
        .files
        .iter()
        .find(|f| {
            f.name
                .eq_ignore_ascii_case(&format!("1x.{PREFERRED_FORMAT}"))
        })
        .filter(|f| f.width > 0 && f.height > 0)
        .map(|f| (f.width, f.height));

    Some(Emote {
        id: active.id.clone(),
        name: active.name.clone(),
        provider: ProviderId::SevenTv,
        scope,
        url_1x: pick("1x")?,
        url_2x: pick("2x"),
        url_3x: pick("3x"),
        animated: data.animated,
        aspect,
        zero_width: active.flags & FLAG_ZERO_WIDTH != 0,
    })
}

/// 7TV returns `//cdn.7tv.app/...`; make it absolute and confirm the host.
fn absolute(host_url: &str, file: &str) -> Option<Url> {
    let base = host_url.strip_prefix("//").unwrap_or(host_url);
    let url = Url::parse(&format!("https://{base}/{file}")).ok()?;
    (url.host_str() == Some("cdn.7tv.app")).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a live `GET https://7tv.io/v3/users/twitch/12875057`.
    const FIXTURE: &str = r#"{
      "emote_set": { "emotes": [
        { "id": "01EZPGBXMR0001DCZS00AD662R", "name": "FeelsRainMan", "flags": 0,
          "data": { "animated": true, "host": {
            "url": "//cdn.7tv.app/emote/01EZPGBXMR0001DCZS00AD662R",
            "files": [
              {"name":"1x.webp","width":32,"height":32,"format":"WEBP"},
              {"name":"2x.webp","width":64,"height":64,"format":"WEBP"},
              {"name":"3x.webp","width":96,"height":96,"format":"WEBP"},
              {"name":"1x.avif","width":32,"height":32,"format":"AVIF"}
            ]}}},
        { "id": "02", "name": "SPEED", "flags": 1,
          "data": { "animated": false, "host": {
            "url": "//cdn.7tv.app/emote/02",
            "files": [{"name":"1x.webp","width":32,"height":32,"format":"WEBP"}]}}},
        { "id": "03", "name": "Broken", "flags": 0, "data": { "animated": false } }
      ]}}"#;

    fn parsed() -> EmoteSet {
        serde_json::from_str::<UserResponse>(FIXTURE)
            .unwrap()
            .emote_set
            .unwrap()
    }

    #[test]
    fn converts_a_channel_set() {
        let emotes = to_emotes(&parsed(), EmoteScope::Channel);
        assert_eq!(emotes.len(), 2, "the entry without a host is skipped");

        let first = &emotes[0];
        assert_eq!(first.name, "FeelsRainMan");
        assert_eq!(first.provider, ProviderId::SevenTv);
        assert_eq!(first.scope, EmoteScope::Channel);
        assert!(first.animated);
        assert_eq!(
            first.url_1x.as_str(),
            "https://cdn.7tv.app/emote/01EZPGBXMR0001DCZS00AD662R/1x.webp"
        );
        assert_eq!(
            first.url_2x.as_ref().unwrap().as_str(),
            "https://cdn.7tv.app/emote/01EZPGBXMR0001DCZS00AD662R/2x.webp"
        );
        assert_eq!(first.aspect, Some((32, 32)));
    }

    /// Overlay emotes stack on the preceding emote; missing the flag renders them
    /// as a broken-looking sequence.
    #[test]
    fn the_zero_width_flag_is_read() {
        let emotes = to_emotes(&parsed(), EmoteScope::Channel);
        assert!(!emotes[0].zero_width);
        assert!(emotes[1].zero_width, "SPEED is an overlay emote");
    }

    #[test]
    fn prefers_webp_over_avif() {
        let emotes = to_emotes(&parsed(), EmoteScope::Channel);
        assert!(emotes[0].url_1x.as_str().ends_with(".webp"));
    }

    #[test]
    fn a_scale_that_does_not_exist_is_none() {
        let emotes = to_emotes(&parsed(), EmoteScope::Channel);
        assert_eq!(emotes[1].url_2x, None);
    }

    /// A hostile host URL must not redirect image loads off the 7TV CDN.
    #[test]
    fn only_the_seventv_cdn_is_accepted() {
        assert!(absolute("//cdn.7tv.app/emote/1", "1x.webp").is_some());
        assert!(absolute("//evil.example/emote/1", "1x.webp").is_none());
        assert!(absolute("//cdn.7tv.app.evil.example/e/1", "1x.webp").is_none());
    }

    #[test]
    fn a_user_with_no_emote_set_yields_nothing() {
        let response: UserResponse = serde_json::from_str(r#"{"emote_set":null}"#).unwrap();
        assert!(response.emote_set.is_none());
    }
}

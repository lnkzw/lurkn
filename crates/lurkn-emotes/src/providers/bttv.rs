//! BetterTTV v3. Shapes verified against live responses on 2026-09-20.

use lurkn_core::{Emote, EmoteScope, ProviderId};
use serde::Deserialize;
use url::Url;

/// BTTV does not flag overlay emotes in its API, so the set is well-known and
/// hardcoded — the same list every other client carries.
const ZERO_WIDTH_CODES: &[&str] = &[
    "SoSnowy",
    "IceCold",
    "SantaHat",
    "TopHat",
    "ReinDeer",
    "CandyCane",
    "cvMask",
    "cvHazmat",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelResponse {
    #[serde(default)]
    pub channel_emotes: Vec<BttvEmote>,
    #[serde(default)]
    pub shared_emotes: Vec<BttvEmote>,
}

impl ChannelResponse {
    pub fn all(&self) -> impl Iterator<Item = &BttvEmote> {
        self.channel_emotes.iter().chain(self.shared_emotes.iter())
    }
}

#[derive(Debug, Deserialize)]
pub struct BttvEmote {
    pub id: String,
    pub code: String,
    #[serde(default)]
    pub animated: bool,
    #[serde(default)]
    pub width: u16,
    #[serde(default)]
    pub height: u16,
}

pub fn to_emotes<'a>(emotes: impl Iterator<Item = &'a BttvEmote>, scope: EmoteScope) -> Vec<Emote> {
    emotes.filter_map(|e| to_emote(e, scope.clone())).collect()
}

fn to_emote(emote: &BttvEmote, scope: EmoteScope) -> Option<Emote> {
    let url = |scale: &str| -> Option<Url> {
        let url = Url::parse(&format!(
            "https://cdn.betterttv.net/emote/{}/{scale}",
            emote.id
        ))
        .ok()?;
        (url.host_str() == Some("cdn.betterttv.net")).then_some(url)
    };

    Some(Emote {
        id: emote.id.clone(),
        name: emote.code.clone(),
        provider: ProviderId::Bttv,
        scope,
        url_1x: url("1x")?,
        url_2x: url("2x"),
        url_3x: url("3x"),
        animated: emote.animated,
        aspect: (emote.width > 0 && emote.height > 0).then_some((emote.width, emote.height)),
        zero_width: ZERO_WIDTH_CODES.contains(&emote.code.as_str()),
    })
}

/// The global endpoint returns a bare array rather than an object.
pub fn parse_global(body: &str) -> Result<Vec<BttvEmote>, serde_json::Error> {
    serde_json::from_str(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a live `GET .../3/cached/users/twitch/12875057`.
    const CHANNEL: &str = r#"{
      "id":"x","bots":[],"avatar":"",
      "channelEmotes":[],
      "sharedEmotes":[
        {"id":"55898e122612142e6aaa935b","code":"PepePls","imageType":"gif",
         "animated":true,"width":21,"height":28},
        {"id":"5f1b0186cf6d2144653d2970","code":"SoSnowy","imageType":"gif",
         "animated":true,"width":28,"height":28}
      ]}"#;

    fn parsed() -> ChannelResponse {
        serde_json::from_str(CHANNEL).unwrap()
    }

    #[test]
    fn converts_channel_and_shared_emotes_together() {
        let response = parsed();
        let emotes = to_emotes(response.all(), EmoteScope::Channel);
        assert_eq!(emotes.len(), 2);

        let pepe = &emotes[0];
        assert_eq!(pepe.name, "PepePls");
        assert_eq!(pepe.provider, ProviderId::Bttv);
        assert!(pepe.animated);
        assert_eq!(pepe.aspect, Some((21, 28)));
        assert_eq!(
            pepe.url_1x.as_str(),
            "https://cdn.betterttv.net/emote/55898e122612142e6aaa935b/1x"
        );
        assert_eq!(
            pepe.url_3x.as_ref().unwrap().as_str(),
            "https://cdn.betterttv.net/emote/55898e122612142e6aaa935b/3x"
        );
    }

    /// BTTV does not report overlay emotes, so the known set is hardcoded.
    #[test]
    fn known_overlay_emotes_are_marked_zero_width() {
        let response = parsed();
        let emotes = to_emotes(response.all(), EmoteScope::Channel);
        assert!(!emotes[0].zero_width, "PepePls is an ordinary emote");
        assert!(emotes[1].zero_width, "SoSnowy stacks on the previous emote");
    }

    #[test]
    fn parses_the_global_array_shape() {
        let global =
            parse_global(r#"[{"id":"1","code":"OMEGALUL","imageType":"png","animated":false}]"#)
                .unwrap();
        let emotes = to_emotes(global.iter(), EmoteScope::Global);
        assert_eq!(emotes[0].name, "OMEGALUL");
        assert_eq!(emotes[0].scope, EmoteScope::Global);
        assert_eq!(emotes[0].aspect, None, "no dimensions reported");
    }

    /// An id from the API cannot be used to point image loads elsewhere.
    #[test]
    fn a_hostile_id_stays_on_the_bttv_cdn() {
        let emote = BttvEmote {
            id: "../../evil.example/x".into(),
            code: "X".into(),
            animated: false,
            width: 0,
            height: 0,
        };
        let converted = to_emote(&emote, EmoteScope::Global).unwrap();
        assert_eq!(converted.url_1x.host_str(), Some("cdn.betterttv.net"));
    }
}

use serde::{Deserialize, Serialize};
use specta::Type;
use url::Url;

/// Where an emote came from. Lives in core because chat tokens carry emotes and
/// `lurkn-emotes` already depends on core, not the other way round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    Twitch,
    SevenTv,
    Bttv,
    Ffz,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EmoteScope {
    Global,
    Channel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Emote {
    pub id: String,
    pub name: String,
    pub provider: ProviderId,
    pub scope: EmoteScope,
    pub url_1x: Url,
    pub url_2x: Option<Url>,
    pub url_3x: Option<Url>,
    pub animated: bool,
    /// Width and height, so the chat log can reserve space and not reflow as
    /// images load. 7TV emotes are frequently not square.
    pub aspect: Option<(u16, u16)>,
    /// 7TV and BTTV overlay emotes stack onto the preceding emote.
    pub zero_width: bool,
}

/// Twitch's emote CDN template.
///
/// `https://static-cdn.jtvnw.net/emoticons/v2/{id}/{format}/{theme}/{scale}`
pub fn twitch_emote_url(id: &str, animated: bool, scale: &str) -> Option<Url> {
    let format = if animated { "animated" } else { "static" };
    Url::parse(&format!(
        "https://static-cdn.jtvnw.net/emoticons/v2/{id}/{format}/dark/{scale}"
    ))
    .ok()
}

/// Build a Twitch-native emote from the id an EventSub fragment carries.
pub fn twitch_emote(id: &str, name: &str, animated: bool) -> Option<Emote> {
    Some(Emote {
        id: id.to_owned(),
        name: name.to_owned(),
        provider: ProviderId::Twitch,
        scope: EmoteScope::Global,
        url_1x: twitch_emote_url(id, animated, "1.0")?,
        url_2x: twitch_emote_url(id, animated, "2.0"),
        url_3x: twitch_emote_url(id, animated, "3.0"),
        animated,
        aspect: Some((28, 28)),
        zero_width: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_twitch_cdn_url() {
        assert_eq!(
            twitch_emote_url("25", false, "1.0").unwrap().as_str(),
            "https://static-cdn.jtvnw.net/emoticons/v2/25/static/dark/1.0"
        );
        assert_eq!(
            twitch_emote_url("25", true, "3.0").unwrap().as_str(),
            "https://static-cdn.jtvnw.net/emoticons/v2/25/animated/dark/3.0"
        );
    }

    #[test]
    fn a_twitch_emote_carries_all_three_scales() {
        let emote = twitch_emote("25", "Kappa", false).unwrap();
        assert_eq!(emote.provider, ProviderId::Twitch);
        assert_eq!(emote.name, "Kappa");
        assert!(emote.url_2x.is_some() && emote.url_3x.is_some());
        assert!(!emote.zero_width);
    }

    /// Emote ids come from Twitch, but a hostile one must not escape the CDN host.
    #[test]
    fn a_hostile_emote_id_cannot_change_the_host() {
        let url = twitch_emote_url("../../evil.example/x", false, "1.0").unwrap();
        assert_eq!(url.host_str(), Some("static-cdn.jtvnw.net"));
    }
}

//! FrankerFaceZ v1. Shapes verified against live responses on 2026-09-20.

use std::collections::HashMap;

use lurkn_core::{Emote, EmoteScope, ProviderId};
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct RoomResponse {
    #[serde(default)]
    pub sets: HashMap<String, EmoteSet>,
}

/// The global endpoint names which of its sets are actually global.
#[derive(Debug, Deserialize)]
pub struct GlobalResponse {
    #[serde(default)]
    pub default_sets: Vec<i64>,
    #[serde(default)]
    pub sets: HashMap<String, EmoteSet>,
}

#[derive(Debug, Deserialize)]
pub struct EmoteSet {
    #[serde(default)]
    pub emoticons: Vec<FfzEmote>,
}

#[derive(Debug, Deserialize)]
pub struct FfzEmote {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub width: u16,
    #[serde(default)]
    pub height: u16,
    /// FFZ's term for an overlay emote.
    #[serde(default)]
    pub modifier: bool,
    #[serde(default)]
    pub urls: HashMap<String, String>,
}

pub fn room_emotes(response: &RoomResponse) -> Vec<Emote> {
    response
        .sets
        .values()
        .flat_map(|set| set.emoticons.iter())
        .filter_map(|e| to_emote(e, EmoteScope::Channel))
        .collect()
}

/// Only the sets FFZ lists in `default_sets` are global; the response carries others.
pub fn global_emotes(response: &GlobalResponse) -> Vec<Emote> {
    let wanted: Vec<String> = response.default_sets.iter().map(i64::to_string).collect();

    response
        .sets
        .iter()
        .filter(|(id, _)| wanted.contains(id))
        .flat_map(|(_, set)| set.emoticons.iter())
        .filter_map(|e| to_emote(e, EmoteScope::Global))
        .collect()
}

fn to_emote(emote: &FfzEmote, scope: EmoteScope) -> Option<Emote> {
    let pick = |scale: &str| -> Option<Url> {
        let raw = emote.urls.get(scale)?;
        // FFZ historically returned protocol-relative URLs; normalise both forms.
        let absolute = if raw.starts_with("//") {
            format!("https:{raw}")
        } else {
            raw.to_owned()
        };
        let url = Url::parse(&absolute).ok()?;
        (url.scheme() == "https" && url.host_str() == Some("cdn.frankerfacez.com")).then_some(url)
    };

    Some(Emote {
        id: emote.id.to_string(),
        name: emote.name.clone(),
        provider: ProviderId::Ffz,
        scope,
        url_1x: pick("1")?,
        url_2x: pick("2"),
        // FFZ jumps from 2 to 4; there is no 3x.
        url_3x: pick("4"),
        // FFZ emotes are static.
        animated: false,
        aspect: (emote.width > 0 && emote.height > 0).then_some((emote.width, emote.height)),
        zero_width: emote.modifier,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a live `GET .../v1/room/id/12875057`.
    const ROOM: &str = r#"{
      "room": {},
      "sets": { "441260": { "emoticons": [
        {"id":157576,"name":"monkaMEGA","height":28,"width":28,"modifier":false,
         "urls":{"1":"https://cdn.frankerfacez.com/emote/157576/1",
                 "2":"https://cdn.frankerfacez.com/emote/157576/2",
                 "4":"https://cdn.frankerfacez.com/emote/157576/4"}},
        {"id":9999,"name":"ffzHyper","height":28,"width":28,"modifier":true,
         "urls":{"1":"//cdn.frankerfacez.com/emote/9999/1"}}
      ]}}}"#;

    const GLOBAL: &str = r#"{
      "default_sets": [3],
      "sets": {
        "3": { "emoticons": [
          {"id":1,"name":"ZreknarF","height":28,"width":40,
           "urls":{"1":"https://cdn.frankerfacez.com/emote/1/1"}}]},
        "1532818": { "emoticons": [
          {"id":2,"name":"NotGlobal","height":28,"width":28,
           "urls":{"1":"https://cdn.frankerfacez.com/emote/2/1"}}]}
      }}"#;

    #[test]
    fn converts_a_room_set() {
        let response: RoomResponse = serde_json::from_str(ROOM).unwrap();
        let mut emotes = room_emotes(&response);
        emotes.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(emotes.len(), 2);
        let monka = emotes.iter().find(|e| e.name == "monkaMEGA").unwrap();
        assert_eq!(monka.provider, ProviderId::Ffz);
        assert_eq!(monka.scope, EmoteScope::Channel);
        assert_eq!(
            monka.url_1x.as_str(),
            "https://cdn.frankerfacez.com/emote/157576/1"
        );
        // FFZ has no 3x; the 4x slot fills it.
        assert_eq!(
            monka.url_3x.as_ref().unwrap().as_str(),
            "https://cdn.frankerfacez.com/emote/157576/4"
        );
        assert_eq!(monka.aspect, Some((28, 28)));
        assert!(!monka.animated);
    }

    #[test]
    fn modifier_emotes_are_zero_width() {
        let response: RoomResponse = serde_json::from_str(ROOM).unwrap();
        let hyper = room_emotes(&response)
            .into_iter()
            .find(|e| e.name == "ffzHyper")
            .unwrap();
        assert!(hyper.zero_width);
    }

    #[test]
    fn protocol_relative_urls_are_normalised_to_https() {
        let response: RoomResponse = serde_json::from_str(ROOM).unwrap();
        let hyper = room_emotes(&response)
            .into_iter()
            .find(|e| e.name == "ffzHyper")
            .unwrap();
        assert_eq!(
            hyper.url_1x.as_str(),
            "https://cdn.frankerfacez.com/emote/9999/1"
        );
    }

    /// The global response carries sets that are not global; only `default_sets`
    /// counts, or unrelated emotes would leak into every channel.
    #[test]
    fn only_default_sets_are_treated_as_global() {
        let response: GlobalResponse = serde_json::from_str(GLOBAL).unwrap();
        let emotes = global_emotes(&response);
        assert_eq!(emotes.len(), 1);
        assert_eq!(emotes[0].name, "ZreknarF");
        assert_eq!(emotes[0].scope, EmoteScope::Global);
    }

    #[test]
    fn an_emote_without_a_usable_url_is_skipped() {
        let response: RoomResponse = serde_json::from_str(
            r#"{"sets":{"1":{"emoticons":[
                 {"id":1,"name":"Evil","urls":{"1":"https://evil.example/x"}},
                 {"id":2,"name":"NoUrls","urls":{}}]}}}"#,
        )
        .unwrap();
        assert!(room_emotes(&response).is_empty());
    }
}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Provider fetching against a mock server, with the failure isolation
//! Lurkn relies on.

use lurkn_emotes::fetch::{Bases, EmoteFetcher};
use lurkn_emotes::EmoteRegistry;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fetcher(server: &MockServer) -> EmoteFetcher {
    EmoteFetcher::with_bases(
        reqwest::Client::new(),
        Bases {
            seventv: server.uri(),
            bttv: server.uri(),
            ffz: server.uri(),
        },
    )
}

async fn mount_seventv(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/v3/users/twitch/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "emote_set": { "emotes": [
                { "id": "7tv1", "name": "Pog", "flags": 0,
                  "data": { "animated": false, "host": {
                    "url": "//cdn.7tv.app/emote/7tv1",
                    "files": [{"name":"1x.webp","width":32,"height":32,"format":"WEBP"}]}}}
            ]}
        })))
        .mount(server)
        .await;
}

async fn mount_bttv(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/3/cached/users/twitch/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "x", "channelEmotes": [],
            "sharedEmotes": [
                {"id":"bttv1","code":"Pog","imageType":"png","animated":false},
                {"id":"bttv2","code":"PepePls","imageType":"gif","animated":true}
            ]
        })))
        .mount(server)
        .await;
}

async fn mount_ffz(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/v1/room/id/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "room": {},
            "sets": { "1": { "emoticons": [
                {"id":1,"name":"monkaS","width":28,"height":28,
                 "urls":{"1":"https://cdn.frankerfacez.com/emote/1/1"}}
            ]}}
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn fetches_all_three_channel_providers() {
    let server = MockServer::start().await;
    mount_seventv(&server).await;
    mount_bttv(&server).await;
    mount_ffz(&server).await;

    let registry = EmoteRegistry::build(fetcher(&server).all_channel("1").await);

    assert_eq!(
        registry.len(),
        3,
        "Pog collides; PepePls and monkaS are distinct"
    );
    assert!(registry.get("PepePls").is_some());
    assert!(registry.get("monkaS").is_some());
}

/// The precedence rule, end to end: 7TV and BTTV both define `Pog`.
#[tokio::test]
async fn seventv_wins_a_collision_with_bttv() {
    let server = MockServer::start().await;
    mount_seventv(&server).await;
    mount_bttv(&server).await;
    mount_ffz(&server).await;

    let registry = EmoteRegistry::build(fetcher(&server).all_channel("1").await);
    assert_eq!(registry.get("Pog").unwrap().id, "7tv1");
}

/// 7TV unavailable → Twitch emotes continue working. The
/// other providers must keep working too.
#[tokio::test]
async fn one_provider_failing_does_not_take_out_the_others() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v3/users/twitch/1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    mount_bttv(&server).await;
    mount_ffz(&server).await;

    let registry = EmoteRegistry::build(fetcher(&server).all_channel("1").await);

    // With 7TV down, BTTV now owns Pog.
    assert_eq!(registry.get("Pog").unwrap().id, "bttv1");
    assert!(registry.get("monkaS").is_some());
}

#[tokio::test]
async fn every_provider_failing_yields_an_empty_registry_not_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let registry = EmoteRegistry::build(fetcher(&server).all_channel("1").await);
    assert!(registry.is_empty());
}

/// A channel with no 7TV set answers 404. That is normal, not a failure.
#[tokio::test]
async fn a_missing_channel_set_is_not_treated_as_a_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v3/users/twitch/1"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let outcome = fetcher(&server).seventv_channel("1").await;
    assert!(outcome.emotes.is_empty());
    assert!(!outcome.failed, "404 means 'no set', not 'provider broken'");
}

#[tokio::test]
async fn a_malformed_body_is_reported_as_a_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/cached/users/twitch/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{ not json"))
        .mount(&server)
        .await;

    let outcome = fetcher(&server).bttv_channel("1").await;
    assert!(outcome.emotes.is_empty());
    assert!(outcome.failed);
}

#[tokio::test]
async fn global_sets_are_fetched_from_the_global_endpoints() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v3/emote-sets/global"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "emotes": [{ "id": "g1", "name": "GlobalPog", "flags": 0,
                "data": { "animated": false, "host": {
                  "url": "//cdn.7tv.app/emote/g1",
                  "files": [{"name":"1x.webp","width":32,"height":32,"format":"WEBP"}]}}}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/3/cached/emotes/global"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id":"bg1","code":"GlobalBttv","imageType":"png","animated":false}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/set/global"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "default_sets": [3],
            "sets": { "3": { "emoticons": [
                {"id":1,"name":"GlobalFfz","width":28,"height":28,
                 "urls":{"1":"https://cdn.frankerfacez.com/emote/1/1"}}]}}
        })))
        .mount(&server)
        .await;

    let registry = EmoteRegistry::build(fetcher(&server).all_global().await);
    assert_eq!(registry.len(), 3);
    assert!(registry.get("GlobalPog").is_some());
    assert!(registry.get("GlobalBttv").is_some());
    assert!(registry.get("GlobalFfz").is_some());
}

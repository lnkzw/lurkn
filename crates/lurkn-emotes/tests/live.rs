#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Contract checks against the real 7TV / BTTV / FFZ APIs.
//!
//! Ignored by default: these hit community services with no SLA, so an outage
//! must never red the build (`docs/architecture.md` §12). Run deliberately with
//! `cargo test -p lurkn-emotes --test live -- --ignored --nocapture`.

use lurkn_emotes::fetch::EmoteFetcher;
use lurkn_emotes::EmoteRegistry;

/// A large, long-established channel: it has sets on all three providers.
const BROADCASTER_ID: &str = "12875057";

fn fetcher() -> EmoteFetcher {
    EmoteFetcher::new(reqwest::Client::new())
}

#[tokio::test]
#[ignore = "hits live third-party APIs"]
async fn every_provider_still_returns_a_usable_channel_set() {
    let f = fetcher();

    let seven = f.seventv_channel(BROADCASTER_ID).await;
    let bttv = f.bttv_channel(BROADCASTER_ID).await;
    let ffz = f.ffz_channel(BROADCASTER_ID).await;

    println!(
        "channel sets — 7TV {} (failed {}), BTTV {} (failed {}), FFZ {} (failed {})",
        seven.emotes.len(),
        seven.failed,
        bttv.emotes.len(),
        bttv.failed,
        ffz.emotes.len(),
        ffz.failed
    );

    assert!(!seven.failed, "7TV response shape changed");
    assert!(!bttv.failed, "BTTV response shape changed");
    assert!(!ffz.failed, "FFZ response shape changed");
    assert!(!seven.emotes.is_empty(), "7TV returned no emotes");
    assert!(!bttv.emotes.is_empty(), "BTTV returned no emotes");
    assert!(!ffz.emotes.is_empty(), "FFZ returned no emotes");
}

#[tokio::test]
#[ignore = "hits live third-party APIs"]
async fn every_provider_still_returns_a_usable_global_set() {
    let f = fetcher();

    let seven = f.seventv_global().await;
    let bttv = f.bttv_global().await;
    let ffz = f.ffz_global().await;

    println!(
        "global sets — 7TV {}, BTTV {}, FFZ {}",
        seven.emotes.len(),
        bttv.emotes.len(),
        ffz.emotes.len()
    );

    assert!(!seven.emotes.is_empty(), "7TV global set is empty");
    assert!(!bttv.emotes.is_empty(), "BTTV global set is empty");
    assert!(!ffz.emotes.is_empty(), "FFZ global set is empty");
}

/// Every URL we hand the WebView must be https on a known CDN, whatever the
/// providers return (`docs/architecture.md` §11).
#[tokio::test]
#[ignore = "hits live third-party APIs"]
async fn every_live_emote_url_is_on_an_allowed_cdn() {
    let sets = fetcher().all_channel(BROADCASTER_ID).await;
    let mut checked = 0usize;

    for emote in sets.iter().flatten() {
        for url in [
            Some(&emote.url_1x),
            emote.url_2x.as_ref(),
            emote.url_3x.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                lurkn_emotes::is_allowed_emote_host(url),
                "{} served {url} off an allowed CDN",
                emote.name
            );
            checked += 1;
        }
    }

    println!("checked {checked} live emote URLs");
    assert!(checked > 0);
}

/// The registry must survive real data: hundreds of emotes, real collisions.
#[tokio::test]
#[ignore = "hits live third-party APIs"]
async fn a_registry_built_from_live_data_resolves_collisions() {
    let f = fetcher();
    let mut sets = f.all_channel(BROADCASTER_ID).await;
    sets.extend(f.all_global().await);

    let total: usize = sets.iter().map(Vec::len).sum();
    let registry = EmoteRegistry::build(sets);

    println!(
        "{total} emotes across providers collapsed to {} names",
        registry.len()
    );
    assert!(registry.len() > 100, "expected a substantial set");
    assert!(
        registry.len() < total,
        "real data should contain at least one name collision"
    );

    // Tokenising a message against the live registry must not panic or lose text.
    let tokens = registry.apply(vec![lurkn_core::ChatToken::Text {
        text: "hello world this is a message".into(),
    }]);
    assert!(!tokens.is_empty());
}

/// Overlay emotes exist in real data; the flag must actually be read.
#[tokio::test]
#[ignore = "hits live third-party APIs"]
async fn live_data_contains_zero_width_emotes() {
    let sets = fetcher().all_channel(BROADCASTER_ID).await;
    let zero_width: Vec<&str> = sets
        .iter()
        .flatten()
        .filter(|e| e.zero_width)
        .map(|e| e.name.as_str())
        .collect();

    println!("{} overlay emotes: {zero_width:?}", zero_width.len());
    assert!(
        !zero_width.is_empty(),
        "no overlay emotes found — the zero-width flag may no longer be read correctly"
    );
}

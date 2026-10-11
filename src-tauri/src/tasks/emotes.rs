//! Loading and caching third-party emotes for a channel.
//!
//! Global sets are shared across channels and cached for a day; channel sets for
//! an hour (`docs/architecture.md` §10). A stale set is served while the refetch
//! happens, so switching channels never shows a bare-text chat if it can help it.

use std::sync::Arc;

use lurkn_core::{Emote, UserId};
use lurkn_emotes::{EmoteFetcher, EmoteRegistry};
use lurkn_store::Store;
use time::{Duration, OffsetDateTime};

const PROVIDERS: [&str; 3] = ["seventv", "bttv", "ffz"];
const GLOBAL_TTL: Duration = Duration::hours(24);
const CHANNEL_TTL: Duration = Duration::hours(1);

/// Load every global set, from cache when it is fresh enough.
pub async fn load_global(fetcher: &EmoteFetcher, store: &Arc<Store>) -> Vec<Vec<Emote>> {
    let cached: Vec<Option<Vec<Emote>>> = PROVIDERS
        .iter()
        .map(|p| fresh(store, p, None, GLOBAL_TTL))
        .collect();

    if cached.iter().all(Option::is_some) {
        tracing::debug!("global emotes served from cache");
        return cached.into_iter().flatten().collect();
    }

    let fetched = fetcher.all_global().await;
    persist(store, None, &fetched);

    // Fall back to whatever was cached for any provider that came back empty.
    merge(fetched, cached)
}

/// Load a channel's sets, from cache when fresh.
pub async fn load_channel(
    fetcher: &EmoteFetcher,
    store: &Arc<Store>,
    channel: &UserId,
) -> Vec<Vec<Emote>> {
    let id = channel.as_str();
    let cached: Vec<Option<Vec<Emote>>> = PROVIDERS
        .iter()
        .map(|p| fresh(store, p, Some(id), CHANNEL_TTL))
        .collect();

    if cached.iter().all(Option::is_some) {
        tracing::debug!(channel = id, "channel emotes served from cache");
        return cached.into_iter().flatten().collect();
    }

    let fetched = fetcher.all_channel(id).await;
    persist(store, Some(id), &fetched);
    merge(fetched, cached)
}

/// Build the registry a channel's chat will be rendered with.
pub async fn registry_for(
    fetcher: &EmoteFetcher,
    store: &Arc<Store>,
    channel: &UserId,
) -> EmoteRegistry {
    let (global, channel_sets) = tokio::join!(
        load_global(fetcher, store),
        load_channel(fetcher, store, channel)
    );

    // Channel sets first so they win ties; the registry enforces precedence
    // regardless, but ordering keeps the intent obvious.
    let mut sets = channel_sets;
    sets.extend(global);

    let registry = EmoteRegistry::build(sets);
    tracing::debug!(
        channel = channel.as_str(),
        emotes = registry.len(),
        "emote registry built"
    );
    registry
}

fn fresh(
    store: &Arc<Store>,
    provider: &str,
    channel_id: Option<&str>,
    ttl: Duration,
) -> Option<Vec<Emote>> {
    let (emotes, fetched_at) = store.load_emote_set(provider, channel_id).ok().flatten()?;
    (OffsetDateTime::now_utc() - fetched_at < ttl).then_some(emotes)
}

fn persist(store: &Arc<Store>, channel_id: Option<&str>, sets: &[Vec<Emote>]) {
    for (provider, emotes) in PROVIDERS.iter().zip(sets) {
        // An empty set usually means the provider failed; caching it would hide
        // working emotes behind a TTL.
        if emotes.is_empty() {
            continue;
        }
        if let Err(error) = store.save_emote_set(provider, channel_id, emotes) {
            tracing::warn!(%error, provider, "could not cache an emote set");
        }
    }
}

/// Prefer freshly fetched sets; fall back to cache where the fetch produced nothing.
fn merge(fetched: Vec<Vec<Emote>>, cached: Vec<Option<Vec<Emote>>>) -> Vec<Vec<Emote>> {
    fetched
        .into_iter()
        .zip(cached)
        .map(|(new, old)| {
            if new.is_empty() {
                old.unwrap_or_default()
            } else {
                new
            }
        })
        .collect()
}

/// Load a channel's emotes into `AppState` and tell the frontend.
pub async fn refresh_for(app: &tauri::AppHandle, channel: &UserId) {
    use tauri::Manager;
    use tauri_specta::Event;

    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };

    let registry = registry_for(&state.emote_fetcher, &state.store, channel).await;
    let count = registry.len();

    // A channel switch during the fetch must not install the wrong set.
    let still_current = state
        .current_channel
        .read()
        .await
        .as_ref()
        .is_some_and(|(_, id)| id == channel);
    if !still_current {
        tracing::debug!(
            channel = channel.as_str(),
            "discarding emotes for a channel we left"
        );
        return;
    }

    *state.emotes.write().await = registry;

    // Badges come from Twitch, not the emote providers, but they load on the same
    // trigger and are needed before the first message renders well.
    load_badges(&state, channel).await;

    if let Err(error) = (crate::events::EmotesReady {
        channel_id: channel.to_string(),
        count: count as u32,
    })
    .emit(app)
    {
        tracing::error!(%error, "could not emit the emote-ready event");
    }
}

/// Fetch global badge sets once, and this channel's every switch.
async fn load_badges(state: &crate::state::AppState, channel: &UserId) {
    let service = state.badges();

    if state.global_badges.read().await.is_empty() {
        match service.global().await {
            Ok(map) => *state.global_badges.write().await = map,
            // Badges are decoration: chat without them beats no chat.
            Err(error) => tracing::warn!(%error, "could not load global chat badges"),
        }
    }

    match service.channel(channel.as_str()).await {
        Ok(map) => *state.channel_badges.write().await = map,
        Err(error) => tracing::warn!(%error, "could not load channel chat badges"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emote(name: &str) -> Emote {
        Emote {
            id: name.into(),
            name: name.into(),
            provider: lurkn_core::ProviderId::SevenTv,
            scope: lurkn_core::EmoteScope::Channel,
            url_1x: url::Url::parse("https://cdn.7tv.app/emote/1/1x.webp").unwrap(),
            url_2x: None,
            url_3x: None,
            animated: false,
            aspect: None,
            zero_width: false,
        }
    }

    /// A provider that failed this time should keep showing its cached emotes
    /// rather than silently losing them.
    #[test]
    fn a_failed_fetch_falls_back_to_the_cached_set() {
        let merged = merge(
            vec![vec![emote("Fresh")], vec![], vec![emote("AlsoFresh")]],
            vec![None, Some(vec![emote("Cached")]), None],
        );
        assert_eq!(merged[0][0].name, "Fresh");
        assert_eq!(merged[1][0].name, "Cached");
        assert_eq!(merged[2][0].name, "AlsoFresh");
    }

    #[test]
    fn a_fresh_set_always_wins_over_cache() {
        let merged = merge(vec![vec![emote("Fresh")]], vec![Some(vec![emote("Stale")])]);
        assert_eq!(merged[0][0].name, "Fresh");
    }

    #[test]
    fn nothing_anywhere_yields_an_empty_set_not_a_panic() {
        assert_eq!(merge(vec![vec![]], vec![None]), vec![Vec::<Emote>::new()]);
    }
}

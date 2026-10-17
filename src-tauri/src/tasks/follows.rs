//! The follow list and its live state.
//!
//! Two cadences, per `docs/architecture.md` §4.3:
//!   - the follow list itself, slowly (it changes rarely),
//!   - `Get Followed Streams`, every 60s ± jitter — which *is* the live check,
//!     because Helix returns only live channels there.

use std::time::Duration as StdDuration;

use lurkn_core::{FollowedChannel, UserId};
use lurkn_twitch::follows::{assemble, diff_live, sort_for_sidebar};
use lurkn_twitch::HelixError;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::events::{FollowsUpdated, LiveDelta};
use crate::state::AppState;

/// How long the follow list stays fresh before a poll refetches it.
const FOLLOWS_TTL: time::Duration = time::Duration::minutes(15);

/// Everything the sidebar needs, plus the freshness the UI reports.
#[derive(Debug, Default)]
pub struct FollowsState {
    pub channels: Vec<FollowedChannel>,
    pub live_known: bool,
    pub stale_reason: Option<String>,
    pub loading: bool,
    /// Live ids from the previous poll, for delta emission.
    pub previous_live: Vec<UserId>,
    pub follows_fetched_at: Option<time::OffsetDateTime>,
    /// When the last poll succeeded, on the wall clock.
    ///
    /// Wall clock, not `Instant`: the monotonic clock does not advance while
    /// the machine is suspended, so an `Instant` would report a few seconds
    /// having passed over a night's sleep — which is exactly the case this
    /// field exists to notice.
    pub last_poll_at: Option<time::OffsetDateTime>,
}

impl FollowsState {
    pub fn view(&self) -> crate::commands::follows::FollowsView {
        crate::commands::follows::FollowsView {
            channels: self.channels.clone(),
            live_known: self.live_known,
            stale_reason: self.stale_reason.clone(),
            loading: self.loading,
        }
    }

    fn live_ids(&self) -> Vec<UserId> {
        self.channels
            .iter()
            .filter(|c| c.is_live())
            .map(|c| c.channel.user.id.clone())
            .collect()
    }
}

/// Show the cached list immediately, before any network call.
pub async fn load_from_cache(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let cached = match state.store.load_follows() {
        Ok(list) => list,
        Err(error) => {
            tracing::warn!(%error, "could not read the cached follow list");
            return;
        }
    };

    if cached.is_empty() {
        return;
    }

    tracing::debug!(count = cached.len(), "restored the follow list from cache");
    {
        let mut follows = state.follows.write().await;
        follows.channels = cached;
        // Nothing is known to be live until a poll succeeds.
        follows.live_known = false;
        follows.follows_fetched_at = state.store.follows_fetched_at().ok().flatten();
    }
    emit(app, &state).await;
}

/// How a refresh went, as far as the next one is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    Done,
    /// Twitch could not be reached at all: no network yet, typically, in the
    /// seconds after a wake from sleep.
    Unreachable,
    /// Logged out, refused, rate limited: nothing a quick retry would fix.
    Skipped,
}

/// One refresh cycle. `force` ignores the follow-list TTL.
pub async fn refresh_now(app: &AppHandle, state: &AppState, force: bool) -> Refresh {
    let Some(service) = state.follows_service().await else {
        tracing::debug!("not logged in; skipping the follow refresh");
        return Refresh::Skipped;
    };
    let Some(user_id) = state.current_user_id().await else {
        return Refresh::Skipped;
    };

    {
        let mut follows = state.follows.write().await;
        follows.loading = true;
    }
    emit(app, state).await;

    let needs_full = force || {
        let follows = state.follows.read().await;
        follows.channels.is_empty()
            || follows
                .follows_fetched_at
                .is_none_or(|at| time::OffsetDateTime::now_utc() - at > FOLLOWS_TTL)
    };

    let outcome = fetch(&service, &user_id, needs_full, state).await;

    let mut follows = state.follows.write().await;
    follows.loading = false;

    let refresh = match &outcome {
        Ok(_) => Refresh::Done,
        Err(HelixError::Transport(_)) => Refresh::Unreachable,
        Err(_) => Refresh::Skipped,
    };

    match outcome {
        Ok(channels) => {
            let previous = std::mem::take(&mut follows.previous_live);
            // Captured before the flag is set: everything live on the first
            // successful poll went live before Lurkn started, so it is news to
            // the list but not to the user.
            let first_poll = !follows.live_known;

            // A gap far longer than the poll interval means the machine was
            // asleep, or Lurkn was starved of network, for that whole time.
            // Everything that started streaming meanwhile is news to the list
            // and not to the user, the same as on the first poll — announcing
            // it produces the armada of notifications a laptop lid earns.
            let resuming = follows.last_poll_at.is_some_and(|at| {
                time::OffsetDateTime::now_utc() - at > time::Duration::seconds(RESUME_GAP_SECS)
            });
            follows.last_poll_at = Some(time::OffsetDateTime::now_utc());
            follows.channels = channels;
            follows.live_known = true;
            follows.stale_reason = None;
            if needs_full {
                follows.follows_fetched_at = Some(time::OffsetDateTime::now_utc());
            }

            let current = follows.live_ids();
            let delta = diff_live(&previous, &current);
            follows.previous_live = current;

            if let Err(error) = state.store.save_follows(&follows.channels) {
                tracing::warn!(%error, "could not cache the follow list");
            }
            drop(follows);

            if !delta.is_empty() {
                tracing::debug!(
                    went_live = delta.went_live.len(),
                    went_offline = delta.went_offline.len(),
                    "live set changed"
                );

                if first_poll || resuming {
                    tracing::debug!(
                        first_poll,
                        resuming,
                        went_live = delta.went_live.len(),
                        "catching up; not announcing"
                    );
                } else {
                    announce_live(app, state, &delta.went_live).await;
                }
                let payload = LiveDelta {
                    went_live: delta.went_live.iter().map(|i| i.to_string()).collect(),
                    went_offline: delta.went_offline.iter().map(|i| i.to_string()).collect(),
                };
                if let Err(error) = payload.emit(app) {
                    tracing::error!(%error, "could not emit the live delta");
                }
            }
        }
        Err(error) => {
            // Keep whatever is on screen; say why it may be out of date.
            tracing::warn!(%error, "the follow refresh failed; serving the cached list");
            follows.stale_reason = Some(describe(&error));
            drop(follows);
        }
    }

    emit(app, state).await;
    refresh
}

async fn fetch(
    service: &lurkn_twitch::FollowsService,
    user_id: &str,
    needs_full: bool,
    state: &AppState,
) -> std::result::Result<Vec<FollowedChannel>, HelixError> {
    let live = service.live_streams(user_id).await?;

    if !needs_full {
        // The cheap path: reuse the known channels, swap in the new live set.
        let mut channels = state.follows.read().await.channels.clone();
        let mut by_id: std::collections::HashMap<_, _> = live
            .into_iter()
            .map(|s| (s.channel_id.clone(), s))
            .collect();

        for entry in &mut channels {
            entry.stream = by_id.remove(&entry.channel.user.id);
            if let Some(stream) = &entry.stream {
                entry.channel.title = Some(stream.title.clone());
                entry.channel.category = stream.category.clone();
            }
        }
        sort_for_sidebar(&mut channels);
        return Ok(channels);
    }

    let channels = service.followed_channels(user_id).await?;
    let ids: Vec<UserId> = channels.iter().map(|c| c.user.id.clone()).collect();

    // Avatars and offline metadata are nice-to-have: a failure here must not cost
    // the user their follow list.
    let users = service.users(&ids).await.unwrap_or_else(|error| {
        tracing::warn!(%error, "could not fetch avatars");
        Default::default()
    });
    let offline_ids: Vec<UserId> = ids
        .iter()
        .filter(|id| !live.iter().any(|s| &&s.channel_id == id))
        .cloned()
        .collect();
    let info = service
        .channel_info(&offline_ids)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not fetch offline channel metadata");
            Default::default()
        });

    Ok(assemble(channels, live, &users, &info))
}

/// A sentence the user can act on, not a status code.
fn describe(error: &HelixError) -> String {
    match error {
        HelixError::Unauthorized => "Your Twitch login expired.".to_owned(),
        HelixError::RateLimited { .. } => {
            "Twitch is rate limiting us. Showing the last known list.".to_owned()
        }
        HelixError::Transport(_) => {
            "Could not reach Twitch. Showing the last known list.".to_owned()
        }
        _ => "Twitch could not be reached. Showing the last known list.".to_owned(),
    }
}

/// How long a gap between polls means the machine was away rather than busy.
///
/// Five times the poll interval: a slow refresh or a retry is well inside it,
/// a closed lid is not.
const RESUME_GAP_SECS: i64 = (lurkn_twitch::LIVE_POLL_INTERVAL_SECS * 5) as i64;

/// Past this many at once, one line instead of a stack of them.
///
/// A handful is information; a dozen is a wall that has to be dismissed one at
/// a time, which teaches people to turn notifications off.
const MAX_LIVE_NOTIFICATIONS: usize = 4;

/// Tell the user who just went live, if they asked to be told.
///
/// The caller skips the first successful poll of a session — see `first_poll`
/// there. A dozen notifications on launch is what makes people turn
/// notifications off for good.
async fn announce_live(app: &AppHandle, state: &AppState, went_live: &[UserId]) {
    if went_live.is_empty() || !state.settings.read().await.notifications.live {
        return;
    }

    let follows = state.follows.read().await;

    let names: Vec<(&str, Option<&str>)> = went_live
        .iter()
        .filter_map(|id| {
            let channel = follows.channels.iter().find(|c| &c.channel.user.id == id)?;
            let category = channel
                .stream
                .as_ref()
                .and_then(|s| s.category.as_ref())
                .map(|c| c.name.as_str());
            Some((channel.channel.user.display_name.as_str(), category))
        })
        .collect();

    if names.len() > MAX_LIVE_NOTIFICATIONS {
        crate::notify::many_went_live(app, &names.iter().map(|(n, _)| *n).collect::<Vec<_>>());
        return;
    }

    for (name, category) in names {
        crate::notify::went_live(app, name, category);
    }
}

async fn emit(app: &AppHandle, state: &AppState) {
    let view = state.follows.read().await.view();
    if let Err(error) = FollowsUpdated(view).emit(app) {
        tracing::error!(%error, "could not emit the follow list");
    }

    // The tray lists who is live, so it follows the same updates the sidebar
    // does rather than polling on its own.
    crate::tray::refresh(app).await;

    // An open channel the reader does not follow rides the same poll, so its
    // title and live state age no faster than a follow's.
    crate::commands::chat::refresh_unfollowed(app, state).await;
}

/// How often the poller looks at the clocks. Short, because it is also how
/// soon a wake from sleep is noticed; looking costs nothing.
const TICK: StdDuration = StdDuration::from_secs(5);

/// How far past a tick the wall clock may run before it means the machine was
/// asleep rather than busy.
const WAKE_SLACK: StdDuration = StdDuration::from_secs(30);

/// How soon a poll that could not reach Twitch is tried again. A machine that
/// has just woken is usually a few seconds ahead of its network, and waiting a
/// whole interval for the second try leaves the list as old as the sleep for
/// another minute.
const RETRY_UNREACHABLE: StdDuration = StdDuration::from_secs(10);

/// Poll forever on the live cadence, with jitter so many clients do not align,
/// and at once after a wake from sleep.
pub async fn run_poller(app: AppHandle) {
    let mut due = tokio::time::Instant::now() + interval();
    let mut wall = time::OffsetDateTime::now_utc();

    loop {
        tokio::time::sleep(TICK).await;

        // Tokio's clock is monotonic and stands still while the machine
        // sleeps, so a wait begun before the lid closed still has most of its
        // time to run once it opens. The wall clock keeps going; a jump in it
        // is the machine waking up.
        let woke = woke_up(wall, time::OffsetDateTime::now_utc());
        wall = time::OffsetDateTime::now_utc();
        if !woke && tokio::time::Instant::now() < due {
            continue;
        }
        if woke {
            tracing::debug!("the machine woke up; refreshing the live list now");
        }

        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let refresh = if state.current_user_id().await.is_some() {
            refresh_now(&app, &state, false).await
        } else {
            Refresh::Skipped
        };

        due = tokio::time::Instant::now()
            + match refresh {
                Refresh::Unreachable => RETRY_UNREACHABLE,
                Refresh::Done | Refresh::Skipped => interval(),
            };
        // A slow refresh is time the wall clock spent awake.
        wall = time::OffsetDateTime::now_utc();
    }
}

fn interval() -> StdDuration {
    use lurkn_twitch::{LIVE_POLL_INTERVAL_SECS, LIVE_POLL_JITTER_SECS};
    StdDuration::from_secs(LIVE_POLL_INTERVAL_SECS) + jitter(LIVE_POLL_JITTER_SECS)
}

/// Whether more wall-clock time passed over one tick than a tick and some
/// slack: the machine was suspended in between.
fn woke_up(before: time::OffsetDateTime, now: time::OffsetDateTime) -> bool {
    now - before > TICK + WAKE_SLACK
}

/// Jitter without pulling in an rng crate for one number.
fn jitter(max_secs: u64) -> StdDuration {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    StdDuration::from_millis(nanos % (max_secs.max(1) * 1000))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: time::OffsetDateTime = time::macros::datetime!(2026-10-17 08:00 UTC);

    #[test]
    fn an_ordinary_tick_is_not_a_wake() {
        assert!(!woke_up(T0, T0 + TICK));
        // A busy runtime runs late, but not by half a minute.
        assert!(!woke_up(T0, T0 + TICK + StdDuration::from_secs(20)));
    }

    #[test]
    fn a_night_between_two_ticks_is_a_wake() {
        assert!(woke_up(T0, T0 + time::Duration::hours(8)));
        assert!(woke_up(
            T0,
            T0 + TICK + WAKE_SLACK + StdDuration::from_secs(1)
        ));
    }

    /// The wall clock set back, by hand or by NTP, is not a wake.
    #[test]
    fn a_clock_set_back_is_not_a_wake() {
        assert!(!woke_up(T0, T0 - time::Duration::minutes(5)));
    }

    /// A wake must be noticed long before the poll would have come round.
    #[test]
    fn a_wake_is_noticed_well_inside_the_interval() {
        assert!(TICK * 6 <= StdDuration::from_secs(lurkn_twitch::LIVE_POLL_INTERVAL_SECS));
    }
}

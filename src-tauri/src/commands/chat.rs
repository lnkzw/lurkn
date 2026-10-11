//! Chat commands. See `docs/architecture.md` §8.3 and §5.

use lurkn_core::Login;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tauri_specta::Event;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatStatus {
    /// The channel chat is currently following, if any.
    pub channel_login: Option<String>,
    pub channel_id: Option<String>,
    /// `connecting` | `connected` | `reconnecting` | `failed` | `disconnected`
    pub state: String,
    pub detail: Option<String>,
    /// Followers-only, slow mode and friends for the current channel.
    pub room: lurkn_core::RoomState,
    /// Ready-made labels for the chat header.
    pub room_labels: Vec<String>,
}

/// The channel that was open when Lurkn last closed.
///
/// No longer has to be followed: any channel can be opened, so any channel can
/// be the one to come back to. One that has since been renamed or closed fails
/// to select and the UI shows its empty state, which is the right outcome.
#[tauri::command]
#[specta::specta]
pub async fn last_channel(state: State<'_, AppState>) -> Result<Option<String>, ()> {
    Ok(state.store.load_last_channel())
}

/// Fetch a channel the reader does not follow, by name, as fully as a follow.
async fn load_unfollowed(
    state: &AppState,
    login: &Login,
) -> Result<lurkn_core::FollowedChannel, String> {
    let service = state
        .follows_service()
        .await
        .ok_or_else(|| "Log in to open a channel.".to_owned())?;

    let unanswered = |error: lurkn_twitch::error::HelixError| {
        tracing::warn!(%error, "could not look up a channel");
        "Twitch did not answer. Try again in a moment.".to_owned()
    };

    let user = service
        .user_by_login(login)
        .await
        .map_err(unanswered)?
        .ok_or_else(|| format!("Twitch has no channel called {}.", login.as_str()))?;

    service.channel_state(user).await.map_err(unanswered)
}

/// Refetch the open unfollowed channel, for the follow poll.
///
/// Keeps its title and live state as current as a follow's, which a snapshot
/// taken at the moment it was opened would not be.
pub async fn refresh_unfollowed(app: &tauri::AppHandle, state: &AppState) {
    let Some(login) = state
        .unfollowed
        .read()
        .await
        .as_ref()
        .map(|c| c.channel.user.login.clone())
    else {
        return;
    };

    match load_unfollowed(state, &login).await {
        Ok(channel) => set_unfollowed(app, state, Some(channel)).await,
        // Keep the last good copy: a failed poll is not a reason to blank the
        // header of a chat that is still working.
        Err(message) => tracing::debug!(message, "could not refresh the open channel"),
    }
}

async fn set_unfollowed(
    app: &tauri::AppHandle,
    state: &AppState,
    channel: Option<lurkn_core::FollowedChannel>,
) {
    *state.unfollowed.write().await = channel.clone();
    if let Err(error) = crate::events::OpenChannelChanged(channel).emit(app) {
        tracing::warn!(%error, "could not publish the open channel");
    }
}

/// The recent-list entry for a channel, carrying what it actually has.
fn hit_of(channel: &lurkn_core::FollowedChannel) -> lurkn_core::ChannelHit {
    lurkn_core::ChannelHit {
        id: channel.channel.user.id.clone(),
        login: channel.channel.user.login.clone(),
        display_name: channel.channel.user.display_name.clone(),
        avatar_url: channel.channel.user.avatar_url.clone(),
        live: channel.stream.is_some(),
        category: channel.channel.category.as_ref().map(|c| c.name.clone()),
        title: channel.channel.title.clone(),
        viewers: channel.stream.as_ref().map(|s| s.viewer_count),
    }
}

/// Channels matching a name, for the channel palette.
#[tauri::command]
#[specta::specta]
pub async fn search_channels(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<lurkn_core::ChannelHit>, String> {
    let query = query.trim();
    // A single character matches half of Twitch; wait for something to go on.
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }

    let service = state
        .follows_service()
        .await
        .ok_or_else(|| "Log in to search.".to_owned())?;

    service.search_channels(query).await.map_err(|error| {
        tracing::warn!(%error, "could not search channels");
        "Twitch did not answer. Try again in a moment.".to_owned()
    })
}

/// Channels opened without following them, most recent first.
#[tauri::command]
#[specta::specta]
pub async fn recent_channels(
    state: State<'_, AppState>,
) -> Result<Vec<lurkn_core::ChannelHit>, ()> {
    Ok(state.store.load_recent_channels())
}

/// Select a channel: chat follows it, and no browser is opened.
///
/// Opening a stream is a separate, explicit action — see `docs/architecture.md`
/// §8.3. Arrow-keying down the sidebar must not spray browser tabs. The one
/// thing a selection may do is re-point a synced window that is *already* open,
/// and only when the user has asked for that.
#[tauri::command]
#[specta::specta]
pub async fn select_channel(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    login: String,
) -> Result<(), String> {
    let login = Login::parse(&login).map_err(|_| "That channel name is not usable.".to_owned())?;

    let followed = {
        let follows = state.follows.read().await;
        follows
            .channels
            .iter()
            .find(|c| c.channel.user.login == login)
            .map(|c| c.channel.user.id.clone())
    };

    // A channel outside the follow list is fetched exactly as a follow is, so
    // it arrives with its title, category and live state rather than a bare
    // name. It is remembered for the recent list only here, so following
    // someone later does not leave a stale duplicate behind.
    let channel_id = match followed {
        Some(id) => {
            set_unfollowed(&app, &state, None).await;
            id
        }
        None => {
            let channel = load_unfollowed(&state, &login).await?;
            let id = channel.channel.user.id.clone();

            if let Err(error) = state.store.remember_channel(&hit_of(&channel)) {
                tracing::debug!(%error, "could not remember a recent channel");
            }
            set_unfollowed(&app, &state, Some(channel)).await;
            id
        }
    };

    let remembered = login.to_string();

    state
        .set_current_channel(Some((login, channel_id.clone())))
        .await
        .map_err(|error| {
            tracing::warn!(%error, "could not switch chat to the selected channel");
            "Could not join that channel's chat.".to_owned()
        })?;

    // Publish the new status: nothing else emits one on a plain channel switch,
    // so the header would otherwise keep naming the previous channel.
    if let Err(error) = crate::events::ChatStateChanged(state.chat_status().await).emit(&app) {
        tracing::error!(%error, "could not emit the chat state");
    }

    if let Err(error) = state.store.save_last_channel(Some(&remembered)) {
        // Not worth failing the switch over; it only costs the next restart.
        tracing::warn!(%error, "could not remember the open channel");
    }

    follow_with_synced_window(&state, &remembered).await;

    // An overlay set to show the open channel follows it. In the background:
    // it may fetch the new channel's emotes, and the switch must not wait.
    let overlay_app = app.clone();
    tauri::async_runtime::spawn(async move { crate::overlay::sync(&overlay_app).await });

    // Emotes, the room state and the backfill load in the background so chat
    // appears immediately; names render as plain text until the sets arrive.
    let history_login = Login::parse(&remembered).ok();
    tauri::async_runtime::spawn(async move {
        crate::tasks::emotes::refresh_for(&app, &channel_id).await;
        crate::tasks::chat::prime_room_state(&app, &channel_id).await;

        // After the emotes, so backfilled third-party emote names resolve on
        // the first render rather than staying text until something else
        // repaints.
        if let Some(login) = history_login {
            crate::tasks::history::load(&app, &login, channel_id.as_str()).await;
        }
    });

    Ok(())
}

/// Re-point an open synced window at the channel chat just switched to.
///
/// Deliberately never starts the server and never opens a browser: this runs on
/// every channel selection, and a selection must not be able to launch
/// anything. If no window is listening, nothing happens at all.
async fn follow_with_synced_window(state: &AppState, login: &str) {
    let browser = state.settings.read().await.browser.clone();
    if !(browser.sync_window && browser.follow_chat) {
        return;
    }

    let Some(server) = state.watch.read().await.clone() else {
        return;
    };
    let Ok(login) = Login::parse(login) else {
        return;
    };

    if server.set_channel(&login).await > 0 {
        tracing::debug!(channel = login.as_str(), "the synced window followed chat");
    }
}

#[tauri::command]
#[specta::specta]
pub async fn chat_status(state: State<'_, AppState>) -> Result<ChatStatus, ()> {
    Ok(state.chat_status().await)
}

/// Send a message to the currently selected channel.
#[tauri::command]
#[specta::specta]
pub async fn send_message(
    state: State<'_, AppState>,
    text: String,
    reply_to: Option<String>,
) -> Result<(), String> {
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("Nothing to send.".to_owned());
    }
    // Twitch rejects anything longer; say so before spending a request.
    if text.chars().count() > 500 {
        return Err("Twitch messages are limited to 500 characters.".to_owned());
    }

    let reply_to = reply_to.filter(|id| !id.is_empty());

    state
        .send_chat(&text, reply_to.as_deref())
        .await
        .map_err(|error| {
            tracing::warn!(%error, "could not send the chat message");
            match error {
                lurkn_chat::transport::ChatError::Rejected(reason) => reason,
                lurkn_chat::transport::ChatError::RateLimited(secs) => {
                    format!("You are sending too fast. Try again in {secs}s.")
                }
                lurkn_chat::transport::ChatError::Unauthorized => {
                    "Your Twitch login expired.".to_owned()
                }
                _ => "Could not send your message.".to_owned(),
            }
        })
}

/// Everything the user card shows about one chatter.
///
/// Lives with the chat commands because a chat name is the only thing that opens
/// it. The messages half of the card is assembled in the frontend from the log
/// it already holds, so nothing here has to re-read chat history.
#[tauri::command]
#[specta::specta]
pub async fn user_profile(
    state: State<'_, AppState>,
    user_id: String,
) -> Result<lurkn_core::UserProfile, String> {
    if user_id.is_empty() {
        return Err("That account is not one Lurkn can look up.".to_owned());
    }

    let id = lurkn_core::UserId(user_id);

    if let Some(hit) = state.profiles.read().await.get(&id) {
        return Ok(hit.clone());
    }

    let service = state
        .follows_service()
        .await
        .ok_or_else(|| "Log in to look up an account.".to_owned())?;

    let profile = service.profile(&id).await.map_err(|error| {
        tracing::warn!(%error, user_id = id.0, "could not fetch a user profile");
        "Twitch did not answer. Try again in a moment.".to_owned()
    })?;

    let profile = profile.ok_or_else(|| "That account no longer exists.".to_owned())?;

    // Cached for the session: a profile changes rarely, and opening the same
    // card twice should not cost a round trip. The map is bounded by how many
    // distinct names the reader has clicked.
    state.profiles.write().await.insert(id, profile.clone());

    Ok(profile)
}

/// One completable thing for the composer's autocomplete.
///
/// Flattened to what the list needs to draw a row: the emote registry holds
/// sizes, providers and precedence, none of which a suggestion list cares
/// about, and shipping the whole registry over IPC on every channel switch
/// would move a few hundred kilobytes for nothing.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EmoteSuggestion {
    pub name: String,
    /// Small preview for the row.
    pub url: String,
    /// `twitch`, `sevenTv`, `bttv`, `ffz` — shown so duplicate names are
    /// distinguishable.
    pub provider: String,
}

/// The third-party emotes usable in the current channel, for autocomplete.
///
/// Twitch's own emotes are deliberately absent: which of them this account may
/// type needs `user:read:emotes`, a scope Lurkn does not request, and offering
/// emotes that come back rejected would be worse than offering none.
#[tauri::command]
#[specta::specta]
pub async fn chat_emotes(state: State<'_, AppState>) -> Result<Vec<EmoteSuggestion>, ()> {
    Ok(state
        .emotes
        .read()
        .await
        .all()
        .into_iter()
        .map(|emote| EmoteSuggestion {
            name: emote.name,
            url: emote.url_1x.to_string(),
            provider: format!("{:?}", emote.provider).to_lowercase(),
        })
        .collect())
}

/// Suggested channels, and the reason for them.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Suggestions {
    /// The categories these came from, most followed first, as `(name, follows)`
    /// pairs — shown so the list says why it contains what it does.
    pub because: Vec<SuggestionBasis>,
    pub channels: Vec<lurkn_core::ChannelHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionBasis {
    /// UNTRUSTED: a category name from Twitch.
    pub name: String,
    pub follows: u32,
}

/// Live channels in the categories and languages your follows use.
///
/// Cached for five minutes: the palette is opened often, and the busiest
/// streams in a category do not change meaningfully in that time. The rate
/// limit is shared with everything else Lurkn does.
#[tauri::command]
#[specta::specta]
pub async fn suggested_channels(state: State<'_, AppState>) -> Result<Suggestions, String> {
    const TTL: std::time::Duration = std::time::Duration::from_secs(300);
    // A short list: this is a nudge, not a directory.
    const LIMIT: usize = 8;

    if let Some((at, cached)) = state.suggestions.read().await.as_ref() {
        if at.elapsed() < TTL {
            return Ok(cached.clone());
        }
    }

    let follows = state.follows.read().await.channels.clone();

    let service = state
        .follows_service()
        .await
        .ok_or_else(|| "Log in to see suggestions.".to_owned())?;

    let channels = service.suggested(&follows, LIMIT).await.map_err(|error| {
        tracing::warn!(%error, "could not fetch suggestions");
        "Twitch did not answer. Try again in a moment.".to_owned()
    })?;

    let because = lurkn_twitch::suggest::categories(&follows)
        .into_iter()
        .map(|basis| SuggestionBasis {
            name: basis.name,
            follows: basis.follows as u32,
        })
        .collect();

    let answer = Suggestions { because, channels };
    tracing::debug!(
        channels = answer.channels.len(),
        categories = answer.because.len(),
        "fetched suggestions"
    );
    *state.suggestions.write().await = Some((std::time::Instant::now(), answer.clone()));
    Ok(answer)
}

/// The reader's own channel, fetched the way any other channel is.
///
/// Needed as its own call because nobody follows themselves: the follow list
/// never contains it, so without this there is no way to open your own chat
/// short of typing your own name.
#[tauri::command]
#[specta::specta]
pub async fn my_channel(
    state: State<'_, AppState>,
) -> Result<Option<lurkn_core::FollowedChannel>, String> {
    let login = match &*state.auth.read().await {
        crate::commands::auth::AuthStatus::Authenticated { login, .. } => login.clone(),
        _ => return Ok(None),
    };

    let Ok(parsed) = Login::parse(&login) else {
        return Ok(None);
    };

    load_unfollowed(&state, &parsed).await.map(Some)
}

/// Whether the reader's own stream is on air.
#[tauri::command]
#[specta::specta]
pub async fn own_stream_live(state: State<'_, AppState>) -> Result<bool, ()> {
    Ok(*state.own_stream_live.read().await)
}

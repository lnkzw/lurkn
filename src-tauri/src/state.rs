//! The single source of truth. See `docs/architecture.md` §9.1.

use std::sync::Arc;

use lurkn_auth::{IdEndpoints, Session};
use lurkn_browser::BrowserLauncher;
use lurkn_chat::transport::{ChatConnState, ChatError, ChatEvent};
use lurkn_chat::ChatHandle;
use lurkn_core::{Login, UserId};
use lurkn_emotes::{EmoteFetcher, EmoteRegistry};
use lurkn_store::Store;
use lurkn_twitch::{BadgeMap, BadgeService, FollowsService, HelixClient};
use tokio::sync::{broadcast, Mutex, Notify, RwLock};

use crate::chat_api::HelixChatApi;
use crate::commands::auth::AuthStatus;
use crate::commands::chat::ChatStatus;
use crate::tasks::follows::FollowsState;

/// What a URL is being opened for.
///
/// A stream obeys the browser settings — app mode, kiosk, its own window. A
/// link from chat does not: those settings exist to make a stream watchable,
/// and applying them to someone's pasted link drops the reader into a
/// chromeless window with no way back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAs {
    Stream,
    Link,
}

pub struct AppState {
    /// Used for the log folder; stream URLs go through `browser` instead.
    opener: tauri::AppHandle,
    pub session: Arc<Session>,
    pub auth: RwLock<AuthStatus>,
    /// The in-flight device-code poll, so a second login attempt replaces the
    /// first rather than racing it.
    pub login_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    pub browser: Arc<dyn BrowserLauncher>,
    /// The watch-window server, started the first time it is needed and then
    /// left running. `None` means the experiment has never been used.
    pub watch: RwLock<Option<lurkn_watch::WatchServer>>,
    /// How Lurkn's chat looks, as the frontend last reported it, for the chat
    /// beside the player in the synced window.
    pub watch_look: RwLock<Option<lurkn_watch::WatchLook>>,
    /// The chat overlay for OBS, while it is switched on. See `crate::overlay`.
    pub overlay: RwLock<Option<lurkn_watch::overlay::OverlayServer>>,
    /// Why the overlay is not running although it is switched on.
    pub overlay_error: RwLock<Option<String>>,
    /// The channel the overlay shows while it runs: the reader's own, or the
    /// open one, per `OverlaySettings::source`. The own channel is subscribed
    /// alongside the open one.
    pub overlay_channel: RwLock<Option<UserId>>,
    /// Emotes and badges for the overlay's channel, kept apart from the open
    /// channel's so switching chat never changes what the stream shows.
    pub overlay_emotes: RwLock<EmoteRegistry>,
    pub overlay_badges: RwLock<BadgeMap>,
    /// One overlay reconfiguration at a time: a slider dragged across its
    /// range sends a burst of settings changes.
    pub overlay_sync: Mutex<()>,
    /// Held while the subscribed channel set is worked out and sent, so a
    /// channel switch and an overlay change cannot each send a set missing
    /// the other's channel.
    channels_lock: Mutex<()>,
    pub store: Arc<Store>,
    pub follows: RwLock<FollowsState>,
    /// The chat supervisor, started once the user is logged in.
    pub chat: RwLock<Option<ChatHandle>>,
    pub chat_events: broadcast::Sender<ChatEvent>,
    pub chat_state: RwLock<ChatConnState>,
    pub current_channel: RwLock<Option<(Login, UserId)>>,
    /// Third-party emotes for the current channel. Replaced wholesale on switch.
    pub emotes: RwLock<EmoteRegistry>,
    pub emote_fetcher: EmoteFetcher,
    /// Global badge sets, shared across channels.
    pub settings: RwLock<lurkn_core::Settings>,
    /// Highlights and ignores, compiled from `settings` whenever it changes so
    /// no pattern is rebuilt per message.
    pub rules: RwLock<Arc<lurkn_core::ChatRules>>,
    /// The theme file in effect. Kept by `crate::theme::run`.
    pub theme: RwLock<crate::theme::ThemeState>,
    /// Wakes `crate::theme::run` early, so a newly picked theme applies at once.
    pub theme_wake: Notify,
    /// The current channel's chat restrictions.
    pub room_state: RwLock<lurkn_core::RoomState>,
    /// Channels the reader moderates, fetched once per session.
    pub moderated: RwLock<Option<Vec<crate::commands::moderation::ModeratedChannel>>>,
    /// Whether the reader's own stream is on air, per Twitch's own event.
    pub own_stream_live: RwLock<bool>,
    /// The open channel when the reader does not follow it, kept current by the
    /// same poll that keeps the follow list current.
    pub unfollowed: RwLock<Option<lurkn_core::FollowedChannel>>,
    /// Suggested channels, with when they were fetched. See
    /// `commands::chat::suggested_channels`.
    pub suggestions: RwLock<Option<(std::time::Instant, crate::commands::chat::Suggestions)>>,
    /// Profiles behind chat user cards, cached for the session. See
    /// `commands::chat::user_profile`.
    pub profiles: RwLock<std::collections::HashMap<UserId, lurkn_core::UserProfile>>,
    pub global_badges: RwLock<BadgeMap>,
    /// Badges for the current channel; replaced on switch.
    pub channel_badges: RwLock<BadgeMap>,
    http: reqwest::Client,
    helix: HelixClient,
}

impl AppState {
    pub fn new(app: tauri::AppHandle, browser: Arc<dyn BrowserLauncher>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Lurkn/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();

        let client_id = lurkn_auth::client_id();
        let session = Arc::new(Session::new(
            http.clone(),
            IdEndpoints::default(),
            client_id.clone(),
            lurkn_auth::default_storage(),
        ));

        let store = Arc::new(open_store());
        let settings = store.load_settings();
        let rules = Arc::new(lurkn_core::ChatRules::compile(&settings));

        let helix = HelixClient::new(http.clone(), Arc::clone(&session), client_id);
        // Chat is bursty; a generous buffer keeps a slow consumer from dropping
        // messages the moment a big channel gets busy.
        let (chat_events, _) = broadcast::channel(1024);

        Self {
            opener: app,
            session,
            auth: RwLock::new(AuthStatus::LoggedOut),
            login_task: Mutex::new(None),
            browser,
            watch: RwLock::new(None),
            watch_look: RwLock::new(None),
            overlay: RwLock::new(None),
            overlay_error: RwLock::new(None),
            overlay_channel: RwLock::new(None),
            overlay_emotes: RwLock::new(EmoteRegistry::default()),
            overlay_badges: RwLock::new(BadgeMap::new()),
            overlay_sync: Mutex::new(()),
            channels_lock: Mutex::new(()),
            store,
            follows: RwLock::new(FollowsState::default()),
            chat: RwLock::new(None),
            chat_events,
            chat_state: RwLock::new(ChatConnState::Disconnected),
            current_channel: RwLock::new(None),
            emotes: RwLock::new(EmoteRegistry::default()),
            emote_fetcher: EmoteFetcher::new(http.clone()),
            settings: RwLock::new(settings),
            rules: RwLock::new(rules),
            theme: RwLock::new(crate::theme::ThemeState::default()),
            theme_wake: Notify::new(),
            room_state: RwLock::new(lurkn_core::RoomState::default()),
            profiles: RwLock::new(std::collections::HashMap::new()),
            suggestions: RwLock::new(None),
            unfollowed: RwLock::new(None),
            own_stream_live: RwLock::new(false),
            moderated: RwLock::new(None),
            global_badges: RwLock::new(BadgeMap::new()),
            channel_badges: RwLock::new(BadgeMap::new()),
            http,
            helix,
        }
    }

    /// Start the chat supervisor for the logged-in user. Idempotent.
    pub async fn start_chat(&self) {
        let Some(viewer_id) = self.current_user_id().await else {
            return;
        };
        if self.chat.read().await.is_some() {
            return;
        }

        let api = Arc::new(HelixChatApi::new(
            self.http.clone(),
            Arc::clone(&self.session),
            lurkn_auth::client_id(),
            viewer_id,
        ));

        let handle = lurkn_chat::spawn(api, self.chat_events.clone());
        *self.chat.write().await = Some(handle);
        tracing::debug!("chat supervisor started");
    }

    pub async fn stop_chat(&self) {
        if let Some(handle) = self.chat.write().await.take() {
            handle.shutdown().await;
        }
        *self.current_channel.write().await = None;
        *self.chat_state.write().await = ChatConnState::Disconnected;
    }

    /// Point chat at a channel, or at nothing.
    pub async fn set_current_channel(
        &self,
        channel: Option<(Login, UserId)>,
    ) -> Result<(), ChatError> {
        let _held = self.channels_lock.lock().await;
        let open = channel.as_ref().map(|(_, id)| id.clone());
        self.send_channels(open).await?;
        let login = channel.as_ref().map(|(login, _)| login.clone());
        *self.current_channel.write().await = channel;

        // The synced window's chat follows Lurkn's, whatever the player shows.
        if let Some(server) = crate::watch::running(self).await {
            server.set_chat_channel(login.as_ref()).await;
        }
        Ok(())
    }

    /// Re-send the subscribed set after the overlay's channel changed.
    pub async fn resync_channels(&self) -> Result<(), ChatError> {
        let _held = self.channels_lock.lock().await;
        let open = self
            .current_channel
            .read()
            .await
            .as_ref()
            .map(|(_, id)| id.clone());
        self.send_channels(open).await
    }

    /// Subscribe to the open channel and, while the overlay shows the
    /// reader's own, that one too. Callers hold `channels_lock`.
    async fn send_channels(&self, open: Option<UserId>) -> Result<(), ChatError> {
        self.start_chat().await;

        let handle = self.chat.read().await.clone();
        let Some(handle) = handle else {
            return Err(ChatError::NotConnected);
        };

        let mut channels: Vec<UserId> = open.into_iter().collect();
        // An overlay on the open channel needs nothing extra, and the channel
        // it showed until now is the one being switched away from.
        let own = self.settings.read().await.overlay.source == lurkn_core::OverlaySource::Own;
        if let Some(overlay) = self.overlay_channel.read().await.clone() {
            if own && !channels.contains(&overlay) {
                channels.push(overlay);
            }
        }
        handle.set_channels(channels).await
    }

    pub async fn send_chat(&self, text: &str, reply_to: Option<&str>) -> Result<(), ChatError> {
        let Some((_, channel_id)) = self.current_channel.read().await.clone() else {
            return Err(ChatError::NotConnected);
        };
        let handle = self.chat.read().await.clone();
        let Some(handle) = handle else {
            return Err(ChatError::NotConnected);
        };
        handle.send(channel_id.as_str(), text, reply_to).await
    }

    pub async fn chat_status(&self) -> ChatStatus {
        let current = self.current_channel.read().await.clone();
        let (state, detail) = match &*self.chat_state.read().await {
            ChatConnState::Disconnected => ("disconnected", None),
            ChatConnState::Connecting => ("connecting", None),
            ChatConnState::Connected => ("connected", None),
            ChatConnState::Reconnecting { attempt } => {
                ("reconnecting", Some(format!("attempt {attempt}")))
            }
            ChatConnState::Failed { reason } => ("failed", Some(reason.clone())),
        };

        let room = self.room_state.read().await.clone();

        ChatStatus {
            channel_login: current.as_ref().map(|(l, _)| l.to_string()),
            channel_id: current.as_ref().map(|(_, id)| id.to_string()),
            state: state.to_owned(),
            detail,
            room_labels: room.labels(),
            room,
        }
    }

    /// The logged-in user's id, or `None` when logged out.
    pub async fn current_user_id(&self) -> Option<String> {
        match &*self.auth.read().await {
            AuthStatus::Authenticated { user_id, .. } => Some(user_id.clone()),
            _ => None,
        }
    }

    /// Open a web URL the way the user asked for in settings.
    ///
    /// Everything that opens a page goes through here — the stream button, the
    /// sidebar, a link in chat — so the browser choice applies to all of them.
    ///
    /// A choice that cannot be honoured is reported rather than quietly
    /// replaced by the system browser: a stream opening in the wrong browser,
    /// with no explanation, is harder to understand than an error naming what
    /// went wrong.
    pub async fn open_url(
        &self,
        url: &url::Url,
        purpose: OpenAs,
    ) -> Result<(), lurkn_browser::BrowserError> {
        use lurkn_browser::{launch, BrowserError};
        use lurkn_core::{BrowserChoice, LaunchMode};

        // The last gate before a string reaches the OS, whichever path it takes.
        lurkn_browser::check_scheme(url)?;

        let mut browser = self.settings.read().await.browser.clone();

        // The browser settings describe how to watch a stream: app mode, kiosk,
        // a window of its own, or a custom command that may not even be a
        // browser. None of that belongs on a link someone pasted in chat, which
        // should open the way a link opens anywhere else on the desktop.
        if purpose == OpenAs::Link {
            browser.mode = LaunchMode::Normal;
            browser.new_window = false;
            if matches!(browser.choice, BrowserChoice::Command { .. }) {
                browser.choice = BrowserChoice::System;
            }
        }

        let wanted = match &browser.choice {
            BrowserChoice::Command { line } => {
                let launch = launch::for_command(line, url)?;
                tracing::info!(program = %launch.program, "opening with a custom command");
                return launch::spawn(&launch);
            }
            // Plain mode through the system's own handler is exactly what the
            // desktop would do; a mode needs a concrete browser to pass flags to.
            BrowserChoice::System if browser.mode == LaunchMode::Normal => {
                return self.browser.open_url(url);
            }
            BrowserChoice::System => None,
            BrowserChoice::Installed { id } => Some(id.clone()),
        };

        let installed = crate::browser::installed();
        let id = match wanted {
            Some(id) => id,
            None => match lurkn_browser::default_id(&installed) {
                Some(id) => id,
                // The mode cannot be applied, but the stream can still open.
                None => {
                    tracing::debug!("the default browser is not one Lurkn can start itself");
                    return self.browser.open_url(url);
                }
            },
        };

        let Some(found) = installed.into_iter().find(|b| b.id == id) else {
            tracing::warn!(id, "the chosen browser is no longer installed");
            return Err(BrowserError::Missing);
        };

        let launch = launch::for_browser(&found, url, browser.mode, browser.new_window)?;
        tracing::info!(
            browser = %found.name,
            mode = ?launch::effective_mode(found.family, browser.mode),
            ?purpose,
            "opening a page"
        );
        launch::spawn(&launch)
    }

    /// The watch server, started on first use.
    ///
    /// Starting it binds a loopback port, so it is not done until the user has
    /// actually turned the experiment on and opened something.
    pub async fn watch_server(&self) -> Result<lurkn_watch::WatchServer, lurkn_watch::WatchError> {
        if let Some(server) = self.watch.read().await.as_ref() {
            return Ok(server.clone());
        }

        let mut slot = self.watch.write().await;
        // Another caller may have started it while this one waited.
        if let Some(server) = slot.as_ref() {
            return Ok(server.clone());
        }

        let server = lurkn_watch::WatchServer::start().await?;
        *slot = Some(server.clone());
        drop(slot);

        crate::watch::prime(&self.opener, &server).await;
        Ok(server)
    }

    /// Hand a local path to the desktop's file manager.
    pub fn open_path(&self, path: &std::path::Path) -> Result<(), String> {
        let url = url::Url::from_directory_path(path)
            .map_err(|()| "That path cannot be opened.".to_owned())?;
        // Not the BrowserLauncher: that deliberately refuses file:// URLs.
        #[cfg(target_os = "linux")]
        {
            lurkn_browser::launch::spawn(&lurkn_browser::launch::for_desktop(url.as_str()))
                .map_err(|e| e.to_string())
        }

        #[cfg(not(target_os = "linux"))]
        {
            use tauri_plugin_opener::OpenerExt;
            self.opener
                .opener()
                .open_url(url.as_str(), None::<&str>)
                .map_err(|e| e.to_string())
        }
    }

    /// The shared HTTP client, for the few callers outside the Twitch services.
    pub fn http(&self) -> reqwest::Client {
        self.http.clone()
    }

    /// A chat API handle, for calls made outside the socket.
    pub fn chat_api(&self, viewer_id: String) -> HelixChatApi {
        HelixChatApi::new(
            self.http.clone(),
            Arc::clone(&self.session),
            lurkn_auth::client_id(),
            viewer_id,
        )
    }

    /// The raw Helix client, for services built outside this module.
    pub fn helix(&self) -> HelixClient {
        self.helix.clone()
    }

    /// Whether the session was granted the moderation bundle.
    pub async fn has_moderation(&self) -> bool {
        match &*self.auth.read().await {
            crate::commands::auth::AuthStatus::Authenticated { moderation, .. } => *moderation,
            _ => false,
        }
    }

    pub fn badges(&self) -> BadgeService {
        BadgeService::new(self.helix.clone())
    }

    /// A follows service, but only while logged in.
    pub async fn follows_service(&self) -> Option<FollowsService> {
        self.current_user_id().await?;
        Some(FollowsService::new(self.helix.clone()))
    }
}

/// Open the on-disk cache, degrading to an in-memory one.
///
/// A cache is disposable (`docs/architecture.md` §10), so a disk problem costs a
/// refetch, never a login. The final fallback constructs a `Store` from an
/// in-memory SQLite connection, which only fails if the process is out of memory;
/// at that point there is nothing useful left to do.
fn open_store() -> Store {
    if let Some(path) = Store::default_path() {
        match Store::open(&path) {
            Ok(store) => return store,
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "could not open the cache on disk");
            }
        }
    }

    match Store::open_in_memory() {
        Ok(store) => {
            tracing::warn!("running with an in-memory cache; it will not survive a restart");
            store
        }
        Err(error) => {
            tracing::error!(%error, "could not open any cache");
            std::process::abort();
        }
    }
}

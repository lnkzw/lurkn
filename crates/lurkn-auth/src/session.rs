use std::sync::Arc;

use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::device::{DeviceFlow, DevicePrompt};
use crate::endpoints::IdEndpoints;
use crate::error::{AuthError, Result};
use crate::refresh::refresh;
use crate::storage::{StorageKind, TokenStorage};
use crate::tokens::TokenSet;
use crate::validate::{validate, ValidatedToken};

/// What the frontend is told. Never carries a token (`docs/architecture.md` §11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    LoggedOut,
    /// Showing the user their code.
    AwaitingUser {
        user_code: String,
        verification_uri: String,
        expires_at: OffsetDateTime,
    },
    Authenticated {
        user_id: String,
        login: String,
    },
    /// Still showing cached data, but the session needs renewing. Rendered as an
    /// inline banner, not a modal (`docs/architecture.md` §3.4).
    Expired,
}

/// Owns the token and keeps it fresh.
#[derive(Debug)]
pub struct Session {
    http: reqwest::Client,
    endpoints: IdEndpoints,
    client_id: String,
    storage: Box<dyn TokenStorage>,
    tokens: Arc<RwLock<Option<TokenSet>>>,
}

impl Session {
    pub fn new(
        http: reqwest::Client,
        endpoints: IdEndpoints,
        client_id: String,
        storage: Box<dyn TokenStorage>,
    ) -> Self {
        Self {
            http,
            endpoints,
            client_id,
            storage,
            tokens: Arc::new(RwLock::new(None)),
        }
    }

    pub fn storage_kind(&self) -> StorageKind {
        self.storage.kind()
    }

    pub fn device_flow(&self) -> DeviceFlow {
        DeviceFlow::new(
            self.http.clone(),
            self.endpoints.clone(),
            self.client_id.clone(),
        )
    }

    /// Start a login. The caller polls with [`Self::complete_login`].
    pub async fn begin_login(&self) -> Result<DevicePrompt> {
        self.device_flow().start(&[]).await
    }

    /// Log in asking for optional scopes too, for a feature the user turned on.
    pub async fn begin_login_with(&self, extra: &[&str]) -> Result<DevicePrompt> {
        self.device_flow().start(extra).await
    }

    /// One poll tick. `Err(AuthorizationPending)` means "keep going".
    pub async fn complete_login(&self, device_code: &str) -> Result<ValidatedToken> {
        let tokens = self.device_flow().poll_once(device_code).await?;
        self.adopt(tokens).await
    }

    /// Restore a saved session at startup, if there is one and it still works.
    pub async fn restore(&self) -> Result<Option<ValidatedToken>> {
        let Some(saved) = self.storage.load()? else {
            return Ok(None);
        };

        let now = OffsetDateTime::now_utc();
        let usable = if saved.needs_refresh(now) {
            if saved.is_beyond_refresh_window(now) {
                // Past the 30-day public-client ceiling: refreshing cannot help.
                tracing::info!("saved session is past the refresh window; login required");
                return Err(AuthError::RefreshRejected);
            }
            self.refresh_now(&saved).await?
        } else {
            saved
        };

        match self.adopt(usable).await {
            Ok(identity) => Ok(Some(identity)),
            // The token looked fine locally but Twitch disagrees — e.g. revoked
            // from the Twitch connections page.
            Err(e) if e.requires_reauth() => Err(e),
            Err(e) => Err(e),
        }
    }

    /// Return a valid access token, refreshing first if it is due.
    pub async fn access_token(&self) -> Result<String> {
        let current = {
            let guard = self.tokens.read().await;
            guard.clone().ok_or(AuthError::RefreshRejected)?
        };

        if !current.needs_refresh(OffsetDateTime::now_utc()) {
            return Ok(current.access_token);
        }

        let renewed = self.refresh_now(&current).await?;
        let access = renewed.access_token.clone();
        self.persist(renewed).await?;
        Ok(access)
    }

    /// Force a refresh, e.g. after an unexpected 401.
    pub async fn force_refresh(&self) -> Result<()> {
        let current = {
            let guard = self.tokens.read().await;
            guard.clone().ok_or(AuthError::RefreshRejected)?
        };
        let renewed = self.refresh_now(&current).await?;
        self.persist(renewed).await
    }

    pub async fn logout(&self) -> Result<()> {
        *self.tokens.write().await = None;
        self.storage.clear()
    }

    async fn refresh_now(&self, current: &TokenSet) -> Result<TokenSet> {
        tracing::debug!("refreshing the Twitch access token");
        refresh(
            &self.http,
            &self.endpoints,
            &self.client_id,
            &current.refresh_token,
        )
        .await
    }

    /// Persist a token set and confirm with Twitch who it belongs to.
    async fn adopt(&self, tokens: TokenSet) -> Result<ValidatedToken> {
        let identity = validate(&self.http, &self.endpoints, &tokens.access_token).await?;
        self.persist(tokens).await?;
        Ok(identity)
    }

    async fn persist(&self, tokens: TokenSet) -> Result<()> {
        // Twitch rotates the refresh token on every refresh, so the stored copy
        // must be replaced, not merged.
        self.storage.store(&tokens)?;
        *self.tokens.write().await = Some(tokens);
        Ok(())
    }
}

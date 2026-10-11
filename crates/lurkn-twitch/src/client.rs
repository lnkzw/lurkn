use std::sync::Arc;

use lurkn_auth::Session;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use url::Url;

use crate::error::{HelixError, Result};

/// Twitch's paged envelope.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    #[serde(default)]
    pub pagination: Pagination,
}

#[derive(Debug, Default, Deserialize)]
pub struct Pagination {
    pub cursor: Option<String>,
}

/// Twitch's error body. Only `message` is read — the HTTP status is authoritative.
#[derive(Debug, Deserialize)]
struct HelixErrorBody {
    #[serde(default)]
    message: String,
}

/// The only thing in Lurkn that performs Helix requests.
///
/// Owns the authorization header, the refresh-on-401 retry and pagination, so no
/// caller has to. See `docs/architecture.md` §4.1.
#[derive(Debug, Clone)]
pub struct HelixClient {
    http: reqwest::Client,
    session: Arc<Session>,
    client_id: String,
}

impl HelixClient {
    pub fn new(http: reqwest::Client, session: Arc<Session>, client_id: String) -> Self {
        Self {
            http,
            session,
            client_id,
        }
    }

    async fn send_once(&self, url: &Url, token: &str) -> Result<reqwest::Response> {
        self.http
            .get(url.clone())
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(HelixError::from)
    }

    /// GET a Helix endpoint, refreshing once on a 401.
    ///
    /// A 401 is expected from time to time: the clock-based refresh in
    /// `docs/architecture.md` §3.4 cannot catch a token revoked mid-session.
    pub async fn get<T: DeserializeOwned>(&self, url: &Url) -> Result<T> {
        let token = self.session.access_token().await?;
        let mut response = self.send_once(url, &token).await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            tracing::debug!("Helix returned 401; refreshing the token and retrying once");
            self.session.force_refresh().await.map_err(|error| {
                if error.requires_reauth() {
                    HelixError::Unauthorized
                } else {
                    HelixError::Auth(error)
                }
            })?;
            let token = self.session.access_token().await?;
            response = self.send_once(url, &token).await?;
        }

        Self::parse(response).await
    }

    /// Send a write, refreshing once on a 401 exactly as [`Self::get`] does.
    ///
    /// The moderation endpoints answer 204 with no body, so there is nothing to
    /// deserialise — only a status to check and, when it failed, a message from
    /// Twitch worth passing on.
    async fn write_once(
        &self,
        method: reqwest::Method,
        url: &Url,
        body: Option<&serde_json::Value>,
        token: &str,
    ) -> Result<reqwest::Response> {
        let mut request = self
            .http
            .request(method, url.clone())
            .header("Client-Id", &self.client_id)
            .header("Authorization", format!("Bearer {token}"));

        if let Some(body) = body {
            request = request.json(body);
        }

        request.send().await.map_err(HelixError::from)
    }

    async fn write(
        &self,
        method: reqwest::Method,
        url: &Url,
        body: Option<&serde_json::Value>,
    ) -> Result<()> {
        let token = self.session.access_token().await?;
        let mut response = self.write_once(method.clone(), url, body, &token).await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            tracing::debug!("Helix returned 401 on a write; refreshing and retrying once");
            self.session.force_refresh().await.map_err(|error| {
                if error.requires_reauth() {
                    HelixError::Unauthorized
                } else {
                    HelixError::Auth(error)
                }
            })?;
            let token = self.session.access_token().await?;
            response = self.write_once(method, url, body, &token).await?;
        }

        Self::check(response).await
    }

    pub async fn post_empty(&self, url: &Url, body: &serde_json::Value) -> Result<()> {
        self.write(reqwest::Method::POST, url, Some(body)).await
    }

    pub async fn patch_empty(&self, url: &Url, body: &serde_json::Value) -> Result<()> {
        self.write(reqwest::Method::PATCH, url, Some(body)).await
    }

    pub async fn delete(&self, url: &Url) -> Result<()> {
        self.write(reqwest::Method::DELETE, url, None).await
    }

    /// A response with nothing to read: success, or Twitch's reason.
    async fn check(response: reqwest::Response) -> Result<()> {
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(HelixError::Unauthorized);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(HelixError::RateLimited {
                retry_after_secs: 30,
            });
        }

        let body = response.text().await.unwrap_or_default();
        Err(match serde_json::from_str::<HelixErrorBody>(&body) {
            Ok(err) if !err.message.is_empty() => HelixError::Twitch {
                status: status.as_u16(),
                message: err.message,
            },
            _ => HelixError::Twitch {
                status: status.as_u16(),
                message: body,
            },
        })
    }

    async fn parse<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Twitch reports when the point bucket refills, as a unix timestamp.
            let retry_after_secs = response
                .headers()
                .get("Ratelimit-Reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<i64>().ok())
                .map(|reset| {
                    (reset - time::OffsetDateTime::now_utc().unix_timestamp()).clamp(1, 300) as u64
                })
                .unwrap_or(30);
            return Err(HelixError::RateLimited { retry_after_secs });
        }

        let body = response.text().await?;

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(HelixError::Unauthorized);
        }

        if !status.is_success() {
            return Err(match serde_json::from_str::<HelixErrorBody>(&body) {
                Ok(err) if !err.message.is_empty() => HelixError::Twitch {
                    status: status.as_u16(),
                    message: err.message,
                },
                _ => HelixError::Twitch {
                    status: status.as_u16(),
                    message: body,
                },
            });
        }

        serde_json::from_str(&body).map_err(|e| HelixError::Malformed(e.to_string()))
    }

    /// Follow `pagination.cursor` until Twitch stops handing one back.
    ///
    /// `max_pages` is a guard, not a limit anyone should hit: 100 pages is 10,000
    /// follows. Without it a cursor bug would loop forever.
    pub async fn get_all<T: DeserializeOwned>(
        &self,
        build_url: impl Fn(Option<&str>) -> Url,
        max_pages: usize,
    ) -> Result<Vec<T>> {
        let mut items = Vec::new();
        let mut cursor: Option<String> = None;

        for page_number in 0..max_pages {
            let page: Page<T> = self.get(&build_url(cursor.as_deref())).await?;
            let was_empty = page.data.is_empty();
            items.extend(page.data);

            cursor = page.pagination.cursor.filter(|c| !c.is_empty());
            if cursor.is_none() || was_empty {
                return Ok(items);
            }

            if page_number + 1 == max_pages {
                tracing::warn!(max_pages, "stopped paginating at the page guard");
            }
        }

        Ok(items)
    }
}

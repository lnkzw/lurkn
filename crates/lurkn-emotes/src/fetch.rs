//! Fetching emote sets from the third-party providers.
//!
//! Each provider is independent: a failure is logged and skipped so the others
//! still render. None of them require authentication.

use std::time::Duration;

use lurkn_core::{Emote, EmoteScope};

use crate::providers::{bttv, ffz, seventv};

/// Per-provider timeout. These are community services with no SLA, so a slow one
/// must not hold up the channel switch.
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct EmoteFetcher {
    http: reqwest::Client,
    bases: Bases,
}

/// Endpoint bases, overridable so tests can point at a mock server.
#[derive(Debug, Clone)]
pub struct Bases {
    pub seventv: String,
    pub bttv: String,
    pub ffz: String,
}

impl Default for Bases {
    fn default() -> Self {
        Self {
            seventv: "https://7tv.io".into(),
            bttv: "https://api.betterttv.net".into(),
            ffz: "https://api.frankerfacez.com".into(),
        }
    }
}

/// What one provider returned, so a caller can report partial results honestly.
#[derive(Debug, Clone)]
pub struct ProviderOutcome {
    pub emotes: Vec<Emote>,
    pub failed: bool,
}

impl EmoteFetcher {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            bases: Bases::default(),
        }
    }

    pub fn with_bases(http: reqwest::Client, bases: Bases) -> Self {
        Self { http, bases }
    }

    async fn get(&self, url: String) -> Option<String> {
        let response = self.http.get(&url).timeout(TIMEOUT).send().await.ok()?;
        if !response.status().is_success() {
            tracing::debug!(%url, status = %response.status(), "emote provider declined");
            return None;
        }
        response.text().await.ok()
    }

    pub async fn seventv_channel(&self, broadcaster_id: &str) -> ProviderOutcome {
        let url = format!("{}/v3/users/twitch/{broadcaster_id}", self.bases.seventv);
        match self.get(url).await {
            Some(body) => match serde_json::from_str::<seventv::UserResponse>(&body) {
                Ok(parsed) => ProviderOutcome {
                    emotes: parsed
                        .emote_set
                        .map(|set| seventv::to_emotes(&set, EmoteScope::Channel))
                        .unwrap_or_default(),
                    failed: false,
                },
                Err(error) => {
                    tracing::debug!(%error, "7TV channel response was unreadable");
                    ProviderOutcome {
                        emotes: Vec::new(),
                        failed: true,
                    }
                }
            },
            // A channel with no 7TV set answers 404; that is not a failure.
            None => ProviderOutcome {
                emotes: Vec::new(),
                failed: false,
            },
        }
    }

    pub async fn seventv_global(&self) -> ProviderOutcome {
        let url = format!("{}/v3/emote-sets/global", self.bases.seventv);
        parse_or_fail(self.get(url).await, |body| {
            serde_json::from_str::<seventv::EmoteSet>(body)
                .ok()
                .map(|set| seventv::to_emotes(&set, EmoteScope::Global))
        })
    }

    pub async fn bttv_channel(&self, broadcaster_id: &str) -> ProviderOutcome {
        let url = format!("{}/3/cached/users/twitch/{broadcaster_id}", self.bases.bttv);
        match self.get(url).await {
            Some(body) => match serde_json::from_str::<bttv::ChannelResponse>(&body) {
                Ok(parsed) => ProviderOutcome {
                    emotes: bttv::to_emotes(parsed.all(), EmoteScope::Channel),
                    failed: false,
                },
                Err(error) => {
                    tracing::debug!(%error, "BTTV channel response was unreadable");
                    ProviderOutcome {
                        emotes: Vec::new(),
                        failed: true,
                    }
                }
            },
            None => ProviderOutcome {
                emotes: Vec::new(),
                failed: false,
            },
        }
    }

    pub async fn bttv_global(&self) -> ProviderOutcome {
        let url = format!("{}/3/cached/emotes/global", self.bases.bttv);
        parse_or_fail(self.get(url).await, |body| {
            bttv::parse_global(body)
                .ok()
                .map(|list| bttv::to_emotes(list.iter(), EmoteScope::Global))
        })
    }

    pub async fn ffz_channel(&self, broadcaster_id: &str) -> ProviderOutcome {
        let url = format!("{}/v1/room/id/{broadcaster_id}", self.bases.ffz);
        match self.get(url).await {
            Some(body) => match serde_json::from_str::<ffz::RoomResponse>(&body) {
                Ok(parsed) => ProviderOutcome {
                    emotes: ffz::room_emotes(&parsed),
                    failed: false,
                },
                Err(error) => {
                    tracing::debug!(%error, "FFZ room response was unreadable");
                    ProviderOutcome {
                        emotes: Vec::new(),
                        failed: true,
                    }
                }
            },
            None => ProviderOutcome {
                emotes: Vec::new(),
                failed: false,
            },
        }
    }

    pub async fn ffz_global(&self) -> ProviderOutcome {
        let url = format!("{}/v1/set/global", self.bases.ffz);
        parse_or_fail(self.get(url).await, |body| {
            serde_json::from_str::<ffz::GlobalResponse>(body)
                .ok()
                .map(|r| ffz::global_emotes(&r))
        })
    }

    /// Every global set, fetched concurrently. Global sets change rarely and are
    /// shared across channels, so callers cache these for a day.
    pub async fn all_global(&self) -> Vec<Vec<Emote>> {
        let (seven, bttv, ffz) =
            tokio::join!(self.seventv_global(), self.bttv_global(), self.ffz_global());
        vec![seven.emotes, bttv.emotes, ffz.emotes]
    }

    /// Every channel set, fetched concurrently.
    pub async fn all_channel(&self, broadcaster_id: &str) -> Vec<Vec<Emote>> {
        let (seven, bttv, ffz) = tokio::join!(
            self.seventv_channel(broadcaster_id),
            self.bttv_channel(broadcaster_id),
            self.ffz_channel(broadcaster_id),
        );
        vec![seven.emotes, bttv.emotes, ffz.emotes]
    }
}

fn parse_or_fail(
    body: Option<String>,
    parse: impl Fn(&str) -> Option<Vec<Emote>>,
) -> ProviderOutcome {
    match body.as_deref().and_then(parse) {
        Some(emotes) => ProviderOutcome {
            emotes,
            failed: false,
        },
        None => ProviderOutcome {
            emotes: Vec::new(),
            failed: body.is_some(),
        },
    }
}

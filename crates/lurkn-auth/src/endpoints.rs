use url::Url;

/// Twitch's identity host. Distinct from Helix (`api.twitch.tv`), which is owned
/// by `lurkn-twitch` — this crate is the only one that talks to `id.twitch.tv`.
pub const ID_BASE: &str = "https://id.twitch.tv/";

pub const GRANT_DEVICE_CODE: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub const GRANT_REFRESH_TOKEN: &str = "refresh_token";

/// Base URLs for the identity endpoints, overridable so tests can point at a
/// mock server.
#[derive(Debug, Clone)]
pub struct IdEndpoints {
    base: Url,
}

impl Default for IdEndpoints {
    fn default() -> Self {
        Self::new(ID_BASE).unwrap_or_else(|_| unreachable!("ID_BASE is a valid absolute URL"))
    }
}

impl IdEndpoints {
    pub fn new(base: &str) -> Result<Self, url::ParseError> {
        // A trailing slash is required for `join` to append rather than replace.
        let normalized = if base.ends_with('/') {
            base.to_owned()
        } else {
            format!("{base}/")
        };
        Ok(Self {
            base: Url::parse(&normalized)?,
        })
    }

    fn join(&self, path: &str) -> Url {
        self.base
            .join(path)
            .unwrap_or_else(|_| unreachable!("static path joins onto a validated base"))
    }

    pub fn device(&self) -> Url {
        self.join("oauth2/device")
    }

    pub fn token(&self) -> Url {
        self.join("oauth2/token")
    }

    pub fn validate(&self) -> Url {
        self.join("oauth2/validate")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_points_at_twitch() {
        let e = IdEndpoints::default();
        assert_eq!(e.device().as_str(), "https://id.twitch.tv/oauth2/device");
        assert_eq!(e.token().as_str(), "https://id.twitch.tv/oauth2/token");
        assert_eq!(
            e.validate().as_str(),
            "https://id.twitch.tv/oauth2/validate"
        );
    }

    #[test]
    fn accepts_a_mock_base_with_or_without_trailing_slash() {
        for base in ["http://127.0.0.1:8080", "http://127.0.0.1:8080/"] {
            let e = IdEndpoints::new(base).unwrap();
            assert_eq!(e.token().as_str(), "http://127.0.0.1:8080/oauth2/token");
        }
    }
}

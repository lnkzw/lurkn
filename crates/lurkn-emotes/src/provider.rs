use async_trait::async_trait;

use crate::precedence::ProviderId;

/// One source of emotes. A failing provider is logged and skipped, never
/// propagated — Twitch emotes have to keep working when
/// 7TV is down.
#[async_trait]
pub trait EmoteProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    /// Endpoint base, kept on the trait so tests can point it at a mock server.
    fn api_base(&self) -> &str;
}

/// Verified reachable 2026-09-20. All unauthenticated.
pub mod endpoints {
    pub const SEVENTV_GLOBAL: &str = "https://7tv.io/v3/emote-sets/global";
    pub const SEVENTV_USER: &str = "https://7tv.io/v3/users/twitch/";
    pub const BTTV_GLOBAL: &str = "https://api.betterttv.net/3/cached/emotes/global";
    pub const BTTV_USER: &str = "https://api.betterttv.net/3/cached/users/twitch/";
    pub const FFZ_GLOBAL: &str = "https://api.frankerfacez.com/v1/set/global";
    pub const FFZ_ROOM: &str = "https://api.frankerfacez.com/v1/room/id/";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_is_object_safe() {
        fn assert_object_safe(_: Option<&dyn EmoteProvider>) {}
        assert_object_safe(None);
    }

    #[test]
    fn third_party_endpoints_are_https() {
        for url in [
            endpoints::SEVENTV_GLOBAL,
            endpoints::SEVENTV_USER,
            endpoints::BTTV_GLOBAL,
            endpoints::BTTV_USER,
            endpoints::FFZ_GLOBAL,
            endpoints::FFZ_ROOM,
        ] {
            assert!(url.starts_with("https://"), "{url} is not https");
        }
    }
}

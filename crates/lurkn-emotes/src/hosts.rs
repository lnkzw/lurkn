use url::Url;

/// Only these hosts may serve an image that reaches the WebView. Checked in Rust
/// before a URL crosses IPC, and mirrored in the Tauri CSP `img-src`.
/// See `docs/architecture.md` §6.3 and §11.
pub const ALLOWED_EMOTE_HOSTS: &[&str] = &[
    "static-cdn.jtvnw.net", // Twitch emotes, badges, box art
    "cdn.7tv.app",          // 7TV
    "cdn.betterttv.net",    // BTTV
    "cdn.frankerfacez.com", // FFZ
];

pub fn is_allowed_emote_host(url: &Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|h| ALLOWED_EMOTE_HOSTS.contains(&h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn accepts_known_provider_cdns() {
        assert!(is_allowed_emote_host(&u(
            "https://cdn.7tv.app/emote/123/1x.webp"
        )));
        assert!(is_allowed_emote_host(&u(
            "https://static-cdn.jtvnw.net/emoticons/v2/25/default/dark/1.0"
        )));
        assert!(is_allowed_emote_host(&u(
            "https://cdn.betterttv.net/emote/abc/1x"
        )));
        assert!(is_allowed_emote_host(&u(
            "https://cdn.frankerfacez.com/emote/1/1"
        )));
    }

    #[test]
    fn rejects_everything_else() {
        for bad in [
            "http://cdn.7tv.app/emote/1/1x.webp", // plaintext
            "https://evil.example/emote.png",
            "https://cdn.7tv.app.evil.example/x.png", // suffix trick
            "https://evilcdn.7tv.app/x.png",          // subdomain trick
            "javascript:alert(1)",
            "data:image/png;base64,AAAA",
            "file:///etc/passwd",
        ] {
            assert!(
                !is_allowed_emote_host(&u(bad)),
                "should have rejected {bad}"
            );
        }
    }
}

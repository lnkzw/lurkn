use url::Url;

use crate::ids::Login;

const TWITCH_BASE: &str = "https://www.twitch.tv/";
const TWITCH_PLAYER: &str = "https://player.twitch.tv/";

/// Twitch requires a `parent` on the player, as it is built to be embedded.
/// Navigating to it directly works all the same, and Twitch's own popout uses
/// this value.
const PLAYER_PARENT: &str = "twitch.tv";

/// Build the public Twitch page URL for a channel.
///
/// Takes a [`Login`], not a `&str`, so an unvalidated string from chat or an API
/// response can never reach the browser launcher. See `docs/architecture.md` §7.
pub fn twitch_channel_url(login: &Login) -> Url {
    // Safe: TWITCH_BASE is a valid base and Login is restricted to [a-z0-9_].
    Url::parse(TWITCH_BASE)
        .and_then(|base| base.join(login.as_str()))
        .unwrap_or_else(|_| unreachable!("validated login always joins onto a valid base"))
}

/// Build the URL of the bare player for a channel.
///
/// The player page is the video and nothing else: no chat, no panels, no
/// recommendations. Twitch has no URL that turns theatre mode on — it is a
/// per-account setting the site applies itself — so this is the closest thing
/// to it that a URL can ask for, and it loses nothing Lurkn does not already
/// show.
pub fn twitch_player_url(login: &Login) -> Url {
    let mut url = Url::parse(TWITCH_PLAYER)
        .unwrap_or_else(|_| unreachable!("the player base is a valid URL"));
    url.query_pairs_mut()
        .append_pair("channel", login.as_str())
        .append_pair("parent", PLAYER_PARENT);
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_channel_url() {
        let login = Login::parse("forsen").unwrap();
        assert_eq!(
            twitch_channel_url(&login).as_str(),
            "https://www.twitch.tv/forsen"
        );
    }

    #[test]
    fn always_https_on_the_twitch_host() {
        let login = Login::parse("some_user_99").unwrap();
        let url = twitch_channel_url(&login);
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("www.twitch.tv"));
        assert_eq!(url.query(), None);
    }

    #[test]
    fn builds_player_url() {
        let login = Login::parse("forsen").unwrap();
        assert_eq!(
            twitch_player_url(&login).as_str(),
            "https://player.twitch.tv/?channel=forsen&parent=twitch.tv"
        );
    }

    #[test]
    fn the_player_url_is_https_on_twitchs_own_host() {
        let url = twitch_player_url(&Login::parse("some_user_99").unwrap());
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("player.twitch.tv"));
    }
}

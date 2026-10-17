//! `channel.raid` -> [`Raid`].
//!
//! The open channel sending its viewers to another one. Twitch's own page takes
//! its viewers along; this is what lets Lurkn do the same, and what lets a
//! synced window that was showing the raider move on rather than sit on a
//! stream that just ended. Raids *into* the open channel are a different thing:
//! they arrive as a `channel.chat.notification`, see `notification.rs`.

use lurkn_core::{Login, UserId};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RaidEvent {
    pub from_broadcaster_user_id: String,
    pub from_broadcaster_user_login: String,
    pub from_broadcaster_user_name: String,
    pub to_broadcaster_user_id: String,
    pub to_broadcaster_user_login: String,
    pub to_broadcaster_user_name: String,
    #[serde(default)]
    pub viewers: u32,
}

/// One channel raided another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raid {
    pub from_id: UserId,
    pub from_login: Login,
    /// UNTRUSTED.
    pub from_name: String,
    pub to_id: UserId,
    pub to_login: Login,
    /// UNTRUSTED.
    pub to_name: String,
    pub viewers: u32,
}

/// Turn a notification into a raid.
///
/// Returns `None` when either login is not something Twitch should have
/// produced: following a raid means switching to the target, and a name that
/// does not parse is not one to switch to.
pub fn to_raid(event: RaidEvent) -> Option<Raid> {
    Some(Raid {
        from_login: Login::parse(&event.from_broadcaster_user_login).ok()?,
        to_login: Login::parse(&event.to_broadcaster_user_login).ok()?,
        from_id: UserId(event.from_broadcaster_user_id),
        from_name: event.from_broadcaster_user_name,
        to_id: UserId(event.to_broadcaster_user_id),
        to_name: event.to_broadcaster_user_name,
        viewers: event.viewers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Option<Raid> {
        to_raid(serde_json::from_str(json).expect("fixture parses"))
    }

    /// The payload as Twitch documents it.
    #[test]
    fn a_raid_names_both_channels_and_the_viewers() {
        let raid = parse(
            r#"{"from_broadcaster_user_id":"1234","from_broadcaster_user_login":"cool_user",
                "from_broadcaster_user_name":"Cool_User","to_broadcaster_user_id":"1337",
                "to_broadcaster_user_login":"cooler_user","to_broadcaster_user_name":"Cooler_User",
                "viewers":9001}"#,
        )
        .expect("a raid");

        assert_eq!(raid.from_id, UserId("1234".into()));
        assert_eq!(raid.from_login.as_str(), "cool_user");
        assert_eq!(raid.to_id, UserId("1337".into()));
        assert_eq!(raid.to_login.as_str(), "cooler_user");
        assert_eq!(raid.to_name, "Cooler_User");
        assert_eq!(raid.viewers, 9001);
    }

    #[test]
    fn a_target_that_is_not_a_login_is_not_followed() {
        let raid = parse(
            r#"{"from_broadcaster_user_id":"1","from_broadcaster_user_login":"a_user",
                "from_broadcaster_user_name":"A","to_broadcaster_user_id":"2",
                "to_broadcaster_user_login":"not a login","to_broadcaster_user_name":"B",
                "viewers":3}"#,
        );
        assert!(raid.is_none());
    }
}

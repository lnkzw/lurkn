/// The complete set of scopes Lurkn requests, asked for once at login.
///
/// Deliberately excludes `user:read:email` (identity comes from `/oauth2/validate`)
/// and the IRC-only `chat:read` / `chat:edit`. See `docs/architecture.md` §3.3.
pub const REQUIRED_SCOPES: &[&str] = &[
    "user:read:follows", // Get Followed Channels / Get Followed Streams
    "user:read:chat",    // channel.chat.* EventSub subscriptions
    "user:write:chat",   // POST helix/chat/messages
];

/// Scopes Lurkn can use but will not insist on.
///
/// Kept apart from [`REQUIRED_SCOPES`] on purpose: the granted set is checked
/// against the required list after every login and refresh, so adding a scope
/// there would fail the check for every session whose token predates it and log
/// those users out for a feature they never asked for. An optional scope is
/// requested when the user opts in, and its absence only means the feature
/// behind it stays off.
pub const OPTIONAL_SCOPES: &[&str] = &[
    // Sends whispers and, per Twitch, also satisfies the user.whisper.message
    // EventSub subscription, so one scope covers both halves of the feature.
    "user:manage:whispers",
    // Moderation, granted as one bundle: a moderator who wants the timeout
    // button wants the delete button too, and splitting them would mean a
    // consent screen per action.
    "user:read:moderated_channels",
    "moderator:manage:banned_users",
    "moderator:manage:chat_messages",
    "moderator:manage:chat_settings",
    "moderator:manage:announcements",
];

/// The scopes moderation needs, asked for together.
pub const MODERATION_SCOPES: &[&str] = &[
    "user:read:moderated_channels",
    "moderator:manage:banned_users",
    "moderator:manage:chat_messages",
    "moderator:manage:chat_settings",
    "moderator:manage:announcements",
];

/// The scopes to ask for at login. `extra` names optional scopes to include.
pub fn scope_param_with(extra: &[&str]) -> String {
    let mut all: Vec<&str> = REQUIRED_SCOPES.to_vec();
    for scope in extra {
        // Only scopes this build knows about: the string goes to Twitch's
        // consent screen, and a typo there is a login that grants nothing.
        if OPTIONAL_SCOPES.contains(scope) && !all.contains(scope) {
            all.push(scope);
        }
    }
    all.join(" ")
}

pub fn scope_param() -> String {
    REQUIRED_SCOPES.join(" ")
}

/// Whether a granted scope set covers whispers.
pub fn has_whispers(granted: &[String]) -> bool {
    granted
        .iter()
        .any(|s| s.eq_ignore_ascii_case("user:manage:whispers"))
}

/// Whether a granted scope set covers moderation.
///
/// All of them, not any: a half-granted bundle would put buttons on screen
/// that fail when pressed, which is worse than not offering them.
pub fn has_moderation(granted: &[String]) -> bool {
    MODERATION_SCOPES.iter().all(|needed| {
        granted
            .iter()
            .any(|granted| granted.eq_ignore_ascii_case(needed))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_param_is_space_delimited() {
        assert_eq!(
            scope_param(),
            "user:read:follows user:read:chat user:write:chat"
        );
    }

    #[test]
    fn an_optional_scope_is_added_only_when_asked_for() {
        assert_eq!(scope_param_with(&[]), scope_param());
        assert_eq!(
            scope_param_with(&["user:manage:whispers"]),
            "user:read:follows user:read:chat user:write:chat user:manage:whispers"
        );
    }

    /// A scope this build does not know is never forwarded to the consent screen.
    #[test]
    fn an_unknown_scope_is_refused() {
        assert_eq!(
            scope_param_with(&["channel:read:subscriptions"]),
            scope_param()
        );
    }

    #[test]
    fn moderation_needs_the_whole_bundle() {
        let all: Vec<String> = MODERATION_SCOPES.iter().map(|s| (*s).to_owned()).collect();
        assert!(has_moderation(&all));

        // One missing is not "mostly working": it is a button that fails.
        let short: Vec<String> = all.iter().skip(1).cloned().collect();
        assert!(!has_moderation(&short));
        assert!(!has_moderation(&[]));
    }

    #[test]
    fn every_moderation_scope_is_one_the_consent_screen_accepts() {
        for scope in MODERATION_SCOPES {
            assert!(
                OPTIONAL_SCOPES.contains(scope),
                "{scope} would be dropped before it reached Twitch"
            );
        }
    }

    #[test]
    fn whispers_are_detected_however_twitch_cases_them() {
        assert!(has_whispers(&["user:manage:whispers".to_owned()]));
        assert!(has_whispers(&["USER:MANAGE:WHISPERS".to_owned()]));
        assert!(!has_whispers(&["user:read:chat".to_owned()]));
        assert!(!has_whispers(&[]));
    }

    /// Guards the §3.3 minimum-scope decision against casual widening.
    #[test]
    fn requests_no_scope_beyond_the_reviewed_set() {
        for scope in REQUIRED_SCOPES {
            assert!(
                matches!(
                    *scope,
                    "user:read:follows" | "user:read:chat" | "user:write:chat"
                ),
                "{scope} was added without updating docs/architecture.md §3.3"
            );
        }
    }
}

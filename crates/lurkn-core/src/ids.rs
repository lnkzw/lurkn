use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{CoreError, Result};

macro_rules! opaque_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

opaque_id!(
    /// A Twitch numeric user id, carried as a string exactly as Helix returns it.
    UserId
);
opaque_id!(
    /// A chat message id. Used for deduplication — EventSub may redeliver.
    MessageId
);

/// A Twitch login name: lowercase, 3-25 chars of `[a-z0-9_]`.
///
/// Constructing one is the only way a channel name reaches URL building or a
/// subscription condition, so validation lives here and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct Login(String);

impl Login {
    pub fn parse(raw: &str) -> Result<Self> {
        // Twitch logins are case-insensitive; normalise so equality is meaningful.
        let lowered = raw.to_ascii_lowercase();
        let trimmed = lowered.strip_prefix('#').unwrap_or(&lowered);

        if trimmed.len() < 3 {
            return Err(CoreError::InvalidLogin {
                reason: "shorter than 3 characters",
            });
        }
        if trimmed.len() > 25 {
            return Err(CoreError::InvalidLogin {
                reason: "longer than 25 characters",
            });
        }
        if !trimmed
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(CoreError::InvalidLogin {
                reason: "contains characters outside [a-z0-9_]",
            });
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_logins() {
        assert_eq!(Login::parse("forsen").unwrap().as_str(), "forsen");
        assert_eq!(
            Login::parse("some_user_99").unwrap().as_str(),
            "some_user_99"
        );
    }

    #[test]
    fn normalises_case_and_leading_hash() {
        assert_eq!(Login::parse("ForSen").unwrap().as_str(), "forsen");
        assert_eq!(Login::parse("#ForSen").unwrap().as_str(), "forsen");
    }

    #[test]
    fn rejects_bad_logins() {
        for bad in [
            "ab",                    // too short
            "a".repeat(26).as_str(), // too long
            "has space",
            "has/slash",
            "has:colon",
            "../../etc/passwd",
            "user?a=b",
            "user#frag",
            "ünïcode",
        ] {
            assert!(Login::parse(bad).is_err(), "should have rejected {bad:?}");
        }
    }
}

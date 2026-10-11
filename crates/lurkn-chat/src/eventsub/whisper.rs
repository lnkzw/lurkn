//! `user.whisper.message` -> [`Whisper`].
//!
//! Whispers arrive on the same socket as chat and under a scope the user opts
//! into. There is no endpoint for the ones that arrived while Lurkn was closed,
//! so every notification is written to the store as it lands — see
//! `lurkn_core::whisper`.

use lurkn_core::{Login, UserId, Whisper};
use serde::Deserialize;
use time::OffsetDateTime;

#[derive(Debug, Deserialize)]
pub struct WhisperEvent {
    pub from_user_id: String,
    pub from_user_login: String,
    pub from_user_name: String,
    #[serde(default)]
    pub to_user_id: String,
    pub whisper_id: String,
    pub whisper: WhisperBody,
}

#[derive(Debug, Deserialize)]
pub struct WhisperBody {
    #[serde(default)]
    pub text: String,
}

/// Turn a notification into a domain whisper.
///
/// Always incoming: Twitch only notifies the recipient, so a whisper this
/// account sent never comes back over the socket. The sending half records its
/// own copy.
///
/// Returns `None` when the sender's login is not something Twitch should have
/// produced — skipping beats guessing, as elsewhere in this crate.
pub fn to_whisper(event: WhisperEvent, received_at: OffsetDateTime) -> Option<Whisper> {
    let peer_login = Login::parse(&event.from_user_login).ok()?;

    Some(Whisper {
        id: event.whisper_id,
        peer_id: UserId(event.from_user_id),
        peer_login,
        peer_name: event.from_user_name,
        outgoing: false,
        body: event.whisper.text,
        // The payload carries no timestamp, so arrival is the best answer
        // available — and for a live-only feed it is within a second of true.
        sent_at: received_at,
        read: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    fn parse(json: &str) -> Option<Whisper> {
        to_whisper(serde_json::from_str(json).expect("fixture parses"), NOW)
    }

    #[test]
    fn a_whisper_is_threaded_under_its_sender() {
        let whisper = parse(
            r##"{"from_user_id":"3","from_user_login":"nymn","from_user_name":"NymN",
                "to_user_id":"9","whisper_id":"W1","whisper":{"text":"hello there"}}"##,
        )
        .expect("a whisper renders");

        assert_eq!(whisper.peer_id.as_str(), "3");
        assert_eq!(whisper.peer_name, "NymN");
        assert_eq!(whisper.body, "hello there");
        assert!(!whisper.outgoing, "the socket only ever delivers incoming");
        assert!(!whisper.read);
    }

    #[test]
    fn a_sender_login_twitch_should_never_send_is_skipped() {
        assert!(parse(
            r##"{"from_user_id":"3","from_user_login":"not a login","from_user_name":"X",
                "to_user_id":"9","whisper_id":"W2","whisper":{"text":"hi"}}"##,
        )
        .is_none());
    }

    #[test]
    fn an_empty_whisper_still_carries_its_thread() {
        let whisper = parse(
            r##"{"from_user_id":"3","from_user_login":"nymn","from_user_name":"NymN",
                "to_user_id":"9","whisper_id":"W3","whisper":{}}"##,
        )
        .expect("an empty body is not a parse failure");
        assert_eq!(whisper.body, "");
    }
}

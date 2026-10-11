//! EventSub envelope parsing. Pure: no IO, so the whole protocol surface is
//! testable from fixtures. See `docs/architecture.md` §5.1.

use serde::Deserialize;
use time::OffsetDateTime;

#[derive(Debug, Deserialize)]
struct RawEnvelope {
    metadata: RawMetadata,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct RawMetadata {
    message_id: String,
    message_type: String,
    #[serde(default)]
    message_timestamp: String,
    #[serde(default)]
    subscription_type: Option<String>,
}

/// What the socket just told us.
#[derive(Debug, Clone, PartialEq)]
pub enum Envelope {
    /// Handshake. The session id is needed to create subscriptions, and there
    /// are only `keepalive_timeout_seconds` before Twitch expects the first one.
    Welcome {
        session_id: String,
        keepalive_timeout_secs: u64,
    },
    /// A heartbeat. Only meaningful as proof the socket is alive.
    Keepalive,
    /// An event for a subscription. `payload` is the raw event object.
    Notification {
        message_id: String,
        subscription_type: String,
        event: serde_json::Value,
    },
    /// Twitch is moving us to another edge; connect to `reconnect_url` and do not
    /// close this socket until the new one greets us.
    Reconnect { reconnect_url: String },
    /// The subscription is gone for good — do not retry it.
    Revocation {
        subscription_type: String,
        status: String,
    },
    /// A message type Twitch added after this build. Ignored, never fatal.
    Unknown { message_type: String },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvelopeError {
    #[error("not valid JSON: {0}")]
    NotJson(String),
    #[error("missing {field} for a {message_type} message")]
    MissingField {
        message_type: String,
        field: &'static str,
    },
}

/// Parse one text frame.
pub fn parse(frame: &str) -> Result<Envelope, EnvelopeError> {
    let raw: RawEnvelope =
        serde_json::from_str(frame).map_err(|e| EnvelopeError::NotJson(e.to_string()))?;

    let missing = |field| EnvelopeError::MissingField {
        message_type: raw.metadata.message_type.clone(),
        field,
    };

    match raw.metadata.message_type.as_str() {
        "session_welcome" => {
            let session = raw
                .payload
                .get("session")
                .ok_or_else(|| missing("payload.session"))?;
            let session_id = session
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| missing("payload.session.id"))?
                .to_owned();
            let keepalive_timeout_secs = session
                .get("keepalive_timeout_seconds")
                .and_then(serde_json::Value::as_u64)
                // Twitch's documented default when it does not say otherwise.
                .unwrap_or(10);
            Ok(Envelope::Welcome {
                session_id,
                keepalive_timeout_secs,
            })
        }

        "session_keepalive" => Ok(Envelope::Keepalive),

        "notification" => {
            let event = raw
                .payload
                .get("event")
                .cloned()
                .ok_or_else(|| missing("payload.event"))?;
            let subscription_type = raw
                .metadata
                .subscription_type
                .clone()
                .or_else(|| {
                    raw.payload
                        .get("subscription")
                        .and_then(|s| s.get("type"))
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .ok_or_else(|| missing("metadata.subscription_type"))?;

            Ok(Envelope::Notification {
                message_id: raw.metadata.message_id,
                subscription_type,
                event,
            })
        }

        "session_reconnect" => {
            let reconnect_url = raw
                .payload
                .get("session")
                .and_then(|s| s.get("reconnect_url"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| missing("payload.session.reconnect_url"))?
                .to_owned();
            Ok(Envelope::Reconnect { reconnect_url })
        }

        "revocation" => {
            let subscription = raw.payload.get("subscription");
            Ok(Envelope::Revocation {
                subscription_type: subscription
                    .and_then(|s| s.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                status: subscription
                    .and_then(|s| s.get("status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_owned(),
            })
        }

        other => Ok(Envelope::Unknown {
            message_type: other.to_owned(),
        }),
    }
}

/// The envelope's own timestamp, when it parses. Used only for diagnostics —
/// message ordering comes from arrival order.
pub fn timestamp(frame: &str) -> Option<OffsetDateTime> {
    let raw: RawEnvelope = serde_json::from_str(frame).ok()?;
    OffsetDateTime::parse(
        &raw.metadata.message_timestamp,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes mirror Twitch's documented EventSub WebSocket frames.
    const WELCOME: &str = r#"{
      "metadata":{"message_id":"96a3f3b5","message_type":"session_welcome",
                  "message_timestamp":"2026-09-20T12:00:00.000Z"},
      "payload":{"session":{"id":"AQoQexAWVYKS","status":"connected",
                 "connected_at":"2026-09-20T12:00:00.000Z",
                 "keepalive_timeout_seconds":10,"reconnect_url":null}}}"#;

    #[test]
    fn parses_the_welcome_handshake() {
        assert_eq!(
            parse(WELCOME).unwrap(),
            Envelope::Welcome {
                session_id: "AQoQexAWVYKS".into(),
                keepalive_timeout_secs: 10,
            }
        );
    }

    #[test]
    fn a_welcome_without_a_timeout_uses_twitchs_default() {
        let frame = r#"{"metadata":{"message_id":"1","message_type":"session_welcome"},
                        "payload":{"session":{"id":"S1"}}}"#;
        assert_eq!(
            parse(frame).unwrap(),
            Envelope::Welcome {
                session_id: "S1".into(),
                keepalive_timeout_secs: 10
            }
        );
    }

    #[test]
    fn parses_a_keepalive() {
        let frame = r#"{"metadata":{"message_id":"1","message_type":"session_keepalive"},
                        "payload":{}}"#;
        assert_eq!(parse(frame).unwrap(), Envelope::Keepalive);
    }

    #[test]
    fn parses_a_notification_and_keeps_the_event_intact() {
        let frame = r#"{
          "metadata":{"message_id":"M1","message_type":"notification",
                      "subscription_type":"channel.chat.message","subscription_version":"1"},
          "payload":{"subscription":{"id":"S","type":"channel.chat.message","status":"enabled"},
                     "event":{"chatter_user_login":"forsen","message":{"text":"hi"}}}}"#;

        match parse(frame).unwrap() {
            Envelope::Notification {
                message_id,
                subscription_type,
                event,
            } => {
                assert_eq!(message_id, "M1");
                assert_eq!(subscription_type, "channel.chat.message");
                assert_eq!(event["chatter_user_login"], "forsen");
            }
            other => panic!("expected a notification, got {other:?}"),
        }
    }

    /// Older frames put the type only on the subscription object.
    #[test]
    fn a_notification_falls_back_to_the_subscription_type() {
        let frame = r#"{
          "metadata":{"message_id":"M1","message_type":"notification"},
          "payload":{"subscription":{"type":"channel.chat.message"},"event":{"a":1}}}"#;
        match parse(frame).unwrap() {
            Envelope::Notification {
                subscription_type, ..
            } => {
                assert_eq!(subscription_type, "channel.chat.message");
            }
            other => panic!("expected a notification, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_reconnect_with_its_url() {
        let frame = r#"{"metadata":{"message_id":"1","message_type":"session_reconnect"},
          "payload":{"session":{"id":"S","status":"reconnecting",
                     "reconnect_url":"wss://eventsub.wss.twitch.tv/ws?challenge=x"}}}"#;
        assert_eq!(
            parse(frame).unwrap(),
            Envelope::Reconnect {
                reconnect_url: "wss://eventsub.wss.twitch.tv/ws?challenge=x".into()
            }
        );
    }

    #[test]
    fn parses_a_revocation() {
        let frame = r#"{"metadata":{"message_id":"1","message_type":"revocation"},
          "payload":{"subscription":{"type":"channel.chat.message",
                     "status":"authorization_revoked"}}}"#;
        assert_eq!(
            parse(frame).unwrap(),
            Envelope::Revocation {
                subscription_type: "channel.chat.message".into(),
                status: "authorization_revoked".into(),
            }
        );
    }

    /// Twitch adding a message type must not take the socket down.
    #[test]
    fn an_unknown_message_type_is_ignored_not_fatal() {
        let frame = r#"{"metadata":{"message_id":"1","message_type":"session_something_new"},
                        "payload":{}}"#;
        assert_eq!(
            parse(frame).unwrap(),
            Envelope::Unknown {
                message_type: "session_something_new".into()
            }
        );
    }

    #[test]
    fn a_truncated_frame_is_an_error_not_a_panic() {
        assert!(matches!(
            parse("{ not json"),
            Err(EnvelopeError::NotJson(_))
        ));
        assert!(matches!(parse(""), Err(EnvelopeError::NotJson(_))));
    }

    #[test]
    fn a_welcome_without_a_session_id_is_reported_precisely() {
        let frame = r#"{"metadata":{"message_id":"1","message_type":"session_welcome"},
                        "payload":{"session":{"status":"connected"}}}"#;
        assert_eq!(
            parse(frame),
            Err(EnvelopeError::MissingField {
                message_type: "session_welcome".into(),
                field: "payload.session.id",
            })
        );
    }

    #[test]
    fn a_notification_without_an_event_is_reported_precisely() {
        let frame = r#"{"metadata":{"message_id":"1","message_type":"notification",
                        "subscription_type":"channel.chat.message"},"payload":{}}"#;
        assert!(matches!(
            parse(frame),
            Err(EnvelopeError::MissingField { .. })
        ));
    }

    #[test]
    fn reads_the_envelope_timestamp() {
        assert_eq!(
            timestamp(WELCOME),
            Some(time::macros::datetime!(2026-09-20 12:00:00 UTC))
        );
        assert_eq!(timestamp("{ not json"), None);
    }
}

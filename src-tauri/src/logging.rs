//! Logging with credential redaction.
//!
//! Technical detail stays available in a log view, and that detail must never
//! contain a token. Redaction happens at the writer,
//! so it covers every layer and every third-party crate's output.

use std::io::{self, Write};

use tracing_subscriber::fmt::MakeWriter;

/// Markers after which the next value is a credential.
const SECRET_MARKERS: &[&str] = &[
    "Bearer ",
    // Twitch's /oauth2/validate uses this scheme rather than Bearer.
    "OAuth ",
    "oauth:",
    "access_token=",
    "refresh_token=",
    "device_code=",
    "client_secret=",
    "\"access_token\":\"",
    "\"refresh_token\":\"",
    "\"device_code\":\"",
];

const REPLACEMENT: &str = "<redacted>";

/// Replace the value following any [`SECRET_MARKERS`] occurrence.
pub fn redact(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;

    'outer: loop {
        // Find the earliest marker in what is left.
        let next = SECRET_MARKERS
            .iter()
            .filter_map(|m| rest.find(m).map(|at| (at, *m)))
            .min_by_key(|(at, _)| *at);

        let Some((at, marker)) = next else {
            break 'outer;
        };

        out.push_str(&rest[..at + marker.len()]);
        let after = &rest[at + marker.len()..];

        // The credential runs until whitespace or a JSON/structural delimiter.
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | ',' | '}' | '&' | ')' | '\''))
            .unwrap_or(after.len());
        out.push_str(REPLACEMENT);
        rest = &after[end..];
    }

    out.push_str(rest);
    out
}

/// Wraps a writer and redacts each write before it reaches the sink.
pub struct RedactingWriter<W>(W);

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match std::str::from_utf8(buf) {
            Ok(text) => {
                self.0.write_all(redact(text).as_bytes())?;
                Ok(buf.len())
            }
            // Non-UTF-8 can't contain a token we know how to find; pass it through.
            Err(_) => self.0.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub struct Redacting<M>(pub M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Redacting<M> {
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter(self.0.make_writer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_bearer_headers() {
        assert_eq!(
            redact("authorization: Bearer abcdef0123456789abcdef0123 ok"),
            "authorization: Bearer <redacted> ok"
        );
    }

    #[test]
    fn redacts_json_token_fields() {
        let line = r#"{"access_token":"s3cret","refresh_token":"r3fresh","expires_in":14820}"#;
        let got = redact(line);
        assert!(!got.contains("s3cret"), "{got}");
        assert!(!got.contains("r3fresh"), "{got}");
        assert!(
            got.contains("14820"),
            "non-secret fields must survive: {got}"
        );
    }

    #[test]
    fn redacts_form_encoded_device_code() {
        assert_eq!(
            redact("client_id=abc&device_code=xyz789&grant_type=urn:x"),
            "client_id=abc&device_code=<redacted>&grant_type=urn:x"
        );
    }

    #[test]
    fn redacts_the_oauth_validate_scheme() {
        assert_eq!(
            redact("Authorization: OAuth abc123def456 ->"),
            "Authorization: OAuth <redacted> ->"
        );
    }

    #[test]
    fn redacts_the_device_code() {
        let got = redact(r#"{"device_code":"rVV9DrnD1MJH","user_code":"RZQXXTVC"}"#);
        assert!(!got.contains("rVV9DrnD1MJH"), "{got}");
        assert!(
            got.contains("RZQXXTVC"),
            "the user-facing code must stay visible: {got}"
        );
    }

    #[test]
    fn leaves_ordinary_lines_untouched() {
        let line = "joined channel forsen, 42 emotes loaded";
        assert_eq!(redact(line), line);
    }

    #[test]
    fn handles_several_secrets_in_one_line() {
        let got = redact("Bearer aaa and oauth:bbb and Bearer ccc");
        assert_eq!(
            got,
            "Bearer <redacted> and oauth:<redacted> and Bearer <redacted>"
        );
    }
}

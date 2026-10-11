//! The smallest HTTP this server needs, as pure functions.
//!
//! Three routes, localhost only: the page and its event stream by `GET`, and
//! the one thing the page may ask of Lurkn — a font step — by `POST`. Keeping
//! the decisions here rather than inside the accept loop is what makes the
//! access rules testable without opening a socket — and the access rules are
//! the whole security story.

use std::collections::BTreeMap;

/// Longer than any request this server answers, and a bound on what a client
/// can make it buffer.
pub const MAX_HEAD: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub host: Option<String>,
}

/// What the server should do with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The watch page itself.
    Page,
    /// The event stream the page listens on.
    Events,
    /// Ctrl and `+` or `-` in the window: Lurkn's chat font, one step.
    Font(Step),
    NotFound,
    /// Right shape, wrong or missing token.
    Forbidden,
    /// Addressed to something other than this machine — see `host_is_local`.
    BadHost,
    MethodNotAllowed,
}

/// Which way a font step goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Bigger,
    Smaller,
}

/// Parse a request head. Returns `None` for anything that is not a request.
pub fn parse_request(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut start = lines.next()?.split(' ');

    let method = start.next()?.to_owned();
    let target = start.next()?;
    // The version is present but nothing here varies on it.
    start.next()?;

    let (path, raw_query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };

    let mut query = BTreeMap::new();
    for pair in raw_query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        query.insert(key.to_owned(), value.to_owned());
    }

    let host = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim().to_owned());

    Some(Request {
        method,
        path: path.to_owned(),
        query,
        host,
    })
}

/// Whether the `Host` header names this machine.
///
/// A site on the internet can point its own hostname at 127.0.0.1 and then talk
/// to this server as a same-origin page — DNS rebinding. The browser sends the
/// attacker's hostname in `Host`, so refusing anything that is not a loopback
/// name closes that door; the token alone would not, because a rebound page can
/// read responses.
pub fn host_is_local(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else {
        // HTTP/1.1 requires one; a request without it is not from a browser.
        return false;
    };

    // An IPv6 literal carries colons of its own, inside brackets, so the port
    // cannot simply be split off the end.
    let (name, given_port) = if host.starts_with('[') {
        let Some(end) = host.find(']') else {
            return false;
        };
        let rest = &host[end + 1..];
        match rest.is_empty() {
            true => (&host[..=end], None),
            false => match rest.strip_prefix(':') {
                Some(port) => (&host[..=end], Some(port)),
                None => return false,
            },
        }
    } else {
        match host.rsplit_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host, None),
        }
    };

    if let Some(given) = given_port {
        if given.parse::<u16>() != Ok(port) {
            return false;
        }
    }

    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

/// Decide what a parsed request gets.
///
/// The font step changes a setting, so it is a `POST`: nothing that merely
/// loads a URL — a prefetch, a link, an `<img>` — can trigger it. A form on
/// another site can send a `POST` here, but not with the token, which only
/// the page itself was given.
pub fn route(request: &Request, token: &str, port: u16) -> Route {
    let method = request.method.as_str();
    if !matches!(method, "GET" | "POST") {
        return Route::MethodNotAllowed;
    }
    if !host_is_local(request.host.as_deref(), port) {
        return Route::BadHost;
    }

    let known = match (method, request.path.as_str()) {
        ("GET", "/") => Route::Page,
        ("GET", "/events") => Route::Events,
        ("POST", "/font") => match request.query.get("step").map(String::as_str) {
            Some("bigger") => Route::Font(Step::Bigger),
            Some("smaller") => Route::Font(Step::Smaller),
            _ => return Route::NotFound,
        },
        (_, "/" | "/events" | "/font") => return Route::MethodNotAllowed,
        _ => return Route::NotFound,
    };

    // Constant-time comparison is not needed: a wrong guess costs a round trip
    // and the token is 128 bits.
    if request.query.get("t").map(String::as_str) != Some(token) {
        return Route::Forbidden;
    }

    known
}

/// What a font step is answered with: done, nothing to say.
pub const NO_CONTENT: &str = "HTTP/1.1 204 No Content\r\n\
                              Cache-Control: no-store\r\n\
                              Connection: close\r\n\
                              \r\n";

/// A complete response for everything except the real routes.
pub fn error_response(route: Route) -> String {
    let (status, body) = match route {
        Route::NotFound => ("404 Not Found", "not found"),
        Route::Forbidden => ("403 Forbidden", "forbidden"),
        Route::BadHost => ("421 Misdirected Request", "wrong host"),
        Route::MethodNotAllowed => ("405 Method Not Allowed", "method not allowed"),
        Route::Page | Route::Events | Route::Font(_) => {
            ("500 Internal Server Error", "unreachable")
        }
    };

    format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(target: &str, host: &str) -> Request {
        parse_request(&format!(
            "GET {target} HTTP/1.1\r\nHost: {host}\r\nAccept: */*\r\n\r\n"
        ))
        .unwrap()
    }

    #[test]
    fn parses_the_path_and_query() {
        let request = get("/events?t=abc&x=1", "127.0.0.1:9000");
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/events");
        assert_eq!(request.query.get("t").unwrap(), "abc");
        assert_eq!(request.query.get("x").unwrap(), "1");
        assert_eq!(request.host.as_deref(), Some("127.0.0.1:9000"));
    }

    #[test]
    fn parses_a_target_without_a_query() {
        let request = get("/", "localhost:9000");
        assert_eq!(request.path, "/");
        assert!(request.query.is_empty());
    }

    #[test]
    fn the_right_token_reaches_both_routes() {
        assert_eq!(
            route(&get("/?t=secret", "127.0.0.1:9000"), "secret", 9000),
            Route::Page
        );
        assert_eq!(
            route(&get("/events?t=secret", "127.0.0.1:9000"), "secret", 9000),
            Route::Events
        );
    }

    #[test]
    fn a_wrong_or_missing_token_is_refused() {
        for target in ["/", "/?t=", "/?t=wrong", "/events", "/events?t=wrong"] {
            assert_eq!(
                route(&get(target, "127.0.0.1:9000"), "secret", 9000),
                Route::Forbidden,
                "{target}"
            );
        }
    }

    /// A page on the internet that points its own name at 127.0.0.1 would
    /// otherwise be same-origin with this server.
    #[test]
    fn a_foreign_host_header_is_refused() {
        for host in ["evil.example", "evil.example:9000", "192.168.1.9:9000"] {
            assert_eq!(
                route(&get("/?t=secret", host), "secret", 9000),
                Route::BadHost,
                "{host}"
            );
        }
    }

    #[test]
    fn loopback_names_are_accepted() {
        for host in ["127.0.0.1:9000", "localhost:9000", "[::1]:9000"] {
            assert!(host_is_local(Some(host), 9000), "{host}");
        }
    }

    #[test]
    fn a_malformed_host_is_refused() {
        for host in ["[::1", "[::1]junk", "[]:9000"] {
            assert!(!host_is_local(Some(host), 9000), "{host}");
        }
    }

    #[test]
    fn a_loopback_name_on_another_port_is_refused() {
        assert!(!host_is_local(Some("127.0.0.1:9001"), 9000));
        assert!(!host_is_local(None, 9000));
    }

    fn request(method: &str, target: &str) -> Request {
        parse_request(&format!(
            "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:9000\r\n\r\n"
        ))
        .unwrap()
    }

    #[test]
    fn the_page_and_its_stream_are_get_only() {
        for target in ["/?t=secret", "/events?t=secret"] {
            assert_eq!(
                route(&request("POST", target), "secret", 9000),
                Route::MethodNotAllowed,
                "{target}"
            );
        }
        assert_eq!(
            route(&request("DELETE", "/?t=secret"), "secret", 9000),
            Route::MethodNotAllowed
        );
    }

    #[test]
    fn a_font_step_is_a_post_with_the_token() {
        assert_eq!(
            route(
                &request("POST", "/font?t=secret&step=bigger"),
                "secret",
                9000
            ),
            Route::Font(Step::Bigger)
        );
        assert_eq!(
            route(
                &request("POST", "/font?t=secret&step=smaller"),
                "secret",
                9000
            ),
            Route::Font(Step::Smaller)
        );
        // Loading the URL, as a link or an image would, changes nothing.
        assert_eq!(
            route(
                &request("GET", "/font?t=secret&step=bigger"),
                "secret",
                9000
            ),
            Route::MethodNotAllowed
        );
    }

    /// The token is what stops a form on another site from posting here.
    #[test]
    fn a_font_step_without_the_token_is_refused() {
        for target in ["/font?step=bigger", "/font?t=wrong&step=bigger"] {
            assert_eq!(
                route(&request("POST", target), "secret", 9000),
                Route::Forbidden,
                "{target}"
            );
        }
    }

    #[test]
    fn an_unknown_font_step_is_not_found() {
        for target in ["/font?t=secret", "/font?t=secret&step=huge"] {
            assert_eq!(
                route(&request("POST", target), "secret", 9000),
                Route::NotFound,
                "{target}"
            );
        }
    }

    #[test]
    fn an_unknown_path_is_not_found_even_with_the_token() {
        assert_eq!(
            route(&get("/secrets?t=secret", "127.0.0.1:9000"), "secret", 9000),
            Route::NotFound
        );
    }

    #[test]
    fn garbage_is_not_a_request() {
        assert!(parse_request("hello").is_none());
        assert!(parse_request("").is_none());
    }
}

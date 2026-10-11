//! Your chat as a page for an OBS browser source.
//!
//! The same loopback server shape as the watch window — one page, one event
//! stream, `GET` only, a `Host` check — with two differences that follow from
//! OBS keeping the URL in a saved scene:
//!
//! - **a fixed port**, chosen in settings, so the URL survives a restart;
//! - **no token.** Everything the page shows is already public on twitch.tv,
//!   and a token would have to be stored, shown and regenerated for no gain.
//!   The `Host` check still stands: without it a site could rebind its own
//!   name to 127.0.0.1 and read the stream as a same-origin page. A plain
//!   cross-origin page cannot read it at all, since nothing here sends CORS
//!   headers.
//!
//! See `docs/architecture.md` §7.3.

pub mod page;
pub mod wire;

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::AbortHandle;

use crate::http::{self, Route};
use crate::{read_head, write_to_all, Result, HEARTBEAT};
pub use wire::{admits, OverlayLook, OverlayMessage};

/// How many recent messages a new connection is replayed. The most the
/// settings let the page show at once, so a refreshed source comes back full.
const REPLAY: usize = lurkn_core::settings::MAX_OVERLAY_MESSAGES as usize;

struct Recent {
    id: String,
    login: String,
    frame: String,
}

struct Shared {
    port: u16,
    nonce: String,
    clients: Mutex<Vec<TcpStream>>,
    /// The current look, serialised, for the page and for every new stream.
    look: Mutex<String>,
    recent: Mutex<VecDeque<Recent>>,
}

/// A running overlay server. Unlike the watch server it can be stopped: the
/// reader can switch it off, or move it to another port.
#[derive(Clone)]
pub struct OverlayServer {
    shared: Arc<Shared>,
    tasks: Arc<Vec<AbortHandle>>,
}

impl std::fmt::Debug for OverlayServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayServer")
            .field("port", &self.shared.port)
            .finish()
    }
}

impl OverlayServer {
    /// Bind `127.0.0.1:port` and start serving. Port 0 picks a free one, which
    /// only the tests want.
    pub async fn start(port: u16, look: &OverlayLook) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        let port = listener.local_addr()?.port();

        let shared = Arc::new(Shared {
            port,
            nonce: crate::random_hex()?,
            clients: Mutex::new(Vec::new()),
            look: Mutex::new(look_json(look)),
            recent: Mutex::new(VecDeque::with_capacity(REPLAY)),
        });

        let accept = tokio::spawn(accept_loop(listener, Arc::clone(&shared)));
        let heartbeat = tokio::spawn(heartbeat(Arc::clone(&shared)));

        tracing::info!(port, "the chat overlay is listening on loopback");
        Ok(Self {
            shared,
            tasks: Arc::new(vec![accept.abort_handle(), heartbeat.abort_handle()]),
        })
    }

    pub fn port(&self) -> u16 {
        self.shared.port
    }

    /// What goes into OBS. `127.0.0.1` rather than `localhost`: the server
    /// binds IPv4 only, and `localhost` may resolve to `::1` first.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.shared.port)
    }

    /// Restyle every open page in place. A look that has not changed is not
    /// sent: this runs on every settings change and channel switch.
    pub async fn set_look(&self, look: &OverlayLook) -> usize {
        let json = look_json(look);
        {
            let mut current = self.shared.look.lock().await;
            if *current == json {
                return self.listeners().await;
            }
            current.clone_from(&json);
        }
        write_to_all(&self.shared.clients, frame("look", &json).as_bytes()).await
    }

    /// Show a message, and keep it for pages that connect later.
    pub async fn message(&self, message: &OverlayMessage) -> usize {
        let Ok(json) = serde_json::to_string(message) else {
            return 0;
        };
        let frame = frame("message", &json);

        {
            let mut recent = self.shared.recent.lock().await;
            if recent.len() == REPLAY {
                recent.pop_front();
            }
            recent.push_back(Recent {
                id: message.id.clone(),
                login: message.login.clone(),
                frame: frame.clone(),
            });
        }

        write_to_all(&self.shared.clients, frame.as_bytes()).await
    }

    /// A moderator removed one message.
    pub async fn delete(&self, message_id: &str) {
        self.shared
            .recent
            .lock()
            .await
            .retain(|r| r.id != message_id);
        self.send_string("delete", message_id).await;
    }

    /// A timeout or ban: everything that chatter said goes.
    pub async fn clear_user(&self, login: &str) {
        self.shared
            .recent
            .lock()
            .await
            .retain(|r| !r.login.eq_ignore_ascii_case(login));
        self.send_string("user", &login.to_ascii_lowercase()).await;
    }

    /// `/clear`.
    pub async fn clear(&self) {
        self.shared.recent.lock().await.clear();
        write_to_all(&self.shared.clients, frame("clear", "null").as_bytes()).await;
    }

    /// How many pages are connected, as of the last write.
    pub async fn listeners(&self) -> usize {
        self.shared.clients.lock().await.len()
    }

    /// Stop listening and drop every connection. The port is free once this
    /// returns, so a restart on the same port can bind it straight away.
    pub async fn shutdown(&self) {
        for task in self.tasks.iter() {
            task.abort();
        }
        self.shared.clients.lock().await.clear();
        // An aborted task drops its listener on the runtime's next poll of it,
        // not here; yield until it has, so the port is really free.
        for _ in 0..50 {
            if TcpListener::bind(("127.0.0.1", self.shared.port))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        tracing::info!(port = self.shared.port, "the chat overlay stopped");
    }

    async fn send_string(&self, event: &str, value: &str) {
        if let Ok(json) = serde_json::to_string(value) {
            write_to_all(&self.shared.clients, frame(event, &json).as_bytes()).await;
        }
    }
}

/// One server-sent event. `data` is always JSON from serde, which escapes
/// newlines, so it cannot end the frame early or forge another.
fn frame(event: &str, data: &str) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

fn look_json(look: &OverlayLook) -> String {
    serde_json::to_string(look).unwrap_or_else(|_| "{}".to_owned())
}

/// Decide what a request gets. The watch server's rules, minus the token.
pub fn route(request: &http::Request, port: u16) -> Route {
    if request.method != "GET" {
        return Route::MethodNotAllowed;
    }
    if !http::host_is_local(request.host.as_deref(), port) {
        return Route::BadHost;
    }
    match request.path.as_str() {
        "/" => Route::Page,
        "/events" => Route::Events,
        _ => Route::NotFound,
    }
}

async fn heartbeat(shared: Arc<Shared>) {
    let mut ticker = tokio::time::interval(HEARTBEAT);
    loop {
        ticker.tick().await;
        write_to_all(&shared.clients, b": ping\n\n").await;
    }
}

async fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let shared = Arc::clone(&shared);
                tokio::spawn(async move {
                    if let Err(error) = serve(stream, shared).await {
                        tracing::debug!(%error, "an overlay request ended early");
                    }
                });
            }
            Err(error) => {
                tracing::warn!(%error, "the overlay listener stopped accepting");
                return;
            }
        }
    }
}

async fn serve(mut stream: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let Some(head) = read_head(&mut stream).await? else {
        return Ok(());
    };

    let Some(request) = http::parse_request(&head) else {
        stream
            .write_all(http::error_response(Route::NotFound).as_bytes())
            .await?;
        return Ok(());
    };

    match route(&request, shared.port) {
        Route::Page => {
            let body = page::page_html(&shared.nonce, &shared.look.lock().await);
            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: text/html; charset=utf-8\r\n\
                 Content-Length: {}\r\n\
                 Cache-Control: no-store\r\n\
                 Connection: close\r\n\
                 \r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await?;
            stream.write_all(body.as_bytes()).await?;
        }

        Route::Events => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\n\
                      Content-Type: text/event-stream\r\n\
                      Cache-Control: no-store\r\n\
                      Connection: keep-alive\r\n\
                      \r\n",
                )
                .await?;

            // The look first, then what was said recently: a source OBS just
            // refreshed comes back styled and full rather than blank.
            let look = frame("look", &shared.look.lock().await);
            stream.write_all(look.as_bytes()).await?;
            for recent in shared.recent.lock().await.iter() {
                stream.write_all(recent.frame.as_bytes()).await?;
            }
            stream.flush().await?;

            shared.clients.lock().await.push(stream);
            return Ok(());
        }

        other => {
            stream
                .write_all(http::error_response(other).as_bytes())
                .await?;
        }
    }

    stream.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use lurkn_core::OverlaySettings;
    use tokio::io::AsyncReadExt;

    fn get(target: &str, host: &str) -> http::Request {
        http::parse_request(&format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n")).unwrap()
    }

    #[test]
    fn both_routes_answer_without_a_token() {
        assert_eq!(route(&get("/", "127.0.0.1:7373"), 7373), Route::Page);
        assert_eq!(route(&get("/?demo", "localhost:7373"), 7373), Route::Page);
        assert_eq!(
            route(&get("/events", "127.0.0.1:7373"), 7373),
            Route::Events
        );
    }

    /// The one guard that matters without a token: a rebound hostname.
    #[test]
    fn a_foreign_host_is_refused() {
        assert_eq!(
            route(&get("/events", "evil.example:7373"), 7373),
            Route::BadHost
        );
        assert_eq!(route(&get("/", "127.0.0.1:9999"), 7373), Route::BadHost);
    }

    #[test]
    fn only_get_and_two_paths() {
        let post = http::parse_request("POST / HTTP/1.1\r\nHost: 127.0.0.1:7373\r\n\r\n").unwrap();
        assert_eq!(route(&post, 7373), Route::MethodNotAllowed);
        assert_eq!(
            route(&get("/secrets", "127.0.0.1:7373"), 7373),
            Route::NotFound
        );
    }

    #[test]
    fn a_frame_cannot_be_split_by_its_data() {
        let json = serde_json::to_string("line\n\nevent: clear").unwrap();
        let frame = frame("delete", &json);
        assert_eq!(frame.matches("\n\n").count(), 1, "{frame}");
    }

    fn message(id: &str, login: &str) -> OverlayMessage {
        OverlayMessage {
            id: id.into(),
            login: login.into(),
            name: login.into(),
            color: None,
            badges: Vec::new(),
            action: false,
            parts: vec![wire::Part::Text { text: "hi".into() }],
            at: 0,
        }
    }

    async fn read_until(stream: &mut TcpStream, needle: &str) -> String {
        let mut seen = String::new();
        let mut chunk = [0_u8; 4096];
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        while !seen.contains(needle) {
            let read = tokio::time::timeout_at(deadline, stream.read(&mut chunk))
                .await
                .expect("timed out")
                .unwrap();
            assert!(read > 0, "closed before {needle:?}: {seen}");
            seen.push_str(&String::from_utf8_lossy(&chunk[..read]));
        }
        seen
    }

    /// End to end over a real socket: a new page gets the look and the replay,
    /// then live messages, and a deletion is forgotten for the next page too.
    #[tokio::test]
    async fn a_page_is_replayed_then_kept_live() {
        let look = OverlayLook::from_settings(&OverlaySettings::default());
        let server = OverlayServer::start(0, &look).await.unwrap();
        let port = server.port();

        server.message(&message("a", "first")).await;
        server.message(&message("b", "second")).await;
        server.delete("a").await;

        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(format!("GET /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let replay = read_until(&mut stream, r#""id":"b""#).await;
        assert!(replay.contains("event: look"));
        assert!(
            !replay.contains(r#""id":"a""#),
            "a deleted message is not replayed"
        );

        // Wait for the stream to be registered before writing to it.
        for _ in 0..100 {
            if server.listeners().await == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(server.message(&message("c", "third")).await, 1);
        read_until(&mut stream, r#""id":"c""#).await;

        server.clear_user("Third").await;
        read_until(&mut stream, "event: user\ndata: \"third\"").await;

        server.shutdown().await;
        // The port is free again, so moving back to it works at once.
        let again = OverlayServer::start(port, &look).await.unwrap();
        again.shutdown().await;
    }

    #[tokio::test]
    async fn the_page_is_served_with_the_current_look() {
        let mut settings = OverlaySettings {
            font_size: 33,
            ..Default::default()
        };
        let server = OverlayServer::start(0, &OverlayLook::from_settings(&settings))
            .await
            .unwrap();
        let port = server.port();

        settings.font_size = 41;
        server
            .set_look(&OverlayLook::from_settings(&settings))
            .await;

        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream
            .write_all(format!("GET / HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let page = read_until(&mut stream, "</html>").await;
        assert!(page.starts_with("HTTP/1.1 200 OK"));
        assert!(page.contains(r#""fontSize":41"#));
        server.shutdown().await;
    }
}

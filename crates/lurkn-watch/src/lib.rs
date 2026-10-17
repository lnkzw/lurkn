#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Pages Lurkn serves to a browser over loopback: the synced watch window, and
//! the chat overlay for OBS ([`overlay`]).
//!
//! A window that follows the channel you pick, without a browser extension.
//!
//! No browser lets a command line re-point a tab that is already open, and the
//! automation protocols that can (DevTools, Marionette) are per-engine and
//! amount to handing another process the keys to a browser profile. The way
//! round it is to own the page: Lurkn serves one page over loopback, the
//! browser stays on it, and only the embedded Twitch player changes channel.
//! See `docs/architecture.md` §7.2.
//!
//! Access rules, all of which are tested in [`http`]:
//!
//! - bound to 127.0.0.1, so nothing off this machine can reach it at all;
//! - a random per-session token on every request, so another local program
//!   cannot read which channels you open;
//! - a `Host` check, so a site that points its own name at 127.0.0.1 cannot
//!   talk to it as a same-origin page;
//! - `GET` for the page and its stream, `POST` for a font step, nothing else.
//!
//! Beside the player the page shows Lurkn's chat ([`chat`]): the messages
//! Lurkn shows, styled the way Lurkn shows them, so a window put on a TV
//! carries the conversation with it.

pub mod chat;
pub mod http;
pub mod overlay;
pub mod page;

use std::collections::VecDeque;
use std::sync::Arc;

use lurkn_core::Login;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};

pub use chat::{WatchLook, WatchMessage};
pub use http::Step;

pub type Result<T> = std::result::Result<T, WatchError>;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("could not open a local port for the watch window: {0}")]
    Bind(#[from] std::io::Error),
    #[error("could not generate a token for the watch window")]
    NoRandomness,
}

/// How often a comment is written to every open stream.
///
/// A browser window closed without a clean shutdown is only noticed when a
/// write to it fails, and [`WatchServer::set_channel`] reports how many
/// listeners it reached. Without this the count could stay stale for as long as
/// the user leaves Lurkn alone.
pub(crate) const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(15);

/// How many recent messages a new connection is replayed: enough to fill a
/// tall window at the smallest font, so a reloaded page comes back full.
const REPLAY: usize = 150;

struct Shared {
    token: String,
    nonce: String,
    port: u16,
    /// Open event streams. Nothing is ever read from these again, so they are
    /// parked here whole rather than split.
    clients: Mutex<Vec<TcpStream>>,
    current: Mutex<Option<Login>>,
    /// The look, serialised; `null` until the frontend has sent one.
    look: Mutex<String>,
    chat: Mutex<ChatLog>,
    requests: broadcast::Sender<Step>,
}

/// What the chat column shows: the channel Lurkn is reading, and its most
/// recent lines.
#[derive(Default)]
struct ChatLog {
    channel: Option<Login>,
    recent: VecDeque<WatchMessage>,
}

/// A running watch server. Dropping it does not stop it; it lives as long as
/// the app does.
#[derive(Debug, Clone)]
pub struct WatchServer {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is a credential; it never goes to the log.
        f.debug_struct("Shared").field("port", &self.port).finish()
    }
}

impl WatchServer {
    /// Bind a loopback port and start serving.
    pub async fn start() -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();

        let shared = Arc::new(Shared {
            token: random_hex()?,
            nonce: random_hex()?,
            port,
            clients: Mutex::new(Vec::new()),
            current: Mutex::new(None),
            look: Mutex::new("null".to_owned()),
            chat: Mutex::new(ChatLog::default()),
            requests: broadcast::channel(16).0,
        });

        let server = Self {
            shared: Arc::clone(&shared),
        };

        tokio::spawn(accept_loop(listener, Arc::clone(&shared)));
        tokio::spawn(heartbeat(shared));

        tracing::info!(port, "the watch window server is listening on loopback");
        Ok(server)
    }

    pub fn port(&self) -> u16 {
        self.shared.port
    }

    /// The URL to point a browser at. Carries the token, so it is a credential:
    /// it belongs in the browser and in nothing else.
    pub fn page_url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/?t={}",
            self.shared.port, self.shared.token
        )
    }

    /// Tell every open window to switch, and report how many were actually
    /// reached.
    ///
    /// Zero means there is no window listening any more and one has to be
    /// opened. The channel is remembered either way, so a window opened next
    /// arrives on the right stream.
    pub async fn set_channel(&self, login: &Login) -> usize {
        *self.shared.current.lock().await = Some(login.clone());
        let frame = format!("event: channel\ndata: {}\n\n", login.as_str());
        write_to_all(&self.shared.clients, frame.as_bytes()).await
    }

    /// The channel the windows were last told to show.
    pub async fn channel(&self) -> Option<Login> {
        self.shared.current.lock().await.clone()
    }

    /// How many windows are listening, as far as the last write could tell.
    pub async fn listeners(&self) -> usize {
        self.shared.clients.lock().await.len()
    }

    /// Font steps asked for from the page, as Ctrl and `+` or `-`. Lurkn owns
    /// the setting, so the page only asks; the new size comes back as a look.
    pub fn requests(&self) -> broadcast::Receiver<Step> {
        self.shared.requests.subscribe()
    }

    /// Restyle every open page in place. A look that has not changed is not
    /// sent: the frontend sends one on every settings change.
    pub async fn set_look(&self, look: WatchLook) {
        let json = serde_json::to_string(&look.sanitized()).unwrap_or_else(|_| "null".to_owned());
        {
            let mut current = self.shared.look.lock().await;
            if *current == json {
                return;
            }
            current.clone_from(&json);
        }
        write_to_all(&self.shared.clients, frame("look", &json).as_bytes()).await;
    }

    /// The channel Lurkn's chat switched to. What was said in the last one
    /// goes: it belongs to that channel.
    pub async fn set_chat_channel(&self, channel: Option<&Login>) {
        let json = {
            let mut chat = self.shared.chat.lock().await;
            if chat.channel.as_ref() == channel {
                return;
            }
            chat.channel = channel.cloned();
            chat.recent.clear();
            chat_json(chat.channel.as_ref())
        };
        write_to_all(&self.shared.clients, frame("chat", &json).as_bytes()).await;
    }

    /// New lines, in the order they arrived.
    pub async fn messages(&self, messages: Vec<WatchMessage>) {
        if messages.is_empty() {
            return;
        }
        let Ok(json) = serde_json::to_string(&messages) else {
            return;
        };
        {
            let mut chat = self.shared.chat.lock().await;
            chat.recent.extend(messages);
            let excess = chat.recent.len().saturating_sub(REPLAY);
            chat.recent.drain(..excess);
        }
        write_to_all(&self.shared.clients, frame("messages", &json).as_bytes()).await;
    }

    /// Backfill from before Lurkn joined, oldest first. It goes above
    /// whatever has arrived live since, and gives way to it when the replay
    /// is full.
    pub async fn history(&self, messages: Vec<WatchMessage>) {
        if messages.is_empty() {
            return;
        }
        let Ok(json) = serde_json::to_string(&messages) else {
            return;
        };
        {
            let mut chat = self.shared.chat.lock().await;
            let room = REPLAY.saturating_sub(chat.recent.len());
            let skip = messages.len().saturating_sub(room);
            for message in messages.into_iter().skip(skip).rev() {
                chat.recent.push_front(message);
            }
        }
        write_to_all(&self.shared.clients, frame("history", &json).as_bytes()).await;
    }

    /// A moderator removed one message. It stays, marked, as it does in Lurkn.
    pub async fn delete(&self, message_id: &str) {
        for message in self.shared.chat.lock().await.recent.iter_mut() {
            if message.id == message_id {
                message.deleted = true;
            }
        }
        self.send_string("delete", message_id).await;
    }

    /// A timeout or ban: everything that chatter said is marked removed.
    pub async fn clear_user(&self, login: &str) {
        for message in self.shared.chat.lock().await.recent.iter_mut() {
            if message.login.eq_ignore_ascii_case(login) {
                message.deleted = true;
            }
        }
        self.send_string("user", &login.to_ascii_lowercase()).await;
    }

    /// `/clear`.
    pub async fn clear(&self) {
        self.shared.chat.lock().await.recent.clear();
        write_to_all(&self.shared.clients, frame("clear", "null").as_bytes()).await;
    }

    async fn send_string(&self, event: &str, value: &str) {
        if let Ok(json) = serde_json::to_string(value) {
            write_to_all(&self.shared.clients, frame(event, &json).as_bytes()).await;
        }
    }
}

/// One server-sent event. `data` is JSON from serde, which escapes newlines,
/// so it cannot end the frame early or forge another.
fn frame(event: &str, data: &str) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

fn chat_json(channel: Option<&Login>) -> String {
    serde_json::to_string(&channel.map(Login::as_str)).unwrap_or_else(|_| "null".to_owned())
}

/// Write to every open stream, dropping the ones that fail. Returns how many
/// were written to.
pub(crate) async fn write_to_all(clients: &Mutex<Vec<TcpStream>>, bytes: &[u8]) -> usize {
    let mut clients = clients.lock().await;
    let mut alive = Vec::with_capacity(clients.len());

    for mut client in clients.drain(..) {
        if client.write_all(bytes).await.is_ok() && client.flush().await.is_ok() {
            alive.push(client);
        } else {
            tracing::debug!("a watch window went away");
        }
    }

    *clients = alive;
    clients.len()
}

async fn heartbeat(shared: Arc<Shared>) {
    let mut ticker = tokio::time::interval(HEARTBEAT);
    loop {
        ticker.tick().await;
        // A comment: valid in an event stream, ignored by the page, and enough
        // to find out whether the socket is still there.
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
                        tracing::debug!(%error, "a watch request ended early");
                    }
                });
            }
            Err(error) => {
                tracing::warn!(%error, "the watch listener stopped accepting");
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
            .write_all(http::error_response(http::Route::NotFound).as_bytes())
            .await?;
        return Ok(());
    };

    match http::route(&request, &shared.token, shared.port) {
        http::Route::Page => {
            let body = page::page_html(
                &shared.token,
                &shared.nonce,
                shared.current.lock().await.as_ref(),
                &shared.look.lock().await,
            );
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

        http::Route::Font(step) => {
            // Nobody listening just means Lurkn is between subscriptions; the
            // page gets its answer either way and the key press is lost.
            let _ = shared.requests.send(step);
            stream.write_all(http::NO_CONTENT.as_bytes()).await?;
        }

        http::Route::Events => {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\n\
                      Content-Type: text/event-stream\r\n\
                      Cache-Control: no-store\r\n\
                      Connection: keep-alive\r\n\
                      \r\n",
                )
                .await?;

            // Replay, so a window that reconnects lands on the current channel
            // instead of waiting for the next switch, styled and with the
            // conversation so far.
            if let Some(login) = shared.current.lock().await.as_ref() {
                let frame = format!("event: channel\ndata: {}\n\n", login.as_str());
                stream.write_all(frame.as_bytes()).await?;
            }
            let look = frame("look", &shared.look.lock().await);
            stream.write_all(look.as_bytes()).await?;
            {
                let chat = shared.chat.lock().await;
                let channel = frame("chat", &chat_json(chat.channel.as_ref()));
                stream.write_all(channel.as_bytes()).await?;
                if !chat.recent.is_empty() {
                    if let Ok(json) = serde_json::to_string(&chat.recent) {
                        stream
                            .write_all(frame("messages", &json).as_bytes())
                            .await?;
                    }
                }
            }
            stream.flush().await?;

            shared.clients.lock().await.push(stream);
            // Deliberately not returned to the caller: the stream stays open
            // and is written to from `set_channel`.
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

/// Read up to the end of the request head, or give up.
pub(crate) async fn read_head(stream: &mut TcpStream) -> std::io::Result<Option<String>> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);

        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > http::MAX_HEAD {
            // Nothing this server answers is this long.
            return Ok(None);
        }
    }

    Ok(String::from_utf8(buffer).ok())
}

/// 128 bits from the system's random source, as hex.
pub(crate) fn random_hex() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| WatchError::NoRandomness)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_128_bits_of_hex_and_never_repeats() {
        let a = random_hex().unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, random_hex().unwrap());
    }
}

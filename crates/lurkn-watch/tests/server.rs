//! The server as a browser meets it: a real socket, real requests.
//!
//! The routing rules are unit-tested; what these cover is the wiring — that a
//! window really receives a switch, that a closed window stops counting, and
//! that a reconnecting window is told where it should already be.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use lurkn_core::{Login, MessageKind};
use lurkn_watch::overlay::wire::Part;
use lurkn_watch::{Step, WatchMessage, WatchServer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

async fn request(port: u16, target: &str, host: &str) -> BufReader<TcpStream> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    stream.flush().await.unwrap();
    BufReader::new(stream)
}

async fn line(reader: &mut BufReader<TcpStream>) -> String {
    let mut out = String::new();
    tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut out))
        .await
        .expect("the server went quiet")
        .unwrap();
    out.trim_end().to_owned()
}

/// Read until a `data:` line, so header order and blank lines do not matter.
async fn next_data(reader: &mut BufReader<TcpStream>) -> String {
    for _ in 0..40 {
        let line = line(reader).await;
        if let Some(rest) = line.strip_prefix("data: ") {
            return rest.to_owned();
        }
    }
    panic!("no data frame arrived");
}

/// Read until the named event, and return its data. Other events — the look
/// and the chat replayed to every new stream — are skipped.
async fn next_event(reader: &mut BufReader<TcpStream>, name: &str) -> String {
    let wanted = format!("event: {name}");
    for _ in 0..80 {
        if line(reader).await == wanted {
            return next_data(reader).await;
        }
    }
    panic!("no {name} event arrived");
}

fn message(id: &str, login: &str) -> WatchMessage {
    WatchMessage {
        id: id.into(),
        login: login.into(),
        name: login.into(),
        color: None,
        badges: Vec::new(),
        kind: MessageKind::Chat,
        parts: vec![Part::Text { text: "hi".into() }],
        text: "hi".into(),
        at: 0,
        deleted: false,
        marked: false,
        mark: None,
        reply_to: None,
        event: None,
    }
}

fn login(name: &str) -> Login {
    Login::parse(name).unwrap()
}

#[tokio::test]
async fn a_window_is_told_to_switch() {
    let server = WatchServer::start().await.unwrap();
    let url = server.page_url();
    let token = url.rsplit("t=").next().unwrap().to_owned();

    let mut events = request(
        server.port(),
        &format!("/events?t={token}"),
        &format!("127.0.0.1:{}", server.port()),
    )
    .await;
    assert!(line(&mut events).await.contains("200 OK"));

    // The connection has to be parked before a switch can reach it.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(server.set_channel(&login("forsen")).await, 1);
    assert_eq!(next_event(&mut events, "channel").await, "forsen");

    assert_eq!(server.set_channel(&login("nymn")).await, 1);
    assert_eq!(next_event(&mut events, "channel").await, "nymn");
}

/// A window that reconnects must not sit blank until the next switch.
#[tokio::test]
async fn a_new_window_is_told_where_it_should_be() {
    let server = WatchServer::start().await.unwrap();
    let token = server.page_url().rsplit("t=").next().unwrap().to_owned();
    let host = format!("127.0.0.1:{}", server.port());

    server.set_channel(&login("forsen")).await;

    let mut events = request(server.port(), &format!("/events?t={token}"), &host).await;
    assert!(line(&mut events).await.contains("200 OK"));
    assert_eq!(next_event(&mut events, "channel").await, "forsen");
}

/// Zero listeners is the signal to open a browser, so a closed window has to
/// stop counting — otherwise the stream opens nowhere at all.
#[tokio::test]
async fn a_closed_window_stops_counting() {
    let server = WatchServer::start().await.unwrap();
    let token = server.page_url().rsplit("t=").next().unwrap().to_owned();
    let host = format!("127.0.0.1:{}", server.port());

    let mut events = request(server.port(), &format!("/events?t={token}"), &host).await;
    assert!(line(&mut events).await.contains("200 OK"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.set_channel(&login("forsen")).await, 1);

    drop(events);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The first write after the close is what discovers it; by the second the
    // count is right, which is what the caller acts on.
    server.set_channel(&login("nymn")).await;
    assert_eq!(server.set_channel(&login("nymn")).await, 0);
    assert_eq!(server.listeners().await, 0);
}

#[tokio::test]
async fn the_page_is_served_with_the_current_channel() {
    let server = WatchServer::start().await.unwrap();
    let token = server.page_url().rsplit("t=").next().unwrap().to_owned();
    let host = format!("127.0.0.1:{}", server.port());

    server.set_channel(&login("forsen")).await;
    let mut page = request(server.port(), &format!("/?t={token}"), &host).await;

    let mut body = String::new();
    for _ in 0..1000 {
        let line = line(&mut page).await;
        body.push_str(&line);
        if line.contains("show(\"forsen\")") {
            break;
        }
    }
    assert!(body.contains("show(\"forsen\")"), "{body}");
}

#[tokio::test]
async fn a_request_without_the_token_is_refused() {
    let server = WatchServer::start().await.unwrap();
    let host = format!("127.0.0.1:{}", server.port());

    let mut page = request(server.port(), "/", &host).await;
    assert!(line(&mut page).await.contains("403"));

    let mut events = request(server.port(), "/events", &host).await;
    assert!(line(&mut events).await.contains("403"));
}

/// A site that resolves its own name to 127.0.0.1 would otherwise be
/// same-origin with this server.
#[tokio::test]
async fn a_request_for_another_host_is_refused() {
    let server = WatchServer::start().await.unwrap();
    let token = server.page_url().rsplit("t=").next().unwrap().to_owned();

    let mut page = request(server.port(), &format!("/?t={token}"), "evil.example").await;
    assert!(line(&mut page).await.contains("421"));
}

/// A window opened mid-conversation shows the conversation, and a chat switch
/// takes the old channel's lines away with it.
#[tokio::test]
async fn a_new_window_gets_the_chat_so_far() {
    let server = WatchServer::start().await.unwrap();
    let token = server.page_url().rsplit("t=").next().unwrap().to_owned();
    let host = format!("127.0.0.1:{}", server.port());

    server.set_chat_channel(Some(&login("forsen"))).await;
    server
        .messages(vec![message("a", "first"), message("b", "second")])
        .await;
    server.delete("a").await;

    let mut events = request(server.port(), &format!("/events?t={token}"), &host).await;
    assert!(line(&mut events).await.contains("200 OK"));
    assert_eq!(next_event(&mut events, "chat").await, r#""forsen""#);
    let replay = next_event(&mut events, "messages").await;
    assert!(replay.contains(r#""id":"a""#) && replay.contains(r#""deleted":true"#));
    assert!(replay.contains(r#""id":"b""#));

    server.set_chat_channel(Some(&login("nymn"))).await;
    assert_eq!(next_event(&mut events, "chat").await, r#""nymn""#);

    let mut fresh = request(server.port(), &format!("/events?t={token}"), &host).await;
    assert!(line(&mut fresh).await.contains("200 OK"));
    assert_eq!(next_event(&mut fresh, "chat").await, r#""nymn""#);
    server.messages(vec![message("c", "third")]).await;
    let next = next_event(&mut fresh, "messages").await;
    assert!(
        !next.contains(r#""id":"b""#),
        "the old channel's lines went: {next}"
    );
    assert!(next.contains(r#""id":"c""#));
}

/// Ctrl and + in the window reaches Lurkn, which owns the setting.
#[tokio::test]
async fn a_font_step_reaches_lurkn() {
    let server = WatchServer::start().await.unwrap();
    let token = server.page_url().rsplit("t=").next().unwrap().to_owned();
    let mut requests = server.requests();

    let mut stream = TcpStream::connect(("127.0.0.1", server.port()))
        .await
        .unwrap();
    stream
        .write_all(
            format!(
                "POST /font?t={token}&step=bigger HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\n\r\n",
                server.port()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut reply = BufReader::new(stream);
    assert!(line(&mut reply).await.contains("204"));

    let step = tokio::time::timeout(Duration::from_secs(5), requests.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(step, Step::Bigger);
}

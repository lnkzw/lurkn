//! Run the synced window on its own, with a made-up chat, for looking at the
//! page in a real browser while working on it.
//!
//! `cargo run -p lurkn-watch --example watch -- [channel]`
//!
//! Prints the URL and stays up, sending a line every 1.5 seconds. Ctrl and +
//! or - in the page step the font here, the way Lurkn would.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lurkn_core::{ChannelEvent, ChannelEventKind, Login, MessageKind};
use lurkn_watch::overlay::wire::Part;
use lurkn_watch::{Step, WatchLook, WatchMessage, WatchServer};

const KAPPA: &str = "https://static-cdn.jtvnw.net/emoticons/v2/25/default/dark/2.0";

#[tokio::main]
async fn main() {
    let channel = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "forsen".to_owned());
    let channel = Login::parse(&channel).expect("a channel name");

    let server = WatchServer::start().await.unwrap();
    let mut look = WatchLook {
        scheme: "dark".into(),
        background: "#0e0e10".into(),
        surface: "#18181b".into(),
        border: "#2a2a2e".into(),
        text: "#efeff1".into(),
        text_dim: "#adadb8".into(),
        accent: "#9146ff".into(),
        highlight: "#9146ff".into(),
        ok: "#00c08b".into(),
        warn: "#ffb020".into(),
        font: "system-ui, sans-serif".into(),
        font_size: 13,
        timestamps: true,
        badges: true,
        emote_scale: 1.75,
        alternate: true,
        channel_events: true,
        replies: true,
    };
    server.set_look(look.clone()).await;
    server.set_channel(&channel).await;
    server.set_chat_channel(Some(&channel)).await;
    println!("{}", server.page_url());

    let mut steps = server.requests();
    let font_server = server.clone();
    tokio::spawn(async move {
        while let Ok(step) = steps.recv().await {
            look.font_size = match step {
                Step::Bigger => look.font_size.saturating_add(1),
                Step::Smaller => look.font_size.saturating_sub(1),
            };
            println!("font {}", look.font_size);
            font_server.set_look(look.clone()).await;
        }
    });

    server
        .messages(vec![
            line(
                "1",
                "chatter_one",
                Some("#1e90ff"),
                vec![text("hello chat "), kappa()],
            ),
            WatchMessage {
                event: Some(ChannelEvent {
                    kind: ChannelEventKind::Resub,
                    text: "generous_viewer subscribed at Tier 1. They've subscribed for 14 months!"
                        .into(),
                    count: Some(14),
                }),
                ..line(
                    "2",
                    "generous_viewer",
                    Some("#ff69b4"),
                    vec![text("still here")],
                )
            },
            WatchMessage {
                marked: true,
                ..line(
                    "3",
                    "someone",
                    Some("#00ff7f"),
                    vec![text("hey "), mention("you"), text(" look at this")],
                )
            },
            WatchMessage {
                reply_to: Some("1".into()),
                ..line("4", "replier", None, vec![text("hello to you too")])
            },
            WatchMessage {
                deleted: true,
                ..line(
                    "5",
                    "spammer",
                    Some("#ff4500"),
                    vec![text("buy followers at a very real site")],
                )
            },
            WatchMessage {
                kind: MessageKind::Action,
                ..line(
                    "6",
                    "waver",
                    Some("#daa520"),
                    vec![text("waves at the stream")],
                )
            },
        ])
        .await;

    let mut n = 6;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        n += 1;
        server
            .messages(vec![line(
                &n.to_string(),
                "regular",
                Some("#9acd32"),
                vec![text(&format!("line number {n}, a little longer so that it wraps onto a second row in the column")), kappa()],
            )])
            .await;
    }
}

fn line(id: &str, login: &str, color: Option<&str>, parts: Vec<Part>) -> WatchMessage {
    WatchMessage {
        id: id.into(),
        login: login.into(),
        name: login.into(),
        color: color.map(Into::into),
        badges: Vec::new(),
        kind: MessageKind::Chat,
        text: parts
            .iter()
            .map(|p| match p {
                Part::Text { text } | Part::Mention { text } => text.clone(),
                Part::Emote { name, .. } => name.clone(),
            })
            .collect(),
        parts,
        at: time::OffsetDateTime::now_utc().unix_timestamp() * 1000,
        deleted: false,
        marked: false,
        mark: None,
        reply_to: None,
        event: None,
    }
}

fn text(text: &str) -> Part {
    Part::Text { text: text.into() }
}

fn mention(name: &str) -> Part {
    Part::Mention { text: name.into() }
}

fn kappa() -> Part {
    Part::Emote {
        name: "Kappa".into(),
        url: KAPPA.into(),
        aspect: Some((28, 28)),
        zero_width: false,
    }
}

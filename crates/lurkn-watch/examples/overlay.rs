//! Run the chat overlay on its own, with a made-up chat, for looking at the
//! page in a real browser or adding it to OBS while working on it.
//!
//! `cargo run -p lurkn-watch --example overlay -- [port] [plain] [still]`
//!
//! Prints the URL and stays up, sending a line every 1.5 seconds.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lurkn_core::{OverlayLayout, OverlaySettings};
use lurkn_watch::overlay::wire::Part;
use lurkn_watch::overlay::{OverlayLook, OverlayMessage, OverlayServer};

const KAPPA: &str = "https://static-cdn.jtvnw.net/emoticons/v2/25/default/dark/3.0";

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let mut settings = OverlaySettings::default();
    for flag in args {
        match flag.as_str() {
            "plain" => settings.layout = OverlayLayout::Plain,
            // Headless screenshots freeze CSS animations part-way.
            "still" => settings.animate = false,
            _ => {}
        }
    }

    let server = OverlayServer::start(port, &OverlayLook::from_settings(&settings))
        .await
        .unwrap();
    println!("{}", server.url());

    let lines: [(&str, Option<&str>, bool, Vec<Part>); 4] = [
        (
            "forsen",
            Some("#0000ff"),
            false,
            vec![text("dark blue gets lifted")],
        ),
        (
            "viewer_two",
            None,
            false,
            vec![text("no colour picked "), emote()],
        ),
        (
            "Long_Name_Here",
            Some("#ff69b4"),
            false,
            vec![text(
                "a long line so the bubble has to wrap at least once across the width",
            )],
        ),
        ("someone", Some("#00ff7f"), true, vec![text("waves")]),
    ];

    let mut n = 0_usize;
    loop {
        let (name, color, action, parts) = &lines[n % lines.len()];
        server
            .message(&OverlayMessage {
                id: format!("m{n}"),
                login: name.to_lowercase(),
                name: (*name).to_owned(),
                color: color.map(str::to_owned),
                badges: Vec::new(),
                action: *action,
                parts: parts.clone(),
                at: (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64,
            })
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        n += 1;
    }
}

fn text(text: &str) -> Part {
    Part::Text { text: text.into() }
}

fn emote() -> Part {
    Part::Emote {
        name: "Kappa".into(),
        url: KAPPA.into(),
        aspect: Some((28, 28)),
        zero_width: false,
    }
}

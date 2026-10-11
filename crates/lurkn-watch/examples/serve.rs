//! Run the watch server on its own, for looking at the page in a real browser.
//!
//! `cargo run -p lurkn-watch --example serve -- forsen`
//!
//! Prints the URL and stays up. Nothing in the app uses this; it exists because
//! the one thing the tests cannot check is whether Twitch's embed is happy
//! being framed by a page on 127.0.0.1.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lurkn_core::Login;
use lurkn_watch::WatchServer;

#[tokio::main]
async fn main() {
    let server = WatchServer::start().await.unwrap();

    if let Some(name) = std::env::args().nth(1) {
        let login = Login::parse(&name).expect("that is not a channel name");
        server.set_channel(&login).await;
    }

    println!("{}", server.page_url());
    std::future::pending::<()>().await;
}

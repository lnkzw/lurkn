# Lurkn — Architecture

How Lurkn is built, and why. Kept current with the code: a section number cited
in a source comment (`docs/architecture.md` §5.3) points here, so numbering is
stable and sections are amended rather than renumbered.

Facts about Twitch and the emote providers were checked against live endpoints
and documentation on the dates given; sources are in §16.

---

## 0. Overview

Lurkn is a desktop client for the Twitch channels you follow: live status,
chat with third-party emotes, whispers and moderation. The stream itself plays
in the user's own browser, so watch time, channel points, subs and Bits keep
working the way Twitch intends.

Three facts about Twitch shaped the design more than anything else:

| # | Fact | Consequence |
|---|------|-------------|
| F1 | Twitch has **no PKCE** support. The only safe flow for a secret-less native app is the **Device Code Flow (DCF)**. | DCF, registered as a public client. §3 |
| F2 | DCF **public-client refresh tokens expire after 30 days**. | "Log in once, forever" is not achievable: periodic re-login, handled without interrupting chat. §3.4 |
| F3 | Twitch **recommends EventSub over IRC**; IRC chat commands are already removed. | Chat is EventSub for reading and Helix for writing. IRC is not implemented. §5 |

---

## 1. Repository structure

Rust is split into a Cargo workspace so that every piece of domain logic is
testable without booting Tauri or a WebView. `src-tauri` is a thin shell that
does wiring only.

```text
lurkn/
├── crates/
│   ├── lurkn-core/      # pure domain: types, rules, settings, themes. No IO.
│   ├── lurkn-auth/      # device code login, token lifecycle, keyring storage
│   ├── lurkn-twitch/    # Helix client: users, follows, streams, badges, moderation
│   ├── lurkn-chat/      # EventSub transport, Helix send, history backfill
│   ├── lurkn-emotes/    # Twitch / 7TV / BTTV / FFZ providers and the resolver
│   ├── lurkn-browser/   # browser discovery and launching
│   ├── lurkn-watch/     # loopback pages: the synced window (§7.2), the OBS overlay (§7.3)
│   └── lurkn-store/     # SQLite cache, settings, whispers
├── src-tauri/
│   ├── tauri.conf.json  # plus tauri.{linux,windows,macos}.conf.json overrides
│   ├── capabilities/    # what the WebView may call (§11)
│   └── src/
│       ├── state.rs     # AppState, the single source of truth (§9.1)
│       ├── commands/    # #[tauri::command] fns — thin, no logic
│       ├── events.rs    # typed events (§8.1)
│       ├── tasks/       # background work: chat pump, follows poller, emotes, history
│       └── …            # tray, theme files, overlay, synced window glue
├── src/                 # Svelte 5 + TypeScript frontend
│   └── lib/
│       ├── ipc/         # generated bindings (tauri-specta) — do not hand-edit
│       ├── stores/      # read-models mirroring Rust state
│       ├── chat/        # filters, commands, completion, activity
│       ├── components/
│       └── ui/          # shortcuts, anchoring, platform
├── docs/                # this file, emote-precedence.md, release.md
└── scripts/             # version bump
```

Dependency direction is one-way:

```text
core  ←  auth, twitch, chat, emotes, browser, watch, store  ←  src-tauri  ←  src (Svelte)
```

`lurkn-core` depends on nothing but `serde`, `specta`, `thiserror`, `time`,
`url` and `regex`. No Twitch crate references `lurkn-browser` or `lurkn-watch`:
browser-specific code cannot leak into the Twitch services without a new Cargo
edge, which review catches.

---

## 2. Technology choices

| Concern | Choice | Why |
|---|---|---|
| Shell | Tauri 2 | small binaries, the system WebView, Rust for everything that touches the network |
| Frontend | Svelte 5 (runes) + TypeScript + Vite | |
| Rust↔TS types | `tauri-specta` + `specta` | one definition, generated TS; removes the drift class of bugs |
| HTTP | `reqwest` (rustls) | no system TLS surprises |
| Async | `tokio` | chat socket, poller and emote fetches concurrently |
| WebSocket | `tokio-tungstenite` | EventSub transport |
| Secrets | `keyring` | Secret Service / KWallet, Keychain, Credential Manager |
| Cache | `rusqlite` (bundled) | survives restart, one file, no daemon |
| Logging | `tracing` + rolling file | backs the log view in Settings → Advanced |
| Tests | `cargo test`, `wiremock`, `vitest` + Testing Library | §12 |

The Rust toolchain is pinned in `rust-toolchain.toml`.

---

## 3. Twitch authentication

### 3.1 Flow decision

Twitch offers implicit, authorization-code, DCF and client-credentials. For a
distributed desktop binary:

- **Authorization code** requires a `client_secret`. Shipping it in the binary is publishing it. **Rejected.**
- **Implicit** returns a token with **no refresh token** — the user re-logs in every ~4h. **Rejected.**
- **Authorization code + PKCE** would be ideal. **Twitch does not support it.** Revisit when it ships.
- **Device Code Flow**, registered as a **public client**: no secret, returns a refresh token, refresh works without a secret. **Selected.**

### 3.2 Flow

```text
POST https://id.twitch.tv/oauth2/device
     client_id, scopes
  → device_code, user_code, verification_uri, expires_in, interval

UI shows:  "Go to twitch.tv/activate and enter  ABCD-EFGH"
           [Open in browser]   ← reuses the browser launcher (§7). No new mechanism.

poll POST https://id.twitch.tv/oauth2/token
     client_id, device_code, grant_type=urn:ietf:params:oauth:grant-type:device_code
     every `interval` s, honouring authorization_pending / slow_down
  → access_token, refresh_token, expires_in (~14820s ≈ 4h1m), scope
```

The whole setup is one screen and one browser tab.

#### Wire facts, verified against live Twitch on 2026-09-20

Twitch's DCF documentation disagrees with itself, so these were captured from
real responses rather than read off the docs:

| | |
|---|---|
| Scope parameter | `scopes` (not OAuth's usual `scope`). **Twitch returns 200 for both**, so the wrong one yields a token granting nothing. `scopes` is the working one. |
| Device response | `{device_code, user_code, verification_uri, expires_in: 1800, interval: 5}` |
| `verification_uri` | already carries the code: `https://www.twitch.tv/activate?device-code=XXXXXXXX` |
| Error envelope | `{"status":400,"message":"authorization_pending"}` — **not** RFC 8628's `{"error":"..."}`. A stock OAuth client misparses this. |
| Error values seen | `authorization_pending`, `invalid device code`, `Invalid refresh token`, `missing authorization token` (401). Prose, not codes, so matching is case-insensitive and substring-based. |
| Refresh | succeeds with no `client_secret`, confirming the public-client registration. Twitch **rotates** the refresh token, so the stored copy must be replaced. |
| Validate | `GET /oauth2/validate` with `Authorization: OAuth <token>` (not `Bearer`). |

Because the endpoint accepts either spelling, the granted `scope` array is
checked against the required scopes after every token exchange and refresh. A
short token fails at login with a message the user can act on, rather than
producing 401s three screens later.

Verified end to end on 2026-09-20: device code login → all scopes granted →
token written to the Secret Service keyring (entry `lurkn/twitch-session`) →
restart → session restored from the keyring and re-validated. Neither the dev
log nor the rolling app log contained a token, a device code or an
`Authorization` value.

### 3.3 Scopes

The minimum, in two tiers (`lurkn-auth::scopes`):

| Scope | Needed for | When |
|---|---|---|
| `user:read:follows` | `Get Followed Channels`, `Get Followed Streams` | always |
| `user:read:chat` | `channel.chat.*` EventSub subscriptions | always |
| `user:write:chat` | `POST helix/chat/messages` | always |
| `user:manage:whispers` | sending whispers and the `user.whisper.message` subscription | when whispers are turned on |
| `user:read:moderated_channels`, `moderator:manage:banned_users`, `moderator:manage:chat_messages`, `moderator:manage:chat_settings`, `moderator:manage:announcements` | moderation, as one bundle | when moderation is granted in Settings |

Optional scopes are kept apart from the required ones on purpose: the granted
set is checked against the required list after every login and refresh, so a
scope added there would fail the check for every session whose token predates
it. An optional scope is requested when the user opts in, and asking for one
re-requests whatever is already granted, so turning one feature on never turns
another off.

Deliberately **not** requested: `user:read:email` (identity comes from
`/oauth2/validate` plus `Get Users`), and the IRC-only `chat:read` /
`chat:edit`.

### 3.4 Token lifecycle — and the 30-day wall (F2)

```text
access_token   ~4h      → refresh silently at 80% of lifetime, and on any 401
refresh_token  30 days  → public clients only. Not renewable by refreshing.
```

After 30 days the user must log in again. There is no way around this for a
secret-less client: periodic re-auth is the price of never holding a secret.
Handling:

- Refresh proactively in the background; never let a user action hit an expired token.
- On refresh failure, **do not log out and do not show a modal**. Keep the cached follow list on screen, switch chat to read-only, and show an inline banner:

```text
  Your Twitch session expired.   [Log in again]
```

- Store `access_token` + `refresh_token` in the OS keyring under one entry. Where no keyring is available (a real case on minimal Linux setups), fall back to `session.json` in the platform's local data folder (`~/.local/share/lurkn` on Linux, honouring `XDG_DATA_HOME`) at mode `0600` **and say so in Settings** — a silent downgrade to the weaker option is not acceptable.

### 3.5 The Twitch application

Lurkn ships with a registered client id (`lurkn-auth::DEFAULT_CLIENT_ID`). A
client id is a public identifier, not a secret — the *client secret* is the
thing Lurkn never has, and the device code flow does not need one. Every
native client compiles its client id in the same way.

The application is registered at <https://dev.twitch.tv/console/apps> as a
**public client** with the **Device Code Flow** enabled. The registration form
asks for a redirect URI even though DCF never uses one; `http://localhost` is
fine. `LURKN_CLIENT_ID` overrides the compiled-in id at runtime, for
development against a throwaway app.

---

## 4. Twitch API layer

### 4.1 Client

One `HelixClient`, injected. It owns the auth header, 401 → refresh → retry
once, `Ratelimit-Remaining` / `Ratelimit-Reset` accounting, and pagination.
Service modules (users, follows, streams, channels, badges, moderation) are
thin typed wrappers. No Twitch HTTP happens anywhere else.

### 4.2 Endpoints

| Purpose | Endpoint | Page size |
|---|---|---|
| identity | `GET /oauth2/validate`, `GET helix/users` | — |
| followed list | `GET helix/channels/followed` | 100 |
| live subset | `GET helix/streams/followed` | 100 |
| offline metadata | `GET helix/channels?broadcaster_id=…` | 100 ids/call |
| avatars | `GET helix/users?id=…` | 100 ids/call |
| badges | `GET helix/chat/badges/global`, `…/chat/badges` | — |
| Twitch emotes | `GET helix/chat/emotes/global`, `…/emotes?broadcaster_id=` | — |
| sending | `POST helix/chat/messages`, `POST helix/whispers` | — |
| moderation | `helix/moderation/*`, `helix/chat/settings`, `helix/chat/announcements` | — |

### 4.3 Polling

`Get Followed Streams` **already returns only live channels**, with title,
game, viewer count and thumbnail. So the live-status loop is:

```text
every 60s (jittered ±10s):
    GET helix/streams/followed   → the live set, paginated
    diff against previous live set → emit only the delta
```

No per-channel calls, no id-batching step. The full followed list
(`/channels/followed`) and offline metadata refresh on a much slower cadence
(~15 min) or on explicit refresh, since follows rarely change.

Cost for a user with 500 follows: 5 requests/min steady-state against an
800-point/min bucket. The interval is a constant, not a user setting.

---

## 5. Chat transport — EventSub

**EventSub WebSocket for reading, Helix `Send Chat Message` for writing. IRC is
not implemented.**

Twitch's own guidance is *"Twitch IRC has some limitations versus EventSub, and
is more complicated to parse, so it is recommended that you use EventSub
subscriptions and API calls instead."* Chat commands over IRC were removed in
2023, non-SSL WebSocket was decommissioned in August 2025, and Twitch has
signalled concurrent-join limits on IRC in favour of EventSub.

The transport sits behind a trait, which is what keeps the UI clean:

```rust
#[async_trait]
pub trait ChatTransport: Send + Sync {
    async fn connect(&mut self, creds: &ChatCreds) -> Result<()>;
    async fn join(&mut self, channel: &ChannelRef) -> Result<()>;
    async fn part(&mut self, channel: &ChannelRef) -> Result<()>;
    async fn send(&mut self, channel: &ChannelRef, text: &str) -> Result<()>;
    fn events(&self) -> broadcast::Receiver<ChatEvent>;   // normalized
}
```

`ChatEvent` is the normalized type; **nothing above this trait ever sees an
EventSub envelope.** `join`/`part` describe intent, not wire protocol.

### 5.1 Verified protocol facts (2026-09-20)

| | Value |
|---|---|
| WebSocket | `wss://eventsub.wss.twitch.tv/ws`, optional `?keepalive_timeout_seconds=` (10–600) |
| Handshake | `session_welcome` → `payload.session.id`; **10 s to create the first subscription** |
| Subscribe | `POST helix/eventsub/subscriptions`, `transport: { method: "websocket", session_id }`, **user access token required** (not app token) |
| Read | `channel.chat.message` **v1**, condition `{ broadcaster_user_id, user_id }`, scope `user:read:chat` |
| Send | `POST helix/chat/messages` `{ broadcaster_id, sender_id, message }`, scope `user:write:chat` |
| Keepalive | no event **or** keepalive within `keepalive_timeout_seconds` ⇒ treat as dead, reconnect |
| Reconnect | `session_reconnect` arrives ~30 s before close with a `reconnect_url`; **do not close the old socket until `session_welcome` lands on the new one**. Close code 4004 if too slow. |
| Revocation | `revocation` message ⇒ subscription is gone for good (user unauthorized / version retired); surface as an auth error, do not retry |
| Limits | 3 sockets per user token, 300 enabled subscriptions per socket, `max_total_cost` 10 |

On cost: a subscription whose `user_id` condition is the authorizing user costs
**0**, so the `max_total_cost` ceiling does not constrain Lurkn. 300 per socket
is the real ceiling.

### 5.2 Service internals

```text
lurkn-chat/src/
├── transport.rs      # ChatTransport trait, ChatEvent, ChatCreds
├── eventsub/
│   ├── socket.rs     # connect, welcome, keepalive watchdog, session_reconnect dance
│   ├── envelope.rs   # pure fn(&str) -> Result<Envelope>   ← unit-testable, no io
│   ├── chat.rs       # channel.chat.message → ChatMessage
│   ├── notification.rs # channel.chat.notification → channel events (subs, raids, …)
│   ├── moderation.rs # clears, deletions, room settings
│   ├── whisper.rs    # user.whisper.message
│   └── subs.rs       # desired-set reconciliation: create/delete subscriptions
├── dedupe.rs         # message_id LRU — Twitch may redeliver
├── backoff.rs        # 1s→30s full jitter
├── api.rs            # Helix send + client-side rate limit
└── history.rs        # backfill from recent-messages.robotty.de (IRCv3 lines)
```

Three things a naive EventSub client gets wrong:

1. **Deduplication.** EventSub may redeliver a notification. Every `message_id` goes through a bounded LRU; repeats are dropped before reaching `AppState`. Without this, duplicate chat lines appear under flaky networks.
2. **The reconnect overlap.** `session_reconnect` requires running two sockets briefly. Closing the old one early drops messages; never closing it leaks. The watchdog owns both and swaps only on the new `session_welcome`.
3. **Subscription reconciliation.** Channel switching is `DELETE` + `POST` over HTTP, not a cheap `JOIN`, so it is ~200 ms and can fail independently of the socket. The UI shows the channel immediately and chat fills in (§8.3); a failed subscribe surfaces as `ChatEvent::State(Failed)`, not a silent empty log.

### 5.3 Beyond chat messages

`channel.chat.message` carries no room or user state. A joined channel
therefore holds **six** subscriptions, all with the reader as `user_id` and so
all at cost 0:

- `channel.chat.message` — the chat itself;
- `channel.chat_settings.update` (with an underscore — Twitch's name, not the `channel.chat.*` pattern) — room modes, **plus `Get Chat Settings` on join**: the subscription fires only on a *change*, so a channel already in followers-only would otherwise look unrestricted;
- `channel.chat.clear`, `channel.chat.clear_user_messages`, `channel.chat.message_delete` — moderation;
- `channel.chat.notification` — subs, gifts, raids, announcements, for the log and the activity feed.

Per user rather than per channel: `user.whisper.message` when whispers are on,
and `stream.online` / `stream.offline` for the reader's own channel.

The upside over IRC, and the reason parsing is simple: the payload arrives
**pre-fragmented** (`text` / `emote` / `cheermote` / `mention`), with badges as
structured `{set_id, id, info}`. The whole class of UTF-16 emote-offset bugs
disappears — §6.2 step 1 is a direct fragment mapping, not index arithmetic.

**History.** Twitch serves no chat history. With the reader's opt-in, the
backfill on join comes from `recent-messages.robotty.de`, which answers in raw
IRCv3 lines; `history.rs` parses those into the same `ChatMessage` and they go
through the same ignores, highlights and emote resolution as live chat. Off by
default, because turning it on tells that service which channels are opened.

### 5.4 Rate limiting

Sending is HTTP, so it shares the 800-point/min Helix bucket, but Twitch's
per-channel chat limits still apply (20 messages / 30 s, higher for
moderators) and are enforced **client-side** — Twitch silently drops excess,
which would read as a Lurkn bug. Subscription churn is serialized so rapid
channel-hopping cannot burst it.

---

## 6. Emotes

### 6.1 Providers

```rust
#[async_trait]
pub trait EmoteProvider: Send + Sync {
    fn name(&self) -> ProviderId;
    async fn global(&self) -> Result<Vec<Emote>>;
    async fn channel(&self, broadcaster_id: &str) -> Result<Vec<Emote>>;
}
```

Endpoints (verified 2026-09-20, all unauthenticated):

| Provider | Global | Channel |
|---|---|---|
| Twitch | `helix/chat/emotes/global` | `helix/chat/emotes?broadcaster_id=` |
| 7TV | `https://7tv.io/v3/emote-sets/global` | `https://7tv.io/v3/users/twitch/{id}` → `emote_set.emotes` |
| BTTV | `https://api.betterttv.net/3/cached/emotes/global` | `https://api.betterttv.net/3/cached/users/twitch/{id}` → `channelEmotes` + `sharedEmotes` |
| FFZ | `https://api.frankerfacez.com/v1/set/global` | `https://api.frankerfacez.com/v1/room/id/{id}` |

A failing third-party provider is **logged and skipped**, never propagated:
with 7TV down, Twitch emotes keep working. Each provider has its own timeout.

### 6.2 Resolution and precedence (normative)

Twitch-native emotes are **not** name-matched. EventSub delivers the message
pre-split into fragments, and an `emote` fragment is authoritative:

```text
1. Map EventSub fragments directly:
     fragment.emote     -> ChatToken::Emote (provider = Twitch)
     fragment.mention   -> ChatToken::Mention
     fragment.text      -> continue to step 2
2. Tokenize each text fragment on whitespace.
3. Look each word up in the channel's resolved name→Emote map.
4. Anything unmatched stays Text.
```

The name→Emote map is built once per channel, later inserts losing to earlier
ones:

```text
7TV channel  >  BTTV channel  >  FFZ channel  >  7TV global  >  BTTV global  >  FFZ global
```

Channel-specific always beats global — that is what the broadcaster chose — and
7TV > BTTV > FFZ matches the ordering users expect. Specified in
`docs/emote-precedence.md`; collisions are asserted in tests, not left to
insertion order.

### 6.3 Rendering

Chat produces `Vec<ChatToken>` in Rust. Svelte renders one element per token
variant. **No raw HTML, ever** (§11): text goes through Svelte's escaped
interpolation, and emotes become `<img>` with a URL validated against an
allowlist of provider hosts before it leaves Rust:

```text
static-cdn.jtvnw.net · cdn.7tv.app · cdn.betterttv.net · cdn.frankerfacez.com
```

A URL from any other host is dropped and the emote falls back to its text. Link
tokens are rendered as `<a>` only after Rust confirms the scheme is `http` or
`https`, and a click routes through the browser launcher — never the WebView.

---

## 7. Browser launcher

```rust
pub trait BrowserLauncher: Send + Sync { fn open_url(&self, url: &Url) -> Result<()>; }
```

The default is the system's own handler through Tauri's opener plugin
(`xdg-open` on Linux, `open` on macOS, the shell on Windows), and it is the
fallback for everything below.

URL construction is a pure function in `lurkn-core` (`twitch_channel_url(login)
-> Url`) over a validated login charset, so no chat-supplied string can reach
the launcher. A launch failure reads *"Could not open your browser. Please
check your system's default browser settings."*, with the detail in the log.

### 7.1 Choosing a browser (Linux)

`lurkn-browser` also finds installed browsers and launches one directly, so a
stream can open in a chosen browser and window mode. Discovery reads
freedesktop desktop entries, which only Linux has; on Windows and macOS the
choice is the system default or a custom command, and the mode options are
hidden.

**Discovery is the desktop's own answer, not a list of browser names.**
`detect.rs` reads desktop entries from the XDG application directories and keeps
those whose `MimeType` claims `x-scheme-handler/http(s)`, that are visible
(`NoDisplay`/`Hidden`/`Terminal`), and whose `TryExec`/program exists — the same
source the system's "open with" dialog uses. A browser with no desktop entry is
not listed; the custom command covers it.

**Family classification is by name, because nothing better exists.** A desktop
entry says what to run, not what it is. `family.rs` searches the command line
for markers, which also catches Flatpak entries (`app/org.mozilla.firefox/…`).
Anything unrecognised is `Family::Other` and receives a plain URL: a wrong guess
means a browser that refuses to start.

| Family | App mode | Kiosk mode |
| --- | --- | --- |
| Chromium (Chrome, Brave, Vivaldi, Opera, Edge, …) | `--app=URL` | `--kiosk URL` |
| Firefox (Firefox, LibreWolf, Zen, Floorp, Mullvad, …) | none | `--kiosk URL` |
| Other | none | none |

Firefox has no app mode, so a request for one steps down to kiosk rather than
failing or silently opening an ordinary window. `Family::Other` steps down to a
plain URL.

**The system default can still take a mode.** `BrowserChoice::System` with a
non-normal mode resolves the default through `xdg-mime query default
x-scheme-handler/https` and launches it like any installed browser; if that
cannot be resolved, it falls back to the opener plugin with no flags.

**A custom command is an argv template, never a shell line.** `%u` marks the
URL, as in a desktop entry; without one the URL is appended. Splitting honours
quotes and nothing else: `$HOME`, `~`, `&&` and `|` are literal characters,
because the program and each argument go to the OS separately. Test
`shell_operators_are_not_operators` pins this.

**A new window is a normal-mode concern only.** App and kiosk already open a
window of their own, and passing both flags confuses Chromium, so
`--new-window` is added only in normal mode and only for a recognised family.

**Theatre mode is not reachable from a URL.** Twitch keeps it per account;
there is no query parameter for it (checked 2026-09-21). What a URL *can* ask
for is the bare player, `https://player.twitch.tv/?channel=<login>&parent=twitch.tv`.
`StreamPage::Player` uses it: video alone, no Twitch chat, no panels. This
applies only to opening a channel; a link clicked in chat opens as written.

**No silent substitution.** A browser named in settings that is no longer
installed produces a message naming the problem rather than quietly opening
something else.

### 7.2 The synced window (experimental)

Opening a second channel normally leaves a second tab, and nothing on a command
line can reuse one: `firefox URL` and `chromium URL` hand off to the running
instance, but always as a new tab.

Three routes exist. **Chromium DevTools Protocol** — launch with
`--remote-debugging-port` and a dedicated profile, then `Page.navigate` an
existing target. Chromium-only, and a debugging port is full control of that
browser profile for any local process. **A browser extension** — a second
product to build, ship and keep working in every browser. **Owning the page** —
chosen.

Lurkn serves one page over loopback (`lurkn-watch`). The browser stays on it for
the whole session, and only the embedded Twitch player changes channel, so the
connection that makes switching possible is never torn down. It works in every
browser, with no automation protocol, no dedicated profile and no extension.

```text
open_channel(login)
  └─ sync_window on
       ├─ watch_server().set_channel(login)  → listeners > 0  → done, the window switched
       └─ listeners == 0                     → open page_url() through the normal launcher
```

The listener count comes from an actual write, not a cached flag: a window
closed behind Lurkn's back fails its write, is pruned, and the next open
launches a fresh one. A 15-second heartbeat keeps that count from going stale
while Lurkn is idle. The server replays the current channel to every new
connection, so a window that reconnects — `EventSource` does that on its own —
lands on the right stream.

**Server-sent events, not a WebSocket.** The traffic is almost all one way,
Lurkn → page. SSE is plain HTTP with no upgrade handshake, the browser
reconnects by itself, and it kept the server small enough to own outright
rather than adding an HTTP framework.

**Access rules**, all unit-tested in `lurkn-watch::http`:

- bound to `127.0.0.1`, so nothing off this machine can reach it;
- a 128-bit token, fresh per process, required on every request;
- a `Host` check — a site that points its own name at 127.0.0.1 would otherwise be same-origin with the server, and the token would not help because a rebound page can read responses;
- `GET` for the page and its stream, `POST /font` for a font step (below), nothing else;
- the page carries `default-src 'none'` with a per-process nonce, so it may talk to Twitch's player and back to Lurkn, load emote and badge images from the providers' CDNs, and nothing else.

The token travels in the URL handed to the browser, so it is visible in that
process's command line to the user's own processes — the same exposure as any
localhost tool that authenticates this way. It grants knowledge of which
channel is playing and one font setting.

Twitch's embed builds its player from a page on `127.0.0.1`
(`parent=127.0.0.1&parent=localhost`) and plays — verified 2026-09-21 by
screenshotting the served page in headless Chromium.

With `follow_chat` on as well, a channel selection re-points the window too —
never opening one, so §8.3's rule survives.

**The chat beside the player.** The page is laid out like Twitch's: the
player, and Lurkn's chat in a column down the right edge, folded away by
default and brought in by an arrow that shows only while the mouse moves. The
point is a window put on a TV — browser fullscreen, stream and chat together —
so it is read-only and has no composer.

- *What it shows* is exactly Lurkn's log: the pump forwards each batch after `screen` and `resolve` (`src-tauri/src/watch.rs`), so ignores, highlights, emotes and badges are decided once, in Rust, for both. Backfill and moderation follow the same path; a removed line stays, struck through, as in Lurkn. The frontend's expression filters do not apply — they run in the WebView — and system lines are left out.
- *Which channel*: the one Lurkn is reading (`set_current_channel` tells the server). The page shows it only while the player is on the same channel; otherwise it says which channel Lurkn is reading rather than putting one stream beside another's chat. `follow_chat` keeps the two together.
- *The look* is sent by the frontend (`watch_set_look`), read back off the document's computed style after the theme is applied, so a theme file and the system preference count without a second copy of the palette in Rust. Rust sanitises every value before it is stored; the page only ever sets them as custom properties.
- *Replay*: the last 150 lines go to every new connection, so a reloaded window comes back full. Rust keeps no log of its own, so a server started mid-conversation begins empty.
- *Ctrl + / −* steps `chat.font_size` from either window. Lurkn handles it as a shortcut; the page `POST`s `/font?t=…&step=bigger|smaller`, the server hands the step to Lurkn over a broadcast channel, and Lurkn saves it like any settings change — the new size returns to the page as a look. A `POST`, because it changes a setting: nothing that only loads a URL can trigger it, and a form on another site cannot carry the token. The keys reach the page only while it, not Twitch's frame, has the keyboard; the page takes it back when the mouse moves over its own part of the window.

**Costs**, stated in the settings hint rather than hidden: it is Twitch's player
rather than the channel page, so the channel-page/player-page setting does not
apply; playback follows the browser's autoplay policy and may need one click;
an age-gated stream shows its gate inside the window.

Settings compatibility: `Settings.browser` used to be the string
`"systemDefault"` and is now a group. Serde's `default` only covers a *missing*
field, so a custom deserializer accepts either shape — without it, an older
settings file would fail to parse and the user would lose every other setting.

### 7.3 The chat overlay for OBS

A channel's chat, served over loopback as a page for an OBS browser source
(`lurkn-watch::overlay`, wired in `src-tauri/src/overlay.rs`). Off by default;
switched on under Settings → Streaming.

**The own channel by default.** A streamer who clicks over to the channel they
are about to raid must not take their overlay with them. While the overlay runs,
the reader's channel is subscribed alongside the open one —
`AppState::send_channels` sends `open ∪ own`, under one lock so a channel switch
and an overlay change cannot each send a set missing the other's channel. Six
more subscriptions at cost 0 (§5.1). The pump routes by `channel_id`
(`overlay::audience`): the own channel's events go to the overlay, and to the
log only when it is also the open channel. That includes moderation — a deleted
message or a timed-out chatter disappears from the stream too — and room state,
which must not overwrite the open channel's. The overlay keeps its own emote
registry and badge map for the same reason.

**Or the open channel** (`OverlaySource::Open`), for watch-alongs and
co-streams. Then the overlay's channel *is* the open one: no extra
subscription, and `select_channel` re-runs `overlay::sync`, which re-points it
and clears the page. Opt-in, because it moves the overlay on every switch.

**Chat only** (`wire::admits`): no channel events (subs, raids, announcements —
the activity feed's), no system lines; a cheer stays, being a chatter's own
message. `!commands` and a list of bot logins are left out by default. The
reader's ignores apply, except to their own lines, as in the log.

**Same server shape as §7.2, two differences**, both because OBS keeps the URL
in a saved scene:

- a **fixed port** (default 7373, below Linux's ephemeral range), configurable;
- **no token** — everything on the page is already public on twitch.tv, and a token would have to be stored, shown and rotated for nothing. The `Host` check stays (it is what stops DNS rebinding); a plain cross-origin page cannot read the stream since no CORS headers are sent.

**Live styling.** The look (`OverlayLook`) is data, sent as an SSE `look` event
on every settings change and applied as CSS custom properties, so OBS restyles
without a refresh — the OBS preview *is* the preview. A new connection is
replayed the look and the last 50 messages (minus deleted ones); each message
carries its timestamp so a fade-out survives the replay. `?demo` loops sample
lines for judging the look in a browser.

**The page writes no markup**: `createElement`/`textContent` only, images from
the provider allowlist (also enforced by its CSP, `script-src` nonce-only).
`style-src` allows inline styles on purpose: OBS applies a source's *Custom
CSS* by inserting a `<style>`, and a nonce would silently drop it. The class
names (`.msg`, `.name`, `.body`, `.emote`, `.badge`, …) are stable for that
reason. Twitch name colours are lifted toward white or black until they reach
4.5:1 against the bubble (or the outline), since Twitch allows colours that
vanish on a dark background.

`cargo run -p lurkn-watch --example overlay -- 7373` and `--example watch`
serve a made-up chat for working on either page without Twitch.

---

## 8. Rust ↔ Svelte IPC model

Rust owns all state. Svelte holds **read-models** it never mutates directly.

### 8.1 Three channels of communication

**1. Commands** — frontend → Rust, request/response, for user intent:
`select_channel`, `send_message`, `open_channel`, `settings_set`,
`moderate_ban`, `whisper_send`, `log_tail`, and so on. Commands are thin:
validate, delegate to a crate, return. No business logic in `src-tauri`.

**2. Events** — Rust → frontend, state changes: `AuthChanged`,
`FollowsUpdated`, `LiveDelta`, `ChatStateChanged`, `EmotesReady`,
`SettingsChanged`, `ThemeChanged`, `WhisperReceived`, `ModerationEvent`, ….
Every window hears them, which is what keeps a second window (§8.6) in step for
free.

**3. Batched chat** — Rust → frontend, high-frequency, as a coalesced event.
Chat in a big stream is hundreds of messages a second, and one IPC round trip
per message saturates the bridge. Rust accumulates messages and emits them
**coalesced on an 80 ms tick**, capped at 200 per batch:

```rust
struct ChatBatch { messages: Vec<ChatMessage> }
```

The frontend appends a batch at a time and trims to a ring buffer of 500
rendered messages. This is the single most important performance decision in
the design. A `tauri::ipc::Channel` would be the upgrade if profiling ever
showed event dispatch itself to be the bottleneck.

A state change flushes the pending batch before it is published, so a
"reconnecting" banner never appears ahead of messages that arrived before it.
If the consumer falls behind the broadcast buffer, the lag is logged rather
than hidden — old lines are dropped, which is the right trade for live chat.

### 8.2 Typed bindings

`tauri-specta` generates `src/lib/ipc/bindings.ts` from the Rust command and
event definitions, on every debug run and on `cargo test`; CI fails if the
committed file differs. Hand-written TS mirrors of Rust structs are not allowed
— they are the main source of silent breakage in Tauri apps.

### 8.3 Channel switching

`select_channel(login)` is the only entry point, and its ordering matters:

```text
select_channel("foo")            ← single click / arrow keys. Chat only.
  1. AppState.current_channel = foo          (instant — the UI responds immediately)
  2. emote sets for foo load in background   (chat renders as text until ready)
  3. ChatTransport: part(old) → join(foo)    (DELETE + POST subscription, §5.2)

open_channel("foo")              ← explicit: watch button, double-click, Ctrl O, right-click
  BrowserLauncher: open twitch.tv/foo
```

**Selecting a channel does not open the browser; watching is a separate,
explicit action.** A channel in the list is chat; opening the stream is a
deliberate act. The alternative — a browser launch on every selection — makes
arrow-keying down the follow list spray tabs, and makes the list unusable as a
chat switcher.

**One opt-in exception, and its limits.** With the synced window (§7.2) on,
`browser.follow_chat` lets a selection re-point the window so what you are
watching is what you are reading. It is off by default and it only ever
*switches a window that is already open*: it never starts the watch server and
never launches a browser, so arrow-keying down the list still costs nothing.

Lurkn is the source of truth for what it opened; it never queries the browser.

### 8.4 Highlights, ignores, filters and commands

Four settings pages. They split across the bridge by what each one is:

| Rule | Where | Why there |
| --- | --- | --- |
| Highlights (phrases, chatters, badges, never-highlight) | Rust, `lurkn-core::rules` | Decides the notification as well as the marked row; one place keeps the two in agreement. |
| Ignores (chatters, hide/replace phrases) | Rust, same module | A dropped message costs no IPC and cannot notify. Replacements run before emote resolution, so a replaced word that is an emote name renders as the emote. |
| Filters | Frontend, `$lib/chat/filter` | A filter is a view: switching one off must bring back what it hid, and only the side holding the log can. |
| Commands | Frontend, `$lib/chat/custom` | Expanded in the composer; the result goes through the same path as typed text, so it may be `/w`, a moderation command, or chat. |

`ChatRules` is compiled once per settings change and held in `AppState`, not
per message. Patterns use the `regex` crate, whose matching is linear in the
input: the pattern is the reader's, but the text is whatever a stranger sends,
so a backtracking engine would let one message hang the pump. `check_pattern`
asks the same crate whether a pattern compiles, because JavaScript's dialect
accepts different things. A broken pattern is skipped rather than fatal, and a
broken filter is skipped rather than hiding the whole chat; both are reported
in settings beside the rule.

The reader's own messages are exempt from both ignores and highlights. The old
`chat.highlightKeywords` list migrates into highlight phrases on load, keeping
its whole-word, any-case behaviour and its notification.

### 8.5 Theme files

A folder of theme files beside the built-in themes, picked from one dropdown,
in TOML because that is what the rest of a dotfiles setup is written in.

| Piece | Where | Note |
| --- | --- | --- |
| Config folder | `src-tauri/src/theme.rs` | `LURKN_CONFIG_DIR`, else `$XDG_CONFIG_HOME/lurkn` on every OS, else `%APPDATA%\lurkn` (Windows) or `~/.config/lurkn` (Linux, macOS). macOS skips `~/Library/Application Support` on purpose: a dotfiles repo already points at `~/.config`. |
| Theme files | `<config>/themes/<id>.toml` | The id is the file stem; `settings.customTheme` stores it, empty for the built-in `theme` preference. |
| Parsing | `lurkn-core::theme` | Pure. Every key is optional and falls back to the built-in palette its `variant` names. Colours must be hex (`#` optional, for pywal/wallust/matugen templates); fonts go through `sanitize_font_name`; radius is clamped. A bad value or unknown key is skipped with a warning, not fatal. |
| Live reload | `theme::run` | Polls the chosen file's mtime and size every 500 ms, woken early by `settings_set`. Polling, not inotify: it follows the symlinks stow, chezmoi and home-manager install. A file that stops parsing keeps its last good palette and shows the error under the picker. |
| Applying | `settings.applyTheme` | Sets `data-theme` to the file's variant, then the file's tokens as inline custom properties on `<html>`, over `app.css`. Only `--*` names are ever set. |

The token names in `app.css` are therefore the theme format:
`lurkn_core::theme::COLORS` maps each file key to one. Renaming a token silently
breaks every theme that sets it. Components use tokens, never literal colours,
for anything a theme should reach.

Not themeable: shadows and modal backdrops (black at an alpha, which reads on
either variant), and window transparency, which needs `transparent: true` at
window creation and every surface painted.

### 8.6 More windows: settings and activity

Settings open over the chat, or — with `window.settings_in_window`, or from the
arrow beside the close button — in a window of their own, for a Lurkn tiled too
narrow to show them.

- *One bundle, several windows.* `open_settings_window` (`commands/window.rs`) builds a webview labelled `settings` on `index.html?view=settings[&section=…]`, and `main.ts` mounts `SettingsWindow.svelte` there instead of `App`. It starts only the settings and auth stores; chat, follows and whispers stay the main window's.
- *Kept in step by the events that already exist.* Both windows write through `settings_set` and both hear `SettingsChanged` and `ThemeChanged`, so a change in either shows in both. Asked for again while open, the window comes forward and `SettingsSection` turns it to the group (the chat's filter menu asks for Filters).
- *Login stays in the main window.* Granting moderation or logging out from the settings window brings the main one forward (`show_main_window`), since that is where the code or the login screen appears.
- *Out and back.* The arrow beside ✕ moves settings between the two places and keeps the choice: over the chat it opens the window at the same group; in the window it clears the preference, has Rust bring the main window forward with `ShowSettings(group)` (`settings_in_main`), and closes itself.
- *The window's header is its title bar*, like the main window's: undecorated on Linux and Windows, where it drags the window, a double click maximises it, and its ✕ closes it. On macOS the native traffic lights stay, drawn over the left of that header, which leaves them room (`framed()` in `commands/window.rs`, `tauri.macos.conf.json` for the main window).
- *Closing it closes it.* Hide-to-tray applies to the `main` window only; the settings window is destroyed and rebuilt next time.
- *From the tray*, Settings follows the same preference (`commands::window::show_settings`).
- The capability file lists every label, so each window gets exactly the main window's permissions and nothing more.

**The activity window** (`activity.in_window`) works the same way under
`?view=activity`, with one difference: the feed is a view of the chat log, and
the log lives in the main window. A second log in the activity window would
start empty and miss everything before it opened, so the main window works the
feed out as it always has and sends it across (`$lib/ipc/relay`,
window-to-window events, no Rust in between); the activity window only draws
it, and asks for the current list once it is listening. Rust opens and removes
the window to match the settings (`activity_window`), destroying rather than
closing it so that only a close the reader asked for — its ✕ or the window
manager's — counts as hiding the feed (`activity_closed`).

A second launch of Lurkn brings the running one forward instead
(`tauri-plugin-single-instance`): two processes would mean two chat sockets,
two tray icons and two writers on one database.

### 8.7 Right-click

The WebView's own menu (Back, Reload, Inspect element) is suppressed everywhere
except text fields, where it carries paste and spelling (`nativeMenu` in
`ui/shortcuts.ts`). Parts that have something to offer draw their own: a
message (`MessageMenu`), the gaps in the chat log (the chat's menu), and a
channel in the list — *Watch the stream* and *Open the chat*. With the chat
header hidden, that and `Ctrl O` are the ways to watch.

---

## 9. Data models

All live in `lurkn-core`, derive `Serialize`/`Deserialize`/`Type` (specta), and
use newtypes so a `UserId` can never be passed where a `Login` belongs. Abridged:

```rust
pub struct UserId(pub String);      // Twitch numeric id, as string
pub struct Login(pub String);       // lowercase, [a-z0-9_]{3,25}
pub struct MessageId(pub String);

pub struct User {
    pub id: UserId,
    pub login: Login,
    pub display_name: String,       // may differ only in case/CJK
    pub avatar_url: Option<Url>,
}

// Named TwitchChannel in code: plain `Channel` collides with the Tauri Specta
// runtime's own type when exported to TypeScript.
pub struct TwitchChannel {          // a broadcaster, independent of live state
    pub user: User,
    pub title: Option<String>,      // UNTRUSTED — never rendered as markup
    pub category: Option<Category>,
    pub language: Option<String>,
    pub followed_at: Option<OffsetDateTime>,
}

pub struct Stream {                 // exists only while live
    pub channel_id: UserId,
    pub title: String,              // UNTRUSTED
    pub category: Option<Category>,
    pub viewer_count: u32,
    pub started_at: OffsetDateTime,
    pub thumbnail_url_template: Option<String>,   // has {width}x{height}
}

pub struct ChatMessage {
    pub id: MessageId,
    pub channel_id: UserId,
    pub author: ChatAuthor,
    pub tokens: Vec<ChatToken>,     // already parsed in Rust
    pub raw_text: String,           // for copy/search, never rendered as markup
    pub sent_at: OffsetDateTime,
    pub kind: MessageKind,          // Chat | Action (/me) | Highlight | System
    pub state: MessageState,        // Sending | Sent | Deleted | Failed(reason)
    pub reply_to: Option<MessageId>,
    pub event: Option<ChannelEvent>,  // a sub, raid, announcement, or bits on a message
    pub highlighted: bool,          // decided in Rust, so the row and the notification agree
    pub highlight_color: Option<Rgb>,
}

pub enum ChatToken {
    Text { text: String },
    Emote { emote: Box<Emote> },
    Link { url: Url, display: String },   // scheme-validated in Rust
    Mention { id: UserId, login: Login, display: String },
}

pub struct Emote {
    pub id: String,
    pub name: String,
    pub provider: ProviderId,       // Twitch | SevenTv | Bttv | Ffz
    pub scope: EmoteScope,          // Global | Channel(UserId)
    pub url_1x: Url,
    pub url_2x: Option<Url>,
    pub url_3x: Option<Url>,
    pub animated: bool,
    pub aspect: Option<(u16, u16)>, // reserve layout space → no chat reflow jitter
    pub zero_width: bool,           // 7TV/BTTV overlay emotes
}
```

Two fields that look optional and are not:

- `Emote.aspect` — 7TV wide emotes are not square. Without reserved dimensions the chat log reflows on every image load.
- `Emote.zero_width` — 7TV/BTTV overlay emotes stack on the preceding emote. Ignoring the flag renders them as a broken-looking sequence.

`ChatMessage.state` supports optimistic send: the message appears immediately
as `Sending`, then resolves.

### 9.1 AppState

`src-tauri/src/state.rs` holds the single source of truth: the session, the
follow list, the open channel, the chat handle, emote registries, badge maps,
room state, settings and the compiled rules, each behind its own lock.
Followed channels and live streams are separate maps joined at read time:
storing "is live" as a field on a channel would duplicate state that the
poller already owns.

---

## 10. Caching

| Data | Store | Fresh for |
|---|---|---|
| followed channels | SQLite | 15 min (served stale on failure, indefinitely) |
| channel metadata | SQLite | 1 h |
| live streams | memory | 60 s (it *is* the poll) |
| global emotes (per provider) | SQLite | 24 h |
| channel emotes | SQLite | 1 h |
| badges | SQLite | 24 h |
| whispers | SQLite | kept — Twitch serves no whisper history |

Expiry marks data *stale*, not unusable. Stale data is **always preferred over
an empty screen**: the follow list renders from SQLite before the first
network call completes, so cold start is instant, and an unreachable Twitch
shows the cached list with a note that it is stale. Images are cached by the
WebView itself.

The database lives in the platform's local data folder: `~/.local/share/lurkn`
on Linux (honouring `XDG_DATA_HOME`), `~/Library/Application Support/lurkn` on
macOS, `%LOCALAPPDATA%\lurkn` on Windows.

---

## 11. Security

Concrete, non-negotiable rules:

1. No raw HTML: `innerHTML`, `{@html}` and their kin appear nowhere in `src/`. Enforced by an ESLint rule (`svelte/no-at-html-tags`) **and** a CI grep, because a lint rule can be disabled inline.
2. Untrusted string fields are marked as such at the boundary: chat text, display names, stream titles, category names, badge titles.
3. Image URLs are validated against the provider host allowlist (§6.3) in Rust before crossing IPC.
4. Link tokens: scheme allowlist `{http, https}`; opened via the browser launcher, never navigated to in the WebView.
5. Tauri CSP locked down: `default-src 'self'`, `script-src 'self'` (no inline script, no eval), `img-src` limited to the four CDN hosts, `connect-src` limited to the IPC bridge — all network happens in Rust. Inline styles are allowed, for Svelte's style bindings.
6. Tauri capabilities: the WebView may move, minimise, maximise and close its window, write to the clipboard and open URLs — no shell, no file system.
7. Tokens never cross IPC. The frontend learns who is logged in and which features are granted, and nothing more.
8. Log redaction: the log writer scrubs anything shaped like a token before it reaches disk.

The loopback pages carry their own rules: §7.2 and §7.3, summarised for users
in `SECURITY.md`.

---

## 12. Testing

The workspace split exists to make most of this run in milliseconds with no
network and no WebView.

**Unit (pure, the bulk):** EventSub envelope parsing against captured fixtures,
including unknown message types (ignored, not fatal); `channel.chat.message`
and notification payloads with multi-byte and emoji-bearing fragments, replies,
cheers and shared chat; deduplication; 7TV/BTTV/FFZ payload normalisation;
the precedence table from `docs/emote-precedence.md`; URL generation and login
validation; rules, filters, commands and completion; settings sanitising and
compatibility with older files; the loopback servers' access rules.

**Adversarial:** hostile input — `<script>` in messages, names and titles,
`javascript:` and `data:` links, oversized emote names, fragments that disagree
with the message, truncated envelopes, invalid patterns and font names that try
to escape their CSS value. Each asserts no panic and no markup in the output.

**Integration:** the Helix client and the device code flow against `wiremock`
(pagination, 401 → refresh → retry, 429, a pending then granted or declined
login); EventSub against a local fake socket (subscribing after the welcome,
a redelivered message shown once, revocation, channel switches, a rejected
optional subscription); the loopback servers over real sockets.

**Frontend:** `vitest` + Testing Library for stores, rules and components.

**CI** (`.github/workflows/ci.yml`): `cargo fmt --check`, `cargo clippy -D
warnings` and `cargo test` on Linux, Windows and macOS; on Linux also that
`bindings.ts` is committed as generated, `pnpm lint`, `pnpm check`, `pnpm test`,
the raw-HTML grep, and a release build.

Not in CI: anything hitting real Twitch or real 7TV/BTTV/FFZ.
`crates/lurkn-emotes/tests/live.rs` holds those checks, marked `#[ignore]` and
run on demand with `just live-check`, so a provider outage never fails the
build. They assert the response shapes still parse, that every returned URL is
on an allowed CDN, and that overlay emotes are still flagged — the things that
break silently when a provider changes its API.

---

## 13. Known limitations

1. **No PKCE** → DCF only. Revisit when Twitch ships PKCE.
2. **30-day refresh-token ceiling** on public clients → periodic re-login is unavoidable. §3.4.
3. **Channel switching costs an HTTP round-trip** (`DELETE` + `POST` subscription). Budget ~200 ms and switch the UI optimistically. §5.2.
4. Chat rate limits: 20 messages/30 s per channel (higher for moderators). Twitch **silently drops** excess, so they are enforced client-side. §5.4.
5. EventSub: 3 sockets per user token, 300 subscriptions per socket, 10 s to subscribe after `session_welcome`, and notifications may be **redelivered**. §5.1.
6. Helix bucket: 800 points/min per client id, shared across the whole app. The design uses ~5/min. §4.3.
7. Pagination: 100 items per page on follows and followed streams. A 2000-follow account is 20 requests on cold start; only once, then cached.
8. 7TV, BTTV and FFZ are community services with no SLA and no auth. Every call is treated as failure-prone. §6.1.
9. Emote name collisions across providers are common (`OMEGALUL`, `Pog`) → precedence is explicit, not incidental. §6.2.
10. Twitch stream thumbnails are cached upstream and lag several minutes — they are not presented as live.
11. Browser choice and window modes need freedesktop desktop entries, so they exist on Linux only. §7.1.
12. Follows and hype trains are not in the activity feed: neither comes over chat, and both need scopes not requested.

---

## 14. Releases

How a release is cut, packaged and published — and what is still to do before
the first one — is in `docs/release.md`.

---

## 15. Decisions

| Question | Decision | Why |
|---|---|---|
| Chat transport | **EventSub only.** IRC is not implemented. | Twitch's recommendation; pre-fragmented payloads. §5 |
| Does selecting a channel open the browser? | **No** — watching is a separate, explicit action. | The follow list stays a chat switcher. §8.3 |
| ~Monthly re-login | **Accepted.** | No client secret ever ships. §3.4 |
| Who registers the Twitch app | The project; the client id is compiled in. | A client id is public. §3.5 |
| Emote precedence | `7TV > BTTV > FFZ`, channel before global. | Normative in `docs/emote-precedence.md`. §6.2 |
| Cache store | **SQLite** (`rusqlite`, bundled). | One file, survives restart, no daemon. §10 |
| Where the stream plays | **The user's browser**, never inside Lurkn. | Watch time, channel points, subs and Bits keep working as Twitch intends. §7 |

---

## 16. Sources (verified 2026-09-20)

- [Twitch Authentication overview](https://dev.twitch.tv/docs/authentication/)
- [Twitch OAuth scopes](https://dev.twitch.tv/docs/authentication/scopes/)
- [Refreshing access tokens](https://dev.twitch.tv/docs/authentication/refresh-tokens/)
- [Migrating from Twitch IRC](https://dev.twitch.tv/docs/chat/irc-migration/)
- [Twitch IRC concepts](https://dev.twitch.tv/docs/chat/irc/)
- [Deprecation of chat commands through IRC](https://discuss.dev.twitch.com/t/deprecation-of-chat-commands-through-irc/40486)
- [Device Code Flow documentation notes](https://discuss.dev.twitch.com/t/device-code-flow-documentation-fixes/59136)
- [PKCE support request thread](https://discuss.dev.twitch.com/t/still-no-support-for-auth-code-flow-with-pkce/61894)
- [Handling EventSub WebSocket events](https://dev.twitch.tv/docs/eventsub/handling-websocket-events/)
- [EventSub subscription types](https://dev.twitch.tv/docs/eventsub/eventsub-subscription-types/)

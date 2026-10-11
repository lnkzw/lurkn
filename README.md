# Lurkn

A focused desktop client for the Twitch channels you follow: who is live, their
chat with Twitch, 7TV, BetterTTV and FrankerFaceZ emotes, whispers — and the
stream itself in your own browser, where watch time, channel points, subs and
Bits keep working the way Twitch intends. For Linux, Windows and macOS.

> **Built with an AI coding assistant.** Lurkn is written with Claude Code. A
> person decides what it does, how it should behave and what ships; the
> assistant writes most of the code. Every change goes through the same checks —
> formatting, lints, several hundred tests and a release build on all three
> systems — but no one has audited it line by line. It is provided as is,
> without warranty of any kind (see [LICENSE](LICENSE)).
>
> Lurkn is not affiliated with or endorsed by Twitch.

Lurkn is at 0.x: it works and is used every day, and any release may still
change how something behaves. See [CHANGELOG.md](CHANGELOG.md) for what each
release brought.

## Installing

Every release is on the [Releases page](https://github.com/lnkzw/lurkn/releases).

| System | Download | Or with |
| --- | --- | --- |
| Linux | `.AppImage` (any distribution), `.deb` (Debian, Ubuntu), `.rpm` (Fedora, openSUSE) | AUR: `lurkn-bin` |
| Windows 10 and 11 | `…_x64-setup.exe` | `scoop` / `winget` |
| macOS 12 and later | `….dmg` (Apple Silicon and Intel) | `brew install --cask lnkzw/lurkn/lurkn` |

<!-- Package manager lines go live as each is set up; see docs/release.md. -->

### First launch on macOS and Windows

Lurkn is not signed with a paid Apple or Microsoft certificate, so both systems
ask once before they run it.

- **macOS** says it cannot verify the developer. Open *System Settings →
  Privacy & Security*, scroll to the message about Lurkn and choose *Open
  Anyway*. Or, in a terminal: `xattr -dr com.apple.quarantine /Applications/Lurkn.app`.
  Needed once per installed version.
- **Windows** SmartScreen says "Windows protected your PC". Choose *More info →
  Run anyway*. Needed once.

A build you compile yourself (below) is never asked about.

## Privacy

No telemetry, no account with Lurkn. It talks to:

- **Twitch**, for your follows, chat, whispers and login;
- **7TV, BetterTTV and FrankerFaceZ**, for the emotes of the channels you open;
- **recent-messages.robotty.de** — only if you switch on chat history, which
  tells that service which channels you open. Off by default.

Your Twitch session sits in your system keyring. Lurkn uses Twitch's device
login, so your password never reaches it, and it has no client secret. It asks
for reading follows and chat and sending chat; whispers and moderation are
asked for only when you turn them on. More in [SECURITY.md](SECURITY.md).

## Building from source

You need [Rust](https://rustup.rs) — `rustup` installs the pinned version from
`rust-toolchain.toml` by itself — [Node](https://nodejs.org) 24 or newer and
[pnpm](https://pnpm.io) 11 or newer (`corepack enable` provides it), plus:

| System | Also needed |
| --- | --- |
| Arch | `sudo pacman -S --needed webkit2gtk-4.1 gtk3 librsvg libayatana-appindicator base-devel` |
| Debian, Ubuntu | `sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev libxdo-dev libayatana-appindicator3-dev build-essential` |
| Fedora | `sudo dnf install webkit2gtk4.1-devel librsvg2-devel libxdo-devel libayatana-appindicator-gtk3-devel` and the *C Development Tools* group |
| macOS | Xcode's command line tools: `xcode-select --install` |
| Windows | *Desktop development with C++* from the Visual Studio Build Tools; WebView2 ships with Windows 10 and 11 |

Then:

```sh
pnpm install
pnpm tauri dev                 # run it, reloading as you edit
pnpm tauri build               # installers for this system, under target/release/bundle/
pnpm tauri build --bundles app # macOS: just Lurkn.app, no disk image
```

## Development

```sh
just dev            # pnpm tauri dev
just test           # cargo test --workspace + vitest
just lint           # clippy -D warnings, eslint, svelte-check
just ci             # everything CI runs, in CI order
just live-check     # contract checks against the real 7TV/BTTV/FFZ APIs
just build          # production bundle
just release 0.2.0  # set a version everywhere; prints how to tag it
```

`just live-check` is deliberately outside CI: those tests hit community services
with no SLA, so an outage there must never fail your build. Every recipe is a
plain command if you would rather not install [just](https://github.com/casey/just).

How Lurkn is built, and why, is in [docs/architecture.md](docs/architecture.md).
How a release is made is in [docs/release.md](docs/release.md).

### Layout

```text
crates/          Rust workspace — all domain logic, testable without Tauri
  lurkn-core     pure domain types, no IO
  lurkn-auth     device code login, tokens, keyring
  lurkn-twitch   Helix API client
  lurkn-chat     EventSub chat transport
  lurkn-emotes   Twitch / 7TV / BTTV / FFZ providers
  lurkn-browser  launching browsers
  lurkn-store    SQLite cache and settings
  lurkn-watch    the loopback pages: synced window, OBS overlay
src-tauri/       thin Tauri shell: commands, events, windows, tray
src/             Svelte 5 frontend
docs/            architecture, release process, decisions
```

`src/lib/ipc/bindings.ts` is generated by `tauri-specta` on `cargo test` and on
every debug run. Do not edit it.

### Twitch application

Lurkn ships with a registered client id. A client id is a public identifier,
not a secret — the *client secret* is the thing Lurkn never has, and the device
code flow does not need one. To point a build at your own app (registered at
<https://dev.twitch.tv/console/apps> as a **public client** with the **Device
Code Flow** enabled), set:

```sh
export LURKN_CLIENT_ID=...
```

### Logs

```text
Linux    $XDG_STATE_HOME/lurkn/logs/   (default: ~/.local/state/lurkn/logs/)
macOS    ~/Library/Logs/lurkn/
Windows  %LOCALAPPDATA%\lurkn\logs\
```

Rotated daily. Credentials are redacted at the writer, so no layer can leak one.
Raise verbosity with `LURKN_LOG=debug`. Settings → Advanced shows the tail.

## License

[MIT](LICENSE). The licenses of everything Lurkn is built on — the Rust crates,
Svelte, the Tauri packages and two icon sets — are in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md), which ships inside every
download as well.

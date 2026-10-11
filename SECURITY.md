# Security

## Reporting a problem

Please report anything that could put someone's Twitch account, machine or
privacy at risk through GitHub's private reporting — *Security → Report a
vulnerability* on the repository — rather than as an issue, so it can be
fixed before it is public. Include what you did, what happened, and the
Lurkn version.

Lurkn is a spare-time project built with an AI coding assistant (see the
README). There is no security team and no bounty; reports are read and
answered as soon as someone can, and fixed in the next release.

## What Lurkn holds, and where

- **Your Twitch session** — the access and refresh tokens from Twitch's device
  login — in the system keyring (Secret Service or KWallet on Linux, Keychain
  on macOS, Credential Manager on Windows). Only where no keyring exists does it
  fall back to a file readable by your user alone, and Settings says so. Lurkn
  never sees your password and has no client secret.
- **A cache** — follows, emotes, settings, whispers — in a local SQLite file.
- **A log**, with tokens removed before anything is written.

No telemetry. Lurkn talks to Twitch, to the 7TV, BetterTTV and FrankerFaceZ
emote services, to GitHub to look for a newer version (unless that is switched
off), and — only with chat history switched on — to
`recent-messages.robotty.de`.

## What Lurkn serves on your machine

Two optional features run a small web server for your browser or OBS. Both
listen on `127.0.0.1` only, so nothing off your machine can reach them, and
both refuse requests whose `Host` is not a loopback name, so a web page cannot
point its own domain at them (DNS rebinding).

| | Synced window (experimental) | Chat overlay for OBS |
| --- | --- | --- |
| Off by default | yes | yes |
| Port | random, new each start | fixed, chosen in settings |
| Token | 128-bit, new each start, required on every request | none |
| Shows | the channel you are watching and its chat | your chat, which is public on Twitch anyway |
| Accepts | `GET` the page and its event stream; `POST` a chat font step | `GET` the page and its event stream |

The synced window's token is in the URL handed to your browser, so other
programs running as your user can see it — the same as any local tool that
works this way. It grants knowledge of what you are watching and one font
setting, nothing more.

## In the app

The interface never renders chat as markup: every message is built from text
and checked image URLs (CI fails on any raw-HTML API). Links from chat open in
your browser, never inside Lurkn, and only `http` and `https`. The WebView runs
under a strict content security policy with no access to the shell or the file
system.

Updates are installed only when you confirm, and only when the download's
signature verifies against the release key built into Lurkn; a file that does
not verify is discarded before anything is written. The private half of that
key is held as a GitHub Actions secret and used by the release workflow alone.

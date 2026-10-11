# Changelog

What changed in each release, newest first. The release workflow takes a
version's section from here as its release notes, so write it for the people
installing Lurkn, not for the people building it.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [Semantic Versioning](https://semver.org/) — while Lurkn is at
0.x, any release may change how something works.

## [Unreleased]

The first public release.

### Twitch

- Log in with Twitch's device code: no password ever reaches Lurkn, and the
  session is kept in your system keyring.
- Followed channels with live status, viewers, uptime and category, as a
  drawer or a docked rail; open any channel by name with `Ctrl K`.
- Chat with Twitch, 7TV, BetterTTV and FrankerFaceZ emotes and badges, replies,
  mentions, an emote picker and completion for names, emotes and commands.
- Whispers, kept on disk because Twitch serves no history for them.
- Optional chat history on join from recent-messages.robotty.de — off by
  default, because turning it on tells that service which channels you open.
- Moderation in channels you moderate: delete, timeout, ban, room modes,
  announcements.

### Reading

- Highlights, ignores, filters and custom commands.
- Notifications when a channel goes live, for whispers and for highlights.
- An activity column for subs, gifts, raids and bits — beside the chat or in a
  window of its own.
- Themes from TOML files in your config folder, reloaded as you save them.

### Watching

- Streams open in your browser, with app, kiosk and new-window modes on Linux.
- Experimental: one synced browser window that follows the channel you pick,
  with Lurkn's chat beside the player for a TV — `Ctrl +` / `Ctrl −` sizes the
  chat in both windows.
- A chat overlay for OBS as a browser source.

### Everywhere

- A tray icon; closing the window keeps chat and whispers running.
- Settings over the chat or in a window of their own.
- Linux, Windows and macOS builds.

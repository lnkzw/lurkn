-- Lurkn cache, schema version 1. See docs/architecture.md §10.
-- This database holds cache and settings only. Tokens live in the OS keyring (§3.4).

CREATE TABLE IF NOT EXISTS channels (
  user_id       TEXT PRIMARY KEY,
  login         TEXT NOT NULL,
  display_name  TEXT NOT NULL,
  avatar_url    TEXT,
  title         TEXT,          -- untrusted: never rendered as markup
  category_id   TEXT,
  category_name TEXT,          -- untrusted
  fetched_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS follows (
  user_id     TEXT PRIMARY KEY REFERENCES channels(user_id) ON DELETE CASCADE,
  followed_at INTEGER,
  position    INTEGER NOT NULL   -- preserves Helix ordering across restarts
);

CREATE TABLE IF NOT EXISTS emote_sets (
  provider   TEXT NOT NULL,      -- 'seventv' | 'bttv' | 'ffz' | 'twitch'
  scope      TEXT NOT NULL,      -- 'global' | 'channel'
  channel_id TEXT,               -- NULL for global sets
  payload    TEXT NOT NULL,      -- normalised Vec<Emote> as JSON
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY (provider, scope, channel_id)
);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_channels_login ON channels(login);

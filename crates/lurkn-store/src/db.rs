use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lurkn_core::{Category, FollowedChannel, Login, TwitchChannel, User, UserId};
use rusqlite::{Connection, OptionalExtension};
use time::OffsetDateTime;

use crate::schema::{MIGRATIONS, SCHEMA_VERSION};

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("cache database error: {0}")]
    Sqlite(String),
    #[error("cache is unavailable: {0}")]
    Unavailable(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error.to_string())
    }
}

/// The on-disk cache. Everything here is disposable: losing it costs one refetch,
/// never a login. Tokens live in the keyring (`docs/architecture.md` §3.4).
#[derive(Debug)]
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// `cache.sqlite` in the platform's local data folder: `~/.local/share/lurkn`
    /// on Linux (honouring `XDG_DATA_HOME`), `~/Library/Application
    /// Support/lurkn` on macOS, `%LOCALAPPDATA%\lurkn` on Windows.
    ///
    /// Local rather than roaming on Windows: it is a cache, and a roaming
    /// profile would copy it between machines at every logon.
    pub fn default_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|base| base.join("lurkn").join("cache.sqlite"))
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::Unavailable(format!("{}: {e}", parent.display())))?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        // WAL keeps the poller's writes from blocking reads on the UI path.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let version: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for sql in MIGRATIONS.iter().skip(version as usize) {
            conn.execute_batch(sql)?;
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| StoreError::Unavailable("lock poisoned".into()))
    }

    /// Replace the cached follow list wholesale.
    ///
    /// A follow that disappeared upstream must disappear here too, so this is a
    /// replace rather than an upsert — inside one transaction, so a crash midway
    /// leaves the previous list intact.
    pub fn save_follows(&self, follows: &[FollowedChannel]) -> Result<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let now = OffsetDateTime::now_utc().unix_timestamp();

        tx.execute("DELETE FROM follows", [])?;

        for (position, entry) in follows.iter().enumerate() {
            let c = &entry.channel;
            tx.execute(
                "INSERT INTO channels
                   (user_id, login, display_name, avatar_url, title,
                    category_id, category_name, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(user_id) DO UPDATE SET
                   login = excluded.login,
                   display_name = excluded.display_name,
                   avatar_url = excluded.avatar_url,
                   title = excluded.title,
                   category_id = excluded.category_id,
                   category_name = excluded.category_name,
                   fetched_at = excluded.fetched_at",
                rusqlite::params![
                    c.user.id.as_str(),
                    c.user.login.as_str(),
                    c.user.display_name,
                    c.user.avatar_url.as_ref().map(|u| u.to_string()),
                    c.title,
                    c.category.as_ref().map(|g| g.id.clone()),
                    c.category.as_ref().map(|g| g.name.clone()),
                    now,
                ],
            )?;

            tx.execute(
                "INSERT INTO follows (user_id, followed_at, position) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    c.user.id.as_str(),
                    c.followed_at.map(|t| t.unix_timestamp()),
                    position as i64,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// The cached follow list, in the order it was saved.
    ///
    /// Live state is deliberately not persisted: a stale "live" badge is worse
    /// than no badge, so everything reads back offline until the first poll.
    pub fn load_follows(&self) -> Result<Vec<FollowedChannel>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT c.user_id, c.login, c.display_name, c.avatar_url, c.title,
                    c.category_id, c.category_name, f.followed_at
             FROM follows f
             JOIN channels c ON c.user_id = f.user_id
             ORDER BY f.position",
        )?;

        let rows = stmt.query_map([], |row| {
            let login: String = row.get(1)?;
            let avatar: Option<String> = row.get(3)?;
            let category_id: Option<String> = row.get(5)?;
            let category_name: Option<String> = row.get(6)?;
            let followed_at: Option<i64> = row.get(7)?;

            Ok((
                row.get::<_, String>(0)?,
                login,
                row.get::<_, String>(2)?,
                avatar,
                row.get::<_, Option<String>>(4)?,
                category_id,
                category_name,
                followed_at,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, login, display_name, avatar, title, cat_id, cat_name, followed_at) = row?;
            // A row we cannot turn back into a valid domain value is dropped, not
            // guessed at; the next refresh will replace it.
            let Ok(login) = Login::parse(&login) else {
                continue;
            };

            out.push(FollowedChannel {
                channel: TwitchChannel {
                    user: User {
                        id: UserId(id),
                        login,
                        display_name,
                        avatar_url: avatar.and_then(|u| url::Url::parse(&u).ok()),
                    },
                    title,
                    category: cat_id.map(|id| Category {
                        id,
                        name: cat_name.unwrap_or_default(),
                        box_art_url: None,
                    }),
                    language: None,
                    followed_at: followed_at
                        .and_then(|t| OffsetDateTime::from_unix_timestamp(t).ok()),
                },
                stream: None,
            });
        }

        Ok(out)
    }

    /// When the follow list was last written, for TTL decisions.
    pub fn follows_fetched_at(&self) -> Result<Option<OffsetDateTime>> {
        let conn = self.lock()?;
        let ts: Option<i64> = conn
            .query_row(
                "SELECT MIN(c.fetched_at) FROM follows f JOIN channels c ON c.user_id = f.user_id",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(ts.and_then(|t| OffsetDateTime::from_unix_timestamp(t).ok()))
    }

    /// Cache a provider's emote set as JSON.
    ///
    /// `channel_id` is `None` for a global set. Emote metadata is small and
    /// changes rarely, so whole-set replacement beats row-level bookkeeping.
    pub fn save_emote_set(
        &self,
        provider: &str,
        channel_id: Option<&str>,
        emotes: &[lurkn_core::Emote],
    ) -> Result<()> {
        let payload = serde_json::to_string(emotes)
            .map_err(|e| StoreError::Sqlite(format!("could not serialize emotes: {e}")))?;
        let scope = if channel_id.is_some() {
            "channel"
        } else {
            "global"
        };

        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO emote_sets (provider, scope, channel_id, payload, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(provider, scope, channel_id) DO UPDATE SET
               payload = excluded.payload,
               fetched_at = excluded.fetched_at",
            rusqlite::params![
                provider,
                scope,
                channel_id,
                payload,
                OffsetDateTime::now_utc().unix_timestamp(),
            ],
        )?;
        Ok(())
    }

    /// Read a cached set back, with how old it is so the caller can apply a TTL.
    pub fn load_emote_set(
        &self,
        provider: &str,
        channel_id: Option<&str>,
    ) -> Result<Option<(Vec<lurkn_core::Emote>, OffsetDateTime)>> {
        let scope = if channel_id.is_some() {
            "channel"
        } else {
            "global"
        };

        let conn = self.lock()?;
        let row: Option<(String, i64)> = conn
            .query_row(
                "SELECT payload, fetched_at FROM emote_sets
                 WHERE provider = ?1 AND scope = ?2
                   AND channel_id IS ?3",
                rusqlite::params![provider, scope, channel_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((payload, fetched_at)) = row else {
            return Ok(None);
        };

        // A set we cannot parse is a set we refetch, not a startup failure.
        let Ok(emotes) = serde_json::from_str(&payload) else {
            tracing::warn!(provider, "discarding an unreadable cached emote set");
            return Ok(None);
        };
        let Ok(fetched_at) = OffsetDateTime::from_unix_timestamp(fetched_at) else {
            return Ok(None);
        };

        Ok(Some((emotes, fetched_at)))
    }

    /// Persist settings as one JSON row.
    pub fn save_settings(&self, settings: &lurkn_core::Settings) -> Result<()> {
        let payload = serde_json::to_string(settings)
            .map_err(|e| StoreError::Sqlite(format!("could not serialize settings: {e}")))?;

        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('app', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![payload],
        )?;
        Ok(())
    }

    /// Load settings, falling back to defaults.
    ///
    /// Never fails on bad data: an unreadable row means the user gets defaults
    /// rather than an app that will not start.
    pub fn load_settings(&self) -> lurkn_core::Settings {
        let Ok(conn) = self.lock() else {
            return lurkn_core::Settings::default();
        };

        let row: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();

        row.and_then(
            |payload| match serde_json::from_str::<lurkn_core::Settings>(&payload) {
                Ok(settings) => Some(settings.sanitized()),
                Err(error) => {
                    tracing::warn!(%error, "discarding unreadable settings");
                    None
                }
            },
        )
        .unwrap_or_default()
    }

    /// Remember which channel was open, so a restart lands where you left off.
    ///
    /// Stored beside the settings rather than inside them: it is session state,
    /// not something the user configured.
    pub fn save_last_channel(&self, login: Option<&str>) -> Result<()> {
        let conn = self.lock()?;
        match login {
            Some(login) => {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('last_channel', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![login],
                )?;
            }
            None => {
                conn.execute("DELETE FROM settings WHERE key = 'last_channel'", [])?;
            }
        }
        Ok(())
    }

    pub fn load_last_channel(&self) -> Option<String> {
        let conn = self.lock().ok()?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'last_channel'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .filter(|login| !login.is_empty())
    }

    /// Channels opened without following them, most recent first.
    ///
    /// Stored as one JSON value in the settings table rather than a table of its
    /// own: it is a short, bounded list read and written whole, and a table
    /// would add a migration for eight rows.
    pub fn load_recent_channels(&self) -> Vec<lurkn_core::ChannelHit> {
        let Ok(conn) = self.lock() else {
            return Vec::new();
        };

        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'recent_channels'",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();

        // A list that no longer parses is a convenience lost, not an error to
        // raise: start again empty.
        raw.and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    /// Put a channel at the front of the recent list, keeping it short.
    pub fn remember_channel(&self, hit: &lurkn_core::ChannelHit) -> Result<()> {
        // Enough to find last night's channel again; a list longer than this is
        // one you would search instead of scan.
        const KEEP: usize = 8;

        let mut recent = self.load_recent_channels();
        recent.retain(|c| c.id != hit.id);
        recent.insert(0, hit.clone());
        recent.truncate(KEEP);

        let payload = serde_json::to_string(&recent)
            .map_err(|e| StoreError::Sqlite(format!("could not serialize recent channels: {e}")))?;

        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('recent_channels', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![payload],
        )?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let conn = self.lock()?;
        conn.execute_batch("DELETE FROM follows; DELETE FROM channels; DELETE FROM emote_sets;")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lurkn_core::Stream;

    use super::*;

    fn entry(id: &str, login: &str, name: &str, live: bool) -> FollowedChannel {
        FollowedChannel {
            channel: TwitchChannel {
                user: User {
                    id: UserId(id.into()),
                    login: Login::parse(login).unwrap(),
                    display_name: name.into(),
                    avatar_url: url::Url::parse("https://static-cdn.jtvnw.net/a.png").ok(),
                },
                title: Some("a title".into()),
                category: Some(Category {
                    id: "77".into(),
                    name: "Path of Exile".into(),
                    box_art_url: None,
                }),
                language: None,
                followed_at: Some(time::macros::datetime!(2020-01-02 03:04:05 UTC)),
            },
            stream: live.then(|| Stream {
                channel_id: UserId(id.into()),
                title: "live now".into(),
                category: None,
                viewer_count: 10,
                started_at: time::macros::datetime!(2026-09-20 12:00:00 UTC),
                thumbnail_url_template: None,
            }),
        }
    }

    fn whisper(
        id: &str,
        peer: &str,
        outgoing: bool,
        body: &str,
        minute: u8,
    ) -> lurkn_core::Whisper {
        lurkn_core::Whisper {
            id: id.into(),
            peer_id: UserId(peer.into()),
            peer_login: Login::parse("forsen").unwrap(),
            peer_name: "Forsen".into(),
            outgoing,
            body: body.into(),
            sent_at: time::macros::datetime!(2026-09-21 12:00:00 UTC)
                + time::Duration::minutes(minute as i64),
            read: false,
        }
    }

    #[test]
    fn a_whisper_thread_reads_downwards_and_counts_what_is_unread() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_whisper(&whisper("1", "9", false, "first", 0))
            .unwrap();
        store
            .save_whisper(&whisper("2", "9", true, "reply", 1))
            .unwrap();
        store
            .save_whisper(&whisper("3", "9", false, "third", 2))
            .unwrap();

        let thread = store.whisper_thread("9", 50).unwrap();
        assert_eq!(
            thread.iter().map(|w| w.body.as_str()).collect::<Vec<_>>(),
            ["first", "reply", "third"],
            "oldest first, the way a transcript reads"
        );

        // Only received messages count as unread; your own never do.
        assert_eq!(store.unread_whispers().unwrap(), 2);
    }

    /// EventSub redelivers across a socket handover; a repeat is not a new line.
    #[test]
    fn the_same_whisper_arriving_twice_is_stored_once() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_whisper(&whisper("1", "9", false, "hello", 0))
            .unwrap();
        store
            .save_whisper(&whisper("1", "9", false, "hello", 0))
            .unwrap();

        assert_eq!(store.whisper_thread("9", 50).unwrap().len(), 1);
    }

    #[test]
    fn threads_are_listed_by_most_recent_activity() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_whisper(&whisper("1", "old", false, "ages ago", 0))
            .unwrap();
        store
            .save_whisper(&whisper("2", "new", false, "just now", 5))
            .unwrap();

        let threads = store.whisper_threads().unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].peer_id.as_str(), "new");
        assert_eq!(threads[0].last_body, "just now");
        assert_eq!(threads[0].unread, 1);
    }

    #[test]
    fn opening_a_thread_clears_its_unread_count() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_whisper(&whisper("1", "9", false, "hi", 0))
            .unwrap();
        assert_eq!(store.unread_whispers().unwrap(), 1);

        store.mark_whispers_read("9").unwrap();
        assert_eq!(store.unread_whispers().unwrap(), 0);
        assert_eq!(store.whisper_threads().unwrap()[0].unread, 0);
    }

    /// The limit keeps the recent end, not the beginning.
    #[test]
    fn a_long_thread_is_cut_at_the_old_end() {
        let store = Store::open_in_memory().unwrap();
        for i in 0..10u8 {
            store
                .save_whisper(&whisper(&i.to_string(), "9", false, &format!("m{i}"), i))
                .unwrap();
        }

        let thread = store.whisper_thread("9", 3).unwrap();
        assert_eq!(
            thread.iter().map(|w| w.body.as_str()).collect::<Vec<_>>(),
            ["m7", "m8", "m9"]
        );
    }

    fn hit(id: &str, login: &str) -> lurkn_core::ChannelHit {
        lurkn_core::ChannelHit {
            id: UserId(id.into()),
            login: Login::parse(login).unwrap(),
            display_name: login.into(),
            avatar_url: None,
            live: false,
            category: None,
            title: None,
            viewers: None,
        }
    }

    #[test]
    fn recent_channels_are_newest_first_without_repeats() {
        let store = Store::open_in_memory().unwrap();
        store.remember_channel(&hit("1", "alpha")).unwrap();
        store.remember_channel(&hit("2", "bravo")).unwrap();
        // Opening one again moves it to the front instead of listing it twice.
        store.remember_channel(&hit("1", "alpha")).unwrap();

        let recent = store.load_recent_channels();
        assert_eq!(
            recent.iter().map(|c| c.login.as_str()).collect::<Vec<_>>(),
            ["alpha", "bravo"]
        );
    }

    #[test]
    fn a_recent_list_saved_before_viewers_existed_still_loads() {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.lock().unwrap();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('recent_channels', ?1)",
                rusqlite::params![
                    r#"[{"id":"1","login":"alpha","displayName":"Alpha","avatarUrl":null,
                        "live":false,"category":null,"title":null}]"#
                ],
            )
            .unwrap();
        }

        let recent = store.load_recent_channels();
        assert_eq!(
            recent.len(),
            1,
            "an older shape is not a reason to lose the list"
        );
        assert_eq!(recent[0].viewers, None);
    }

    #[test]
    fn the_recent_list_stays_short() {
        let store = Store::open_in_memory().unwrap();
        for i in 0..20 {
            store
                .remember_channel(&hit(&i.to_string(), &format!("chan{i}")))
                .unwrap();
        }
        let recent = store.load_recent_channels();
        assert_eq!(recent.len(), 8);
        assert_eq!(recent[0].login.as_str(), "chan19", "the newest survives");
    }

    #[test]
    fn round_trips_the_follow_list_in_order() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_follows(&[
                entry("1", "alpha", "Alpha", false),
                entry("2", "bravo", "Bravo", false),
            ])
            .unwrap();

        let loaded = store.load_follows().unwrap();
        let order: Vec<&str> = loaded
            .iter()
            .map(|f| f.channel.user.login.as_str())
            .collect();
        assert_eq!(order, ["alpha", "bravo"]);
        assert_eq!(loaded[0].channel.title.as_deref(), Some("a title"));
        assert_eq!(
            loaded[0].channel.category.as_ref().unwrap().name,
            "Path of Exile"
        );
        assert!(loaded[0].channel.user.avatar_url.is_some());
        assert!(loaded[0].channel.followed_at.is_some());
    }

    /// A stale "live" badge is worse than none, so live state is never persisted.
    #[test]
    fn live_state_is_not_cached() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_follows(&[entry("1", "alpha", "Alpha", true)])
            .unwrap();
        assert!(!store.load_follows().unwrap()[0].is_live());
    }

    /// An unfollow upstream must remove the row, not leave it behind.
    #[test]
    fn saving_replaces_the_previous_list() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_follows(&[
                entry("1", "alpha", "Alpha", false),
                entry("2", "bravo", "Bravo", false),
            ])
            .unwrap();
        store
            .save_follows(&[entry("2", "bravo", "Bravo", false)])
            .unwrap();

        let loaded = store.load_follows().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].channel.user.login.as_str(), "bravo");
    }

    #[test]
    fn an_empty_cache_reads_back_empty_rather_than_failing() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.load_follows().unwrap().is_empty());
        assert_eq!(store.follows_fetched_at().unwrap(), None);
    }

    #[test]
    fn records_when_the_list_was_fetched() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_follows(&[entry("1", "alpha", "Alpha", false)])
            .unwrap();

        let at = store.follows_fetched_at().unwrap().expect("a timestamp");
        let age = time::OffsetDateTime::now_utc() - at;
        assert!(
            age < time::Duration::seconds(5),
            "timestamp should be now, got {age}"
        );
    }

    #[test]
    fn clearing_empties_the_cache() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_follows(&[entry("1", "alpha", "Alpha", false)])
            .unwrap();
        store.clear().unwrap();
        assert!(store.load_follows().unwrap().is_empty());
    }

    #[test]
    fn reopening_a_file_keeps_the_cache_and_does_not_remigrate() {
        let dir = std::env::temp_dir().join(format!("lurkn-db-{}", std::process::id()));
        let path = dir.join("cache.sqlite");
        let _ = std::fs::remove_dir_all(&dir);

        {
            let store = Store::open(&path).unwrap();
            store
                .save_follows(&[entry("1", "alpha", "Alpha", false)])
                .unwrap();
        }
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(
                store.load_follows().unwrap().len(),
                1,
                "cache must survive a restart"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Untrusted text must survive the round trip byte for byte — escaping belongs
    /// to the renderer, and mangling it here would mask that.
    #[test]
    fn hostile_titles_round_trip_verbatim() {
        let store = Store::open_in_memory().unwrap();
        let mut e = entry("1", "alpha", "Alpha", false);
        e.channel.title = Some("<script>alert('x')</script> '; DROP TABLE channels;--".into());
        store.save_follows(&[e]).unwrap();

        let loaded = store.load_follows().unwrap();
        assert_eq!(loaded.len(), 1, "the table must still exist");
        assert_eq!(
            loaded[0].channel.title.as_deref(),
            Some("<script>alert('x')</script> '; DROP TABLE channels;--")
        );
    }
}

#[cfg(test)]
mod emote_cache_tests {
    use lurkn_core::{Emote, EmoteScope, ProviderId};

    use super::*;

    fn emote(name: &str) -> Emote {
        Emote {
            id: format!("id-{name}"),
            name: name.to_owned(),
            provider: ProviderId::SevenTv,
            scope: EmoteScope::Channel,
            url_1x: url::Url::parse("https://cdn.7tv.app/emote/1/1x.webp").unwrap(),
            url_2x: None,
            url_3x: None,
            animated: false,
            aspect: Some((32, 32)),
            zero_width: false,
        }
    }

    #[test]
    fn round_trips_a_channel_set() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_emote_set("seventv", Some("11111"), &[emote("Pog")])
            .unwrap();

        let (emotes, fetched_at) = store
            .load_emote_set("seventv", Some("11111"))
            .unwrap()
            .expect("a cached set");
        assert_eq!(emotes.len(), 1);
        assert_eq!(emotes[0].name, "Pog");
        assert_eq!(emotes[0].aspect, Some((32, 32)));
        assert!(OffsetDateTime::now_utc() - fetched_at < time::Duration::seconds(5));
    }

    /// Global and channel sets for one provider must not collide.
    #[test]
    fn global_and_channel_sets_are_stored_separately() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_emote_set("seventv", None, &[emote("GlobalPog")])
            .unwrap();
        store
            .save_emote_set("seventv", Some("11111"), &[emote("ChannelPog")])
            .unwrap();

        let global = store.load_emote_set("seventv", None).unwrap().unwrap().0;
        let channel = store
            .load_emote_set("seventv", Some("11111"))
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(global[0].name, "GlobalPog");
        assert_eq!(channel[0].name, "ChannelPog");
    }

    #[test]
    fn two_channels_keep_separate_sets() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_emote_set("bttv", Some("1"), &[emote("One")])
            .unwrap();
        store
            .save_emote_set("bttv", Some("2"), &[emote("Two")])
            .unwrap();

        assert_eq!(
            store.load_emote_set("bttv", Some("1")).unwrap().unwrap().0[0].name,
            "One"
        );
        assert_eq!(
            store.load_emote_set("bttv", Some("2")).unwrap().unwrap().0[0].name,
            "Two"
        );
    }

    #[test]
    fn saving_again_replaces_the_set() {
        let store = Store::open_in_memory().unwrap();
        store
            .save_emote_set("ffz", Some("1"), &[emote("Old"), emote("Gone")])
            .unwrap();
        store
            .save_emote_set("ffz", Some("1"), &[emote("New")])
            .unwrap();

        let emotes = store.load_emote_set("ffz", Some("1")).unwrap().unwrap().0;
        assert_eq!(emotes.len(), 1);
        assert_eq!(emotes[0].name, "New");
    }

    #[test]
    fn a_set_that_was_never_cached_reads_back_as_none() {
        let store = Store::open_in_memory().unwrap();
        assert!(store
            .load_emote_set("seventv", Some("nope"))
            .unwrap()
            .is_none());
        assert!(store.load_emote_set("seventv", None).unwrap().is_none());
    }

    #[test]
    fn an_unreadable_payload_reads_back_as_none_rather_than_failing() {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.lock().unwrap();
            conn.execute(
                "INSERT INTO emote_sets (provider, scope, channel_id, payload, fetched_at)
                 VALUES ('seventv', 'global', NULL, '{ not json', 0)",
                [],
            )
            .unwrap();
        }
        assert!(store.load_emote_set("seventv", None).unwrap().is_none());
    }
}

#[cfg(test)]
mod settings_tests {
    use lurkn_core::{EmoteSize, Settings, ThemePreference};

    use super::*;

    #[test]
    fn an_empty_store_returns_defaults() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.load_settings(), Settings::default());
    }

    #[test]
    fn round_trips_settings() {
        let store = Store::open_in_memory().unwrap();
        let settings = Settings {
            theme: ThemePreference::Light,
            chat: lurkn_core::ChatSettings {
                font_size: 17,
                show_timestamps: false,
                emote_size: EmoteSize::Large,
                ..Default::default()
            },
            ..Default::default()
        };

        store.save_settings(&settings).unwrap();
        assert_eq!(store.load_settings(), settings);
    }

    #[test]
    fn saving_twice_replaces_rather_than_duplicating() {
        let store = Store::open_in_memory().unwrap();
        let mut settings = Settings {
            theme: ThemePreference::Dark,
            ..Default::default()
        };
        store.save_settings(&settings).unwrap();
        settings.theme = ThemePreference::Light;
        store.save_settings(&settings).unwrap();

        assert_eq!(store.load_settings().theme, ThemePreference::Light);
        let conn = store.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    /// A corrupt row must not stop the app starting.
    #[test]
    fn unreadable_settings_fall_back_to_defaults() {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.lock().unwrap();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('app', '{ not json')",
                [],
            )
            .unwrap();
        }
        assert_eq!(store.load_settings(), Settings::default());
    }

    /// A font size stored out of range is clamped on load, not trusted.
    #[test]
    fn a_stored_font_size_is_sanitized_on_load() {
        let store = Store::open_in_memory().unwrap();
        {
            let conn = store.lock().unwrap();
            conn.execute(
                r#"INSERT INTO settings (key, value) VALUES ('app', '{"chat":{"fontSize":250,"showTimestamps":true,"showBadges":true,"emoteSize":"medium"}}')"#,
                [],
            )
            .unwrap();
        }
        assert_eq!(
            store.load_settings().chat.font_size,
            lurkn_core::settings::MAX_FONT_SIZE
        );
    }
}

#[cfg(test)]
mod last_channel_tests {
    use super::*;

    #[test]
    fn nothing_is_remembered_to_begin_with() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.load_last_channel(), None);
    }

    #[test]
    fn round_trips_the_last_channel() {
        let store = Store::open_in_memory().unwrap();
        store.save_last_channel(Some("forsen")).unwrap();
        assert_eq!(store.load_last_channel().as_deref(), Some("forsen"));

        store.save_last_channel(Some("nymn")).unwrap();
        assert_eq!(store.load_last_channel().as_deref(), Some("nymn"));
    }

    #[test]
    fn clearing_it_forgets_the_channel() {
        let store = Store::open_in_memory().unwrap();
        store.save_last_channel(Some("forsen")).unwrap();
        store.save_last_channel(None).unwrap();
        assert_eq!(store.load_last_channel(), None);
    }

    /// It shares a table with the settings row; neither may clobber the other.
    #[test]
    fn it_does_not_collide_with_settings() {
        let store = Store::open_in_memory().unwrap();
        let settings = lurkn_core::Settings {
            theme: lurkn_core::ThemePreference::Light,
            ..Default::default()
        };
        store.save_settings(&settings).unwrap();
        store.save_last_channel(Some("forsen")).unwrap();

        assert_eq!(
            store.load_settings().theme,
            lurkn_core::ThemePreference::Light
        );
        assert_eq!(store.load_last_channel().as_deref(), Some("forsen"));
    }

    #[test]
    fn an_empty_value_reads_back_as_nothing_remembered() {
        let store = Store::open_in_memory().unwrap();
        store.save_last_channel(Some("")).unwrap();
        assert_eq!(store.load_last_channel(), None);
    }
}

/// Whisper history, which exists nowhere else.
///
/// Twitch has no endpoint for past whispers, so these rows are the only record.
/// See `lurkn_core::whisper`.
impl Store {
    /// Record a whisper. Idempotent: EventSub redelivers, notably across a
    /// socket handover, and a duplicate must not become a second line.
    pub fn save_whisper(&self, whisper: &lurkn_core::Whisper) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO whispers (id, peer_id, peer_login, peer_name, outgoing, body, sent_at, read)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                whisper.id,
                whisper.peer_id.as_str(),
                whisper.peer_login.as_str(),
                whisper.peer_name,
                whisper.outgoing as i64,
                whisper.body,
                rfc3339(whisper.sent_at),
                whisper.read as i64,
            ],
        )?;
        Ok(())
    }

    /// One row per conversation, most recently active first.
    pub fn whisper_threads(&self) -> Result<Vec<lurkn_core::WhisperThread>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT w.peer_id, w.peer_login, w.peer_name, w.body, w.outgoing, w.sent_at,
                    (SELECT COUNT(*) FROM whispers u
                      WHERE u.peer_id = w.peer_id AND u.read = 0 AND u.outgoing = 0)
             FROM whispers w
             JOIN (SELECT peer_id, MAX(sent_at) AS newest FROM whispers GROUP BY peer_id) latest
               ON latest.peer_id = w.peer_id AND latest.newest = w.sent_at
             GROUP BY w.peer_id
             ORDER BY w.sent_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (peer_id, login, name, body, outgoing, at, unread) = row?;
            // A login the database cannot parse is a row written by something
            // other than this code; skipping beats rendering a broken thread.
            let Ok(peer_login) = lurkn_core::Login::parse(&login) else {
                continue;
            };
            let Some(last_at) = parse_rfc3339(&at) else {
                continue;
            };

            out.push(lurkn_core::WhisperThread {
                peer_id: lurkn_core::UserId(peer_id),
                peer_login,
                peer_name: name,
                last_body: body,
                last_outgoing: outgoing != 0,
                last_at,
                unread: unread.max(0) as u32,
            });
        }

        Ok(out)
    }

    /// One conversation, oldest first, the way a transcript reads.
    pub fn whisper_thread(&self, peer_id: &str, limit: u32) -> Result<Vec<lurkn_core::Whisper>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, peer_id, peer_login, peer_name, outgoing, body, sent_at, read
             FROM whispers WHERE peer_id = ?1 ORDER BY sent_at DESC LIMIT ?2",
        )?;

        let rows = stmt.query_map(rusqlite::params![peer_id, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, peer, login, name, outgoing, body, at, read) = row?;
            let Ok(peer_login) = lurkn_core::Login::parse(&login) else {
                continue;
            };
            let Some(sent_at) = parse_rfc3339(&at) else {
                continue;
            };

            out.push(lurkn_core::Whisper {
                id,
                peer_id: lurkn_core::UserId(peer),
                peer_login,
                peer_name: name,
                outgoing: outgoing != 0,
                body,
                sent_at,
                read: read != 0,
            });
        }

        // Fetched newest-first so the LIMIT keeps the recent end, then reversed
        // so the transcript reads downwards.
        out.reverse();
        Ok(out)
    }

    /// Mark a conversation read, which is what opening it means.
    pub fn mark_whispers_read(&self, peer_id: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE whispers SET read = 1 WHERE peer_id = ?1 AND read = 0",
            rusqlite::params![peer_id],
        )?;
        Ok(())
    }

    /// How many received whispers have not been read, for the badge.
    pub fn unread_whispers(&self) -> Result<u32> {
        let conn = self.lock()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM whispers WHERE read = 0 AND outgoing = 0",
            [],
            |r| r.get(0),
        )?;
        Ok(count.max(0) as u32)
    }
}

fn rfc3339(at: time::OffsetDateTime) -> String {
    at.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn parse_rfc3339(text: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()
}

/// Bump on every migration. Checked against `PRAGMA user_version` at open.
pub const SCHEMA_VERSION: u32 = 2;

/// Applied in order; index `n` migrates from version `n` to `n + 1`.
pub const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_whispers.sql"),
];

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn migration_count_matches_schema_version() {
        assert_eq!(MIGRATIONS.len() as u32, SCHEMA_VERSION);
    }

    #[test]
    fn migrations_apply_to_an_empty_database() {
        let conn = Connection::open_in_memory().unwrap();
        for sql in MIGRATIONS {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();

        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for expected in ["channels", "emote_sets", "follows", "settings", "whispers"] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}: {tables:?}"
            );
        }
    }
}

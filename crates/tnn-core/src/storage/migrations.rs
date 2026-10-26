//! SQLite schema migrations.
//!
//! Migrations are append-only. Never edit a released migration: add a new one
//! and keep the `PRAGMA user_version` chain intact.

use rusqlite_migration::{M, Migrations};

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(INITIAL)])
}

const INITIAL: &str = r#"
CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  model_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  reasoning TEXT,
  status TEXT NOT NULL,
  token_count INTEGER,
  created_at INTEGER NOT NULL,
  seq INTEGER NOT NULL
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id, seq);

CREATE TABLE installed_models (
  id TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  installed_at INTEGER NOT NULL,
  last_used_at INTEGER,
  display_name TEXT NOT NULL,
  file_name TEXT NOT NULL,
  source TEXT NOT NULL
);

CREATE TABLE downloads (
  model_id TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  bytes_done INTEGER NOT NULL DEFAULT 0,
  bytes_total INTEGER NOT NULL DEFAULT 0,
  etag TEXT,
  updated_at INTEGER NOT NULL
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrations_are_idempotent_and_complete() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations().to_latest(&mut conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 1);
        migrations().to_latest(&mut conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        for expected in [
            "conversations",
            "downloads",
            "installed_models",
            "messages",
            "settings",
        ] {
            assert!(tables.iter().any(|table| table == expected), "{expected}");
        }
    }
}

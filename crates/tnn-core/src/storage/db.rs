//! SQLite persistence: conversations, messages, models, downloads, settings.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::errors::{AppError, Result};
use crate::storage::migrations::migrations;
use crate::storage::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl MessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "system" => Self::System,
            "assistant" => Self::Assistant,
            _ => Self::User,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MessageStatus {
    Complete,
    Streaming,
    Interrupted,
    Error,
}

impl MessageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Streaming => "streaming",
            Self::Interrupted => "interrupted",
            Self::Error => "error",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "streaming" => Self::Streaming,
            "interrupted" => Self::Interrupted,
            "error" => Self::Error,
            _ => Self::Complete,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    pub reasoning: Option<String>,
    pub status: MessageStatus,
    pub token_count: Option<u32>,
    pub created_at: i64,
    pub seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub model_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub pinned: bool,
    pub message_count: i64,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub model_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub pinned: bool,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ModelSource {
    Catalog,
    Imported,
}

impl ModelSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Imported => "imported",
        }
    }

    fn parse(value: &str) -> Self {
        if value == "imported" {
            Self::Imported
        } else {
            Self::Catalog
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InstalledModel {
    pub id: String,
    pub display_name: String,
    pub file_name: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub installed_at: i64,
    pub last_used_at: Option<i64>,
    pub source: ModelSource,
}

impl InstalledModel {
    pub fn is_catalog(&self) -> bool {
        self.source == ModelSource::Catalog
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DownloadRecord {
    pub model_id: String,
    pub state: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub etag: Option<String>,
    pub updated_at: i64,
}

pub fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Clone for Db {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;
        migrations().to_latest(&mut conn)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.mark_streaming_interrupted()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrations().to_latest(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// A download cannot survive a process restart; surface it as paused so the
    /// UI offers Resume instead of a stuck progress bar.
    pub fn pause_stale_downloads(&self) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE downloads SET state = 'paused', updated_at = ?1
             WHERE state IN ('queued', 'downloading', 'verifying')",
            params![now_millis()],
        )?;
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| AppError::Storage {
            detail: "database lock poisoned".into(),
        })
    }

    /// A crash or forced quit can leave rows in `streaming`. They are partial
    /// answers, so keep the text and mark the message interrupted.
    pub fn mark_streaming_interrupted(&self) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE messages SET status = 'interrupted' WHERE status = 'streaming'",
            [],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- chats

    pub fn create_conversation(&self, model_id: &str) -> Result<Conversation> {
        let id = Uuid::now_v7().to_string();
        let now = now_millis();
        {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO conversations (id, title, model_id, created_at, updated_at, pinned)
                 VALUES (?1, 'New chat', ?2, ?3, ?3, 0)",
                params![id, model_id, now],
            )?;
        }
        self.get_conversation(&id)
    }

    pub fn get_conversation(&self, id: &str) -> Result<Conversation> {
        let conn = self.lock()?;
        let mut conversation = conn
            .query_row(
                "SELECT id, title, model_id, created_at, updated_at, pinned
                 FROM conversations WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Conversation {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        model_id: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        pinned: row.get::<_, i64>(5)? != 0,
                        messages: Vec::new(),
                    })
                },
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => AppError::InvalidInput {
                    detail: format!("unknown conversation {id}"),
                },
                other => AppError::from(other),
            })?;

        let mut statement = conn.prepare(
            "SELECT id, conversation_id, role, content, reasoning, status, token_count, created_at, seq
             FROM messages WHERE conversation_id = ?1 ORDER BY seq ASC",
        )?;
        conversation.messages = statement
            .query_map(params![id], message_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(conversation)
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT c.id, c.title, c.model_id, c.created_at, c.updated_at, c.pinned,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id),
                    (SELECT m.content FROM messages m
                      WHERE m.conversation_id = c.id AND m.role = 'user'
                      ORDER BY m.seq DESC LIMIT 1)
             FROM conversations c ORDER BY c.pinned DESC, c.updated_at DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                let content: Option<String> = row.get(7)?;
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    model_id: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    pinned: row.get::<_, i64>(5)? != 0,
                    message_count: row.get(6)?,
                    preview: content.map(|text| truncate(&text, 120)),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn rename_conversation(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::InvalidInput {
                detail: "conversation title cannot be empty".into(),
            });
        }
        let conn = self.lock()?;
        let changed = conn.execute(
            "UPDATE conversations SET title = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, truncate(title, 200), now_millis()],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput {
                detail: format!("unknown conversation {id}"),
            });
        }
        Ok(())
    }

    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_conversation_model(&self, id: &str, model_id: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE conversations SET model_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, model_id, now_millis()],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn append_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        reasoning: Option<&str>,
        status: MessageStatus,
        token_count: Option<u32>,
    ) -> Result<Message> {
        let id = Uuid::now_v7().to_string();
        let now = now_millis();
        {
            let conn = self.lock()?;
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                params![conversation_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(AppError::InvalidInput {
                    detail: format!("unknown conversation {conversation_id}"),
                });
            }

            conn.execute(
                "INSERT INTO messages
                   (id, conversation_id, role, content, reasoning, status, token_count, created_at, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                   (SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE conversation_id = ?2))",
                params![
                    id,
                    conversation_id,
                    role.as_str(),
                    content,
                    reasoning,
                    status.as_str(),
                    token_count,
                    now
                ],
            )?;
            // Also promotes the placeholder title on the first user message.
            if role == MessageRole::User {
                conn.execute(
                    "UPDATE conversations
                     SET updated_at = ?2,
                         title = CASE WHEN title = 'New chat'
                                      THEN ?3 ELSE title END
                     WHERE id = ?1",
                    params![conversation_id, now, truncate(content, 60)],
                )?;
            } else {
                conn.execute(
                    "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
                    params![conversation_id, now],
                )?;
            }
        }
        self.message(&id)
    }

    pub fn message(&self, id: &str) -> Result<Message> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, conversation_id, role, content, reasoning, status, token_count, created_at, seq
             FROM messages WHERE id = ?1",
            params![id],
            message_from_row,
        )
        .map_err(AppError::from)
    }

    pub fn update_message(
        &self,
        id: &str,
        content: &str,
        reasoning: Option<&str>,
        status: MessageStatus,
        token_count: Option<u32>,
    ) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE messages
             SET content = ?2, reasoning = ?3, status = ?4, token_count = ?5
             WHERE id = ?1",
            params![id, content, reasoning, status.as_str(), token_count],
        )?;
        Ok(())
    }

    pub fn delete_message(&self, id: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_messages_after(&self, conversation_id: &str, seq: i64) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1 AND seq > ?2",
            params![conversation_id, seq],
        )?;
        Ok(())
    }

    pub fn last_user_message(&self, conversation_id: &str) -> Result<Option<Message>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT id, conversation_id, role, content, reasoning, status, token_count, created_at, seq
             FROM messages
             WHERE conversation_id = ?1 AND role = 'user'
             ORDER BY seq DESC LIMIT 1",
        )?;
        let mut rows = statement.query_map(params![conversation_id], message_from_row)?;
        rows.next().transpose().map_err(AppError::from)
    }

    // --------------------------------------------------------------- models

    pub fn upsert_installed(&self, model: &InstalledModel) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO installed_models
               (id, path, sha256, size_bytes, installed_at, last_used_at, display_name, file_name, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               path = excluded.path,
               sha256 = excluded.sha256,
               size_bytes = excluded.size_bytes,
               display_name = excluded.display_name,
               file_name = excluded.file_name,
               source = excluded.source",
            params![
                model.id,
                model.path,
                model.sha256,
                model.size_bytes as i64,
                model.installed_at,
                model.last_used_at,
                model.display_name,
                model.file_name,
                model.source.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn installed_models(&self) -> Result<Vec<InstalledModel>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT id, display_name, file_name, path, sha256, size_bytes, installed_at,
                    last_used_at, source
             FROM installed_models ORDER BY installed_at DESC",
        )?;
        let rows = statement
            .query_map([], installed_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn installed_model(&self, id: &str) -> Result<Option<InstalledModel>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT id, display_name, file_name, path, sha256, size_bytes, installed_at,
                    last_used_at, source
             FROM installed_models WHERE id = ?1",
        )?;
        let mut rows = statement.query_map(params![id], installed_from_row)?;
        rows.next().transpose().map_err(AppError::from)
    }

    pub fn delete_installed(&self, id: &str) -> Result<Option<InstalledModel>> {
        let model = self.installed_model(id)?;
        if model.is_some() {
            let conn = self.lock()?;
            conn.execute("DELETE FROM installed_models WHERE id = ?1", params![id])?;
        }
        Ok(model)
    }

    pub fn touch_model(&self, id: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE installed_models SET last_used_at = ?2 WHERE id = ?1",
            params![id, now_millis()],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------ downloads

    pub fn save_download(&self, record: &DownloadRecord) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO downloads
               (model_id, state, bytes_done, bytes_total, etag, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(model_id) DO UPDATE SET
               state = excluded.state,
               bytes_done = excluded.bytes_done,
               bytes_total = excluded.bytes_total,
               etag = excluded.etag,
               updated_at = excluded.updated_at",
            params![
                record.model_id,
                record.state,
                record.bytes_done as i64,
                record.bytes_total as i64,
                record.etag,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn download(&self, model_id: &str) -> Result<Option<DownloadRecord>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT model_id, state, bytes_done, bytes_total, etag, updated_at
             FROM downloads WHERE model_id = ?1",
        )?;
        let mut rows = statement.query_map(params![model_id], download_from_row)?;
        rows.next().transpose().map_err(AppError::from)
    }

    pub fn downloads(&self) -> Result<Vec<DownloadRecord>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT model_id, state, bytes_done, bytes_total, etag, updated_at
             FROM downloads ORDER BY updated_at DESC",
        )?;
        let rows = statement
            .query_map([], download_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_download(&self, model_id: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "DELETE FROM downloads WHERE model_id = ?1",
            params![model_id],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------- settings

    pub fn load_settings(&self) -> Result<Settings> {
        let conn = self.lock()?;
        let mut statement = conn.prepare("SELECT value FROM settings WHERE key = 'settings'")?;
        let mut rows = statement.query(params![])?;
        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            let settings: Settings = serde_json::from_str(&value)?;
            settings.validate()?;
            Ok(settings)
        } else {
            Ok(Settings::default())
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        settings.validate()?;
        let value = serde_json::to_string(settings)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![value],
        )?;
        Ok(())
    }
}

fn message_from_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    let role: String = row.get(2)?;
    let status: String = row.get(5)?;
    let token_count: Option<i64> = row.get(6)?;
    Ok(Message {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role: MessageRole::parse(&role),
        content: row.get(3)?,
        reasoning: row.get(4)?,
        status: MessageStatus::parse(&status),
        token_count: token_count.map(|value| value as u32),
        created_at: row.get(7)?,
        seq: row.get(8)?,
    })
}

fn installed_from_row(row: &Row<'_>) -> rusqlite::Result<InstalledModel> {
    let source: String = row.get(8)?;
    Ok(InstalledModel {
        id: row.get(0)?,
        display_name: row.get(1)?,
        file_name: row.get(2)?,
        path: row.get(3)?,
        sha256: row.get(4)?,
        size_bytes: row.get::<_, i64>(5)? as u64,
        installed_at: row.get(6)?,
        last_used_at: row.get(7)?,
        source: ModelSource::parse(&source),
    })
}

fn download_from_row(row: &Row<'_>) -> rusqlite::Result<DownloadRecord> {
    Ok(DownloadRecord {
        model_id: row.get(0)?,
        state: row.get(1)?,
        bytes_done: row.get::<_, i64>(2)? as u64,
        bytes_total: row.get::<_, i64>(3)? as u64,
        etag: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut result: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_lifecycle() {
        let db = Db::open_in_memory().unwrap();
        let conversation = db.create_conversation("bonsai2-2b-pq2").unwrap();
        assert_eq!(conversation.title, "New chat");

        let user = db
            .append_message(
                &conversation.id,
                MessageRole::User,
                "Explain local AI",
                None,
                MessageStatus::Complete,
                Some(5),
            )
            .unwrap();
        assert_eq!(user.seq, 1);

        let assistant = db
            .append_message(
                &conversation.id,
                MessageRole::Assistant,
                "It runs here.",
                None,
                MessageStatus::Streaming,
                None,
            )
            .unwrap();
        assert_eq!(assistant.seq, 2);

        let loaded = db.get_conversation(&conversation.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.title, "Explain local AI");

        db.mark_streaming_interrupted().unwrap();
        let loaded = db.get_conversation(&conversation.id).unwrap();
        assert_eq!(loaded.messages[1].status, MessageStatus::Interrupted);

        db.rename_conversation(&conversation.id, "Renamed").unwrap();
        assert_eq!(
            db.list_conversations().unwrap()[0].title,
            "Renamed".to_string()
        );

        db.delete_conversation(&conversation.id).unwrap();
        assert!(db.get_conversation(&conversation.id).is_err());
    }

    #[test]
    fn cascades_messages_on_conversation_delete() {
        let db = Db::open_in_memory().unwrap();
        let conversation = db.create_conversation("m").unwrap();
        db.append_message(
            &conversation.id,
            MessageRole::User,
            "hi",
            None,
            MessageStatus::Complete,
            None,
        )
        .unwrap();
        db.delete_conversation(&conversation.id).unwrap();
        assert!(db.last_user_message(&conversation.id).unwrap().is_none());
    }

    #[test]
    fn installed_models_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let model = InstalledModel {
            id: "bonsai2-2b-pq2".into(),
            display_name: "Bonsai 2 · 2B".into(),
            file_name: "m.gguf".into(),
            path: "/tmp/m.gguf".into(),
            sha256: "a".repeat(64),
            size_bytes: 42,
            installed_at: now_millis(),
            last_used_at: None,
            source: ModelSource::Catalog,
        };
        db.upsert_installed(&model).unwrap();
        assert_eq!(db.installed_models().unwrap().len(), 1);
        db.touch_model(&model.id).unwrap();
        assert!(
            db.installed_model(&model.id)
                .unwrap()
                .unwrap()
                .last_used_at
                .is_some()
        );
        assert!(db.delete_installed(&model.id).unwrap().is_some());
        assert!(db.installed_models().unwrap().is_empty());
    }

    #[test]
    fn settings_round_trip_and_defaults() {
        let db = Db::open_in_memory().unwrap();
        assert!(!db.load_settings().unwrap().onboarding_complete);
        let mut settings = db.load_settings().unwrap();
        settings.onboarding_complete = true;
        settings.threads = Some(2);
        db.save_settings(&settings).unwrap();
        let loaded = db.load_settings().unwrap();
        assert!(loaded.onboarding_complete);
        assert_eq!(loaded.threads, Some(2));
    }

    #[test]
    fn downloads_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let record = DownloadRecord {
            model_id: "m".into(),
            state: "downloading".into(),
            bytes_done: 10,
            bytes_total: 100,
            etag: None,
            updated_at: now_millis(),
        };
        db.save_download(&record).unwrap();
        assert_eq!(db.download("m").unwrap().unwrap().bytes_done, 10);
        db.delete_download("m").unwrap();
        assert!(db.download("m").unwrap().is_none());
    }

    #[test]
    fn deletes_tail_messages() {
        let db = Db::open_in_memory().unwrap();
        let conversation = db.create_conversation("m").unwrap();
        for text in ["one", "two", "three"] {
            db.append_message(
                &conversation.id,
                MessageRole::User,
                text,
                None,
                MessageStatus::Complete,
                None,
            )
            .unwrap();
        }
        db.delete_messages_after(&conversation.id, 1).unwrap();
        assert_eq!(
            db.get_conversation(&conversation.id)
                .unwrap()
                .messages
                .len(),
            1
        );
    }
}

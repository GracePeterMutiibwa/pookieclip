use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

// Unpinned history expires after 30 days and is capped at 1000 items
const MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const MAX_ITEMS: i64 = 1000;

const COLUMNS: &str = "id, mime_type, content, source_app, pinned, copied_at";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i64,
    pub mime_type: String,
    pub content: String,
    pub source_app: String,
    pub pinned: bool,
    pub copied_at: i64,
}

pub struct Store {
    connection: Mutex<Connection>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn clip_from_row(row: &Row) -> rusqlite::Result<Clip> {
    Ok(Clip {
        id: row.get(0)?,
        mime_type: row.get(1)?,
        content: row.get(2)?,
        source_app: row.get(3)?,
        pinned: row.get(4)?,
        copied_at: row.get(5)?,
    })
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Store> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS clips (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 mime_type TEXT NOT NULL,
                 content TEXT NOT NULL UNIQUE,
                 source_app TEXT NOT NULL,
                 pinned INTEGER NOT NULL DEFAULT 0,
                 copied_at INTEGER NOT NULL
             );",
        )?;
        let store = Store {
            connection: Mutex::new(connection),
        };
        store.prune()?;
        Ok(store)
    }

    // Copying something already saved moves it to the top instead of duplicating it,
    // except pinned items, which keep their place
    pub fn add(&self, mime_type: &str, content: &str, source_app: &str) -> rusqlite::Result<Clip> {
        let clip = self.connection.lock().unwrap().query_row(
            &format!(
                "INSERT INTO clips (mime_type, content, source_app, copied_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(content) DO UPDATE SET
                     mime_type = excluded.mime_type,
                     source_app = excluded.source_app,
                     copied_at = CASE WHEN pinned = 1 THEN copied_at ELSE excluded.copied_at END
                 RETURNING {COLUMNS}"
            ),
            params![mime_type, content, source_app, now_ms()],
            clip_from_row,
        )?;
        self.prune()?;
        Ok(clip)
    }

    pub fn list(&self) -> rusqlite::Result<Vec<Clip>> {
        let connection = self.connection.lock().unwrap();
        let mut statement =
            connection.prepare(&format!("SELECT {COLUMNS} FROM clips ORDER BY copied_at DESC"))?;
        let clips = statement.query_map([], clip_from_row)?.collect();
        clips
    }

    // Finds the entry holding exactly this content, if there is one
    pub fn find_id(&self, content: &str) -> rusqlite::Result<Option<i64>> {
        self.connection
            .lock()
            .unwrap()
            .query_row("SELECT id FROM clips WHERE content = ?1", params![content], |row| row.get(0))
            .optional()
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("UPDATE clips SET pinned = ?1 WHERE id = ?2", params![pinned, id])?;
        Ok(())
    }

    pub fn remove(&self, id: i64) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM clips WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_unpinned(&self) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM clips WHERE pinned = 0", [])?;
        Ok(())
    }

    // Puts back removed clips for undo, skipping any that were copied again meanwhile
    pub fn restore(&self, clips: &[Clip]) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        for clip in clips {
            transaction.execute(
                &format!("INSERT OR IGNORE INTO clips ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
                params![clip.id, clip.mime_type, clip.content, clip.source_app, clip.pinned, clip.copied_at],
            )?;
        }
        transaction.commit()
    }

    fn prune(&self) -> rusqlite::Result<()> {
        let connection = self.connection.lock().unwrap();
        connection.execute(
            "DELETE FROM clips WHERE pinned = 0 AND copied_at < ?1",
            params![now_ms() - MAX_AGE_MS],
        )?;
        connection.execute(
            "DELETE FROM clips WHERE id IN (
                 SELECT id FROM clips WHERE pinned = 0
                 ORDER BY copied_at DESC LIMIT -1 OFFSET ?1
             )",
            params![MAX_ITEMS],
        )?;
        Ok(())
    }
}
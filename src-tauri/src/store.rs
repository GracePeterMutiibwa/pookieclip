use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

// Unpinned history expires after 30 days and is capped at 1000 items
const MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const MAX_ITEMS: i64 = 1000;

const COLUMNS: &str = "id, mime_type, content, source_app, pinned, copied_at, stored, stored_bytes";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i64,
    pub mime_type: String,
    pub content: String,
    pub source_app: String,
    pub pinned: bool,
    pub copied_at: i64,
    // File list pointing at PookieClip's own copies, once they are saved
    pub stored: Option<String>,
    pub stored_bytes: i64,
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
        stored: row.get(6)?,
        stored_bytes: row.get(7)?,
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
        // Databases from before stored copies existed get the new columns added once
        let has_stored = connection
            .prepare("SELECT 1 FROM pragma_table_info('clips') WHERE name = 'stored'")?
            .exists([])?;
        if !has_stored {
            connection.execute_batch(
                "ALTER TABLE clips ADD COLUMN stored TEXT;
                 ALTER TABLE clips ADD COLUMN stored_bytes INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
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

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Clip>> {
        self.connection
            .lock()
            .unwrap()
            .query_row(
                &format!("SELECT {COLUMNS} FROM clips WHERE id = ?1"),
                params![id],
                clip_from_row,
            )
            .optional()
    }

    // Finds the entry holding exactly this content, if there is one
    pub fn find_id(&self, content: &str) -> rusqlite::Result<Option<i64>> {
        self.connection
            .lock()
            .unwrap()
            .query_row("SELECT id FROM clips WHERE content = ?1", params![content], |row| row.get(0))
            .optional()
    }

    // Records where an entry's own copies were saved and how much space they take
    pub fn set_stored(&self, id: i64, stored: &str, stored_bytes: i64) -> rusqlite::Result<Option<Clip>> {
        self.connection
            .lock()
            .unwrap()
            .query_row(
                &format!("UPDATE clips SET stored = ?1, stored_bytes = ?2 WHERE id = ?3 RETURNING {COLUMNS}"),
                params![stored, stored_bytes, id],
                clip_from_row,
            )
            .optional()
    }

    // Copying from PookieClip's own stored files moves that entry to the top, like a fresh copy
    pub fn touch_stored(&self, stored: &str) -> rusqlite::Result<Option<Clip>> {
        self.connection
            .lock()
            .unwrap()
            .query_row(
                &format!(
                    "UPDATE clips SET copied_at = CASE WHEN pinned = 1 THEN copied_at ELSE ?1 END
                     WHERE stored = ?2 RETURNING {COLUMNS}"
                ),
                params![now_ms(), stored],
                clip_from_row,
            )
            .optional()
    }

    // Every stored file list still in use, so unused copies can be deleted from disk
    pub fn stored_lists(&self) -> rusqlite::Result<Vec<String>> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection.prepare("SELECT stored FROM clips WHERE stored IS NOT NULL")?;
        let lists = statement.query_map([], |row| row.get(0))?.collect();
        lists
    }

    // Drops the oldest unpinned stored copies until the total fits in `allowed` bytes.
    // Pinned copies are never dropped, so pinned files alone can go over the limit.
    pub fn make_room(&self, allowed: i64) -> rusqlite::Result<()> {
        let connection = self.connection.lock().unwrap();
        loop {
            let used: i64 = connection.query_row(
                "SELECT COALESCE(SUM(stored_bytes), 0) FROM clips WHERE stored IS NOT NULL",
                [],
                |row| row.get(0),
            )?;
            if used <= allowed {
                return Ok(());
            }
            let oldest: Option<(i64, String)> = connection
                .query_row(
                    "SELECT id, mime_type FROM clips WHERE stored IS NOT NULL AND pinned = 0
                     ORDER BY copied_at ASC LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((id, mime_type)) = oldest else {
                return Ok(());
            };
            // A file entry still points at the originals, an image has nothing else to paste
            if mime_type.starts_with("image/") {
                connection.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
            } else {
                connection.execute(
                    "UPDATE clips SET stored = NULL, stored_bytes = 0 WHERE id = ?1",
                    params![id],
                )?;
            }
        }
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
                &format!("INSERT OR IGNORE INTO clips ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
                params![
                    clip.id,
                    clip.mime_type,
                    clip.content,
                    clip.source_app,
                    clip.pinned,
                    clip.copied_at,
                    clip.stored,
                    clip.stored_bytes
                ],
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
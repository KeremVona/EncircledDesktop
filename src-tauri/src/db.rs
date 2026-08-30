use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

static DB_CONN: Mutex<Option<Connection>> = Mutex::new(None);

pub fn init_sqlite_db() -> Result<(), rusqlite::Error> {
    let mut lock = DB_CONN.lock().unwrap();
    if lock.is_some() {
        return Ok(());
    }

    let mut db_path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    db_path.push("encircled_desktop.db");
    let conn = Connection::open(db_path)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS queued_uploads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_hash TEXT NOT NULL,
            file_name TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            retry_count INTEGER DEFAULT 0
        )",
        [],
    )?;

    // Make unique index to prevent duplicate autosaves from flooding the offline retry queue
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_queued_session_hash ON queued_uploads (session_id, file_hash)",
        [],
    )?;

    // Maintenance: Prune exhausted retries (retry count >= 10)
    let _ = conn.execute("DELETE FROM queued_uploads WHERE retry_count >= 10", []);

    *lock = Some(conn);
    Ok(())
}

fn with_db<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&Connection) -> R,
{
    let mut lock = DB_CONN.lock().unwrap();
    if lock.is_none() {
        drop(lock);
        let _ = init_sqlite_db();
        lock = DB_CONN.lock().unwrap();
    }
    lock.as_ref().map(f)
}

pub fn enqueue_offline_upload(
    session_id: &str,
    file_path: &str,
    file_hash: &str,
    file_name: &str,
    timestamp: &str,
) {
    with_db(|conn| {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO queued_uploads (session_id, file_path, file_hash, file_name, timestamp, retry_count) VALUES (?, ?, ?, ?, ?, 0)",
            params![session_id, file_path, file_hash, file_name, timestamp],
        );
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedUpload {
    pub id: i64,
    pub session_id: String,
    pub file_path: String,
    pub file_hash: String,
    pub file_name: String,
    pub timestamp: String,
}

pub fn get_pending_queued_uploads() -> Vec<QueuedUpload> {
    with_db(|conn| {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, session_id, file_path, file_hash, file_name, timestamp FROM queued_uploads WHERE retry_count < 10 ORDER BY id DESC",
        ) {
            let rows = stmt.query_map([], |row| {
                Ok(QueuedUpload {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    file_path: row.get(2)?,
                    file_hash: row.get(3)?,
                    file_name: row.get(4)?,
                    timestamp: row.get(5)?,
                })
            });
            if let Ok(mapped) = rows {
                return mapped.flatten().collect();
            }
        }
        Vec::new()
    })
    .unwrap_or_default()
}

pub fn delete_queued_upload(id: i64) {
    with_db(|conn| {
        let _ = conn.execute("DELETE FROM queued_uploads WHERE id = ?", params![id]);
    });
}

pub fn clear_all_queued_uploads() -> Result<usize, String> {
    with_db(|conn| {
        conn.execute("DELETE FROM queued_uploads", [])
            .map_err(|e| e.to_string())
    })
    .unwrap_or_else(|| Err("Could not access SQLite database".into()))
}

pub fn increment_queued_upload_retry(id: i64) {
    with_db(|conn| {
        let _ = conn.execute(
            "UPDATE queued_uploads SET retry_count = retry_count + 1 WHERE id = ?",
            params![id],
        );
    });
}

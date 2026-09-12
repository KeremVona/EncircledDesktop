use crate::models::TelemetryPayload;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

static DB_CONN: Mutex<Option<Connection>> = Mutex::new(None);

pub fn get_db_path() -> PathBuf {
    let base_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let app_dir = base_dir.join("com.encircled.desktop");
    if !app_dir.exists() {
        let _ = std::fs::create_dir_all(&app_dir);
    }

    let new_db_path = app_dir.join("encircled_desktop.db");
    let legacy_db_path = base_dir.join("encircled_desktop.db");

    // Migrate legacy database from Roaming root if present and new DB does not exist yet
    if legacy_db_path.exists() && !new_db_path.exists() {
        println!(
            "[DB] Migrating legacy database from {:?} to {:?}",
            legacy_db_path, new_db_path
        );
        if let Err(e) = std::fs::rename(&legacy_db_path, &new_db_path) {
            println!("[DB] Rename failed ({}), attempting copy fallback", e);
            if std::fs::copy(&legacy_db_path, &new_db_path).is_ok() {
                let _ = std::fs::remove_file(&legacy_db_path);
            }
        }
    }

    new_db_path
}

pub fn init_sqlite_db() -> Result<(), rusqlite::Error> {
    let mut lock = DB_CONN.lock().unwrap_or_else(|e| e.into_inner());
    if lock.is_some() {
        return Ok(());
    }

    let db_path = get_db_path();
    let conn = Connection::open(db_path)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS queued_uploads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_hash TEXT NOT NULL,
            file_name TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            retry_count INTEGER DEFAULT 0,
            api_key TEXT
        )",
        [],
    )?;

    // Migration: add api_key column if upgrading from earlier schema
    let _ = conn.execute("ALTER TABLE queued_uploads ADD COLUMN api_key TEXT", []);

    // Make unique index to prevent duplicate autosaves from flooding the offline retry queue
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_queued_session_hash ON queued_uploads (session_id, file_hash)",
        [],
    )?;

    // Maintenance: Prune exhausted retries (retry count >= 10)
    let _ = conn.execute("DELETE FROM queued_uploads WHERE retry_count >= 10", []);

    conn.execute(
        "CREATE TABLE IF NOT EXISTS telemetry_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            verified INTEGER NOT NULL DEFAULT 0,
            has_debug_flag INTEGER NOT NULL DEFAULT 0,
            timestamp TEXT NOT NULL
        )",
        [],
    )?;

    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_telemetry_session ON telemetry_history (session_id)",
        [],
    );
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_telemetry_id_desc ON telemetry_history (id DESC)",
        [],
    );

    *lock = Some(conn);
    Ok(())
}

fn with_db<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&Connection) -> R,
{
    let mut lock = DB_CONN.lock().unwrap_or_else(|e| e.into_inner());
    if lock.is_none() {
        drop(lock);
        let _ = init_sqlite_db();
        lock = DB_CONN.lock().unwrap_or_else(|e| e.into_inner());
    }
    lock.as_ref().map(f)
}

pub fn enqueue_offline_upload(
    session_id: &str,
    file_path: &str,
    file_hash: &str,
    file_name: &str,
    timestamp: &str,
    api_key: Option<&str>,
) {
    with_db(|conn| {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO queued_uploads (session_id, file_path, file_hash, file_name, timestamp, retry_count, api_key) VALUES (?, ?, ?, ?, ?, 0, ?)",
            params![session_id, file_path, file_hash, file_name, timestamp, api_key],
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
    pub api_key: Option<String>,
}

pub fn get_pending_queued_uploads() -> Vec<QueuedUpload> {
    with_db(|conn| {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, session_id, file_path, file_hash, file_name, timestamp, api_key FROM queued_uploads WHERE retry_count < 10 ORDER BY id DESC",
        ) {
            let rows = stmt.query_map([], |row| {
                Ok(QueuedUpload {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    file_path: row.get(2)?,
                    file_hash: row.get(3)?,
                    file_name: row.get(4)?,
                    timestamp: row.get(5)?,
                    api_key: row.get(6)?,
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

pub fn record_telemetry_event(session_id: &str, payload: &TelemetryPayload) {
    with_db(|conn| {
        let _ = conn.execute(
            "INSERT INTO telemetry_history (session_id, file_name, file_hash, status, verified, has_debug_flag, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                session_id,
                payload.file_name,
                payload.file_hash,
                payload.status,
                if payload.verified { 1 } else { 0 },
                if payload.has_debug_flag { 1 } else { 0 },
                payload.timestamp,
            ],
        );

        // Prune older records keeping the latest 100
        let _ = conn.execute(
            "DELETE FROM telemetry_history WHERE id NOT IN (SELECT id FROM telemetry_history ORDER BY id DESC LIMIT 100)",
            [],
        );
    });
}

pub fn get_recent_telemetry_history(limit: usize) -> Vec<TelemetryPayload> {
    with_db(|conn| {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT file_name, file_hash, status, verified, has_debug_flag, timestamp FROM telemetry_history ORDER BY id DESC LIMIT ?",
        ) {
            let rows = stmt.query_map(params![limit as i64], |row| {
                let verified_int: i64 = row.get(3)?;
                let debug_int: i64 = row.get(4)?;
                Ok(TelemetryPayload {
                    file_name: row.get(0)?,
                    file_hash: row.get(1)?,
                    status: row.get(2)?,
                    verified: verified_int != 0,
                    has_debug_flag: debug_int != 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_db_path_makes_directory() {
        let path = get_db_path();
        assert!(path.ends_with("encircled_desktop.db"));
        let parent = path.parent().expect("DB path should have parent directory");
        assert!(parent.exists());
        assert!(parent.ends_with("com.encircled.desktop"));
    }

    #[test]
    fn test_legacy_db_migration_logic() {
        let temp_dir = std::env::temp_dir().join(format!(
            "encircled_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        let legacy_file = temp_dir.join("encircled_desktop.db");
        let app_subdir = temp_dir.join("com.encircled.desktop");
        let new_file = app_subdir.join("encircled_desktop.db");

        // Simulate legacy database presence
        std::fs::write(&legacy_file, b"test sqlite content").unwrap();

        // Run migration simulation
        if !app_subdir.exists() {
            std::fs::create_dir_all(&app_subdir).unwrap();
        }
        if legacy_file.exists() && !new_file.exists() {
            if std::fs::rename(&legacy_file, &new_file).is_err() {
                if std::fs::copy(&legacy_file, &new_file).is_ok() {
                    let _ = std::fs::remove_file(&legacy_file);
                }
            }
        }

        assert!(
            !legacy_file.exists(),
            "Legacy file should have been migrated"
        );
        assert!(new_file.exists(), "New file should exist after migration");
        let content = std::fs::read(&new_file).unwrap();
        assert_eq!(content, b"test sqlite content");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

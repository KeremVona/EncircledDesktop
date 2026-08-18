use rusqlite::{params, Connection};
use std::path::PathBuf;

pub fn init_sqlite_db() -> Result<Connection, rusqlite::Error> {
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
    Ok(conn)
}

pub fn enqueue_offline_upload(
    session_id: &str,
    file_path: &str,
    file_hash: &str,
    file_name: &str,
    timestamp: &str,
) {
    if let Ok(conn) = init_sqlite_db() {
        let _ = conn.execute(
            "INSERT INTO queued_uploads (session_id, file_path, file_hash, file_name, timestamp, retry_count) VALUES (?, ?, ?, ?, ?, 0)",
            params![session_id, file_path, file_hash, file_name, timestamp],
        );
    }
}

use crate::db::{enqueue_offline_upload, init_sqlite_db};
use crate::models::TelemetryPayload;
use crate::process::check_hoi4_process;
use crate::watcher::get_api_base_url;
use reqwest::multipart;
use reqwest::Client;
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

pub async fn process_and_upload(
    app: &AppHandle,
    client: &Client,
    file_path: &Path,
    session_id: &str,
    sequence_index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = fs::File::open(file_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // Compute SHA-256
    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    let hash = format!("{:x}", hasher.finalize());

    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let (_is_running, has_debug_flag) = check_hoi4_process();

    if has_debug_flag {
        eprintln!("WARNING: HOI4 launched with -debug flag!");
    }

    // Step 1: Verify Hash & Anti-Cheat telemetry
    let verify_payload = serde_json::json!({
        "session_id": session_id,
        "player_steam_id": "local_player",
        "file_hash": hash,
        "file_name": file_name,
        "sequence_index": sequence_index,
        "timestamp": timestamp,
        "has_debug_flag": has_debug_flag,
    });

    println!("Verifying save hash preflight with server: {}", hash);

    let base_url = get_api_base_url();
    let verify_res = client
        .post(format!("{}/api/verify-hash", base_url))
        .json(&verify_payload)
        .send()
        .await;

    let mut should_upload = true;
    let mut verified = false;

    if let Ok(res) = verify_res {
        if res.status().is_success() {
            verified = true;
            if let Ok(json) = res.json::<serde_json::Value>().await {
                if let Some(so) = json.get("should_upload").and_then(|v| v.as_bool()) {
                    should_upload = so;
                }
            }
        } else {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            eprintln!("Preflight verification rejected (HTTP {}): {}", status, err_text);
            if status.as_u16() == 409 || status.as_u16() == 422 || status.as_u16() == 400 {
                should_upload = false;
                let _ = app.emit(
                    "telemetry-event",
                    TelemetryPayload {
                        file_name: file_name.clone(),
                        file_hash: hash.clone(),
                        status: format!("REJECTED: Preflight validation failure ({})", status),
                        verified: false,
                        has_debug_flag,
                        timestamp: timestamp.clone(),
                    },
                );
            }
        }
    } else {
        println!("Verify endpoint unreachable or returned error, proceeding with upload fallback.");
    }

    if should_upload {
        println!(
            "Save file size: {} bytes, Hash: {}, Seq: {}",
            buffer.len(),
            hash,
            sequence_index
        );

        let form = multipart::Form::new()
            .text("session_id", session_id.to_string())
            .text("player_steam_id", "local_player")
            .text("file_hash", hash.clone())
            .text("sequence_index", sequence_index.to_string())
            .text("timestamp", timestamp.clone())
            .text("has_debug_flag", has_debug_flag.to_string())
            .part(
                "savefile",
                multipart::Part::bytes(buffer)
                    .file_name(file_name.clone())
                    .mime_str("application/octet-stream")?,
            );

        let upload_res = client
            .post(format!("{}/api/parse-save", base_url))
            .multipart(form)
            .send()
            .await;

        match upload_res {
            Ok(res) if res.status().is_success() => {
                println!("Successfully uploaded save file!");
                
                let status_msg = format!("Autosave {} (Seq #{}) uploaded & verified", file_name, sequence_index);
                
                let _ = app.emit(
                    "telemetry-event",
                    TelemetryPayload {
                        file_name: file_name.clone(),
                        file_hash: hash.clone(),
                        status: status_msg.clone(),
                        verified,
                        has_debug_flag,
                        timestamp,
                    },
                );

                // Native Notification
                let _ = app
                    .notification()
                    .builder()
                    .title("HOI4 Save Telemetry")
                    .body(format!("{} (Debug Flag: {})", status_msg, has_debug_flag))
                    .show();
            }
            Ok(res) => {
                let status = res.status();
                let err_text = res.text().await.unwrap_or_default();
                eprintln!("Upload rejected by server (HTTP {}): {}", status, err_text);

                if status.as_u16() == 409 || status.as_u16() == 422 {
                    // Anti-fraud rejection: do not queue offline retry for fraud saves
                    let _ = app.emit(
                        "telemetry-event",
                        TelemetryPayload {
                            file_name: file_name.clone(),
                            file_hash: hash.clone(),
                            status: format!("INTEGRITY REJECTED: {}", err_text),
                            verified: false,
                            has_debug_flag,
                            timestamp: timestamp.clone(),
                        },
                    );
                } else {
                    println!("Failed/offline upload attempt. Adding to SQLite queue.");
                    enqueue_offline_upload(
                        session_id,
                        &file_path.to_string_lossy(),
                        &hash,
                        &file_name,
                        &timestamp,
                    );

                    let _ = app.emit(
                        "telemetry-event",
                        TelemetryPayload {
                            file_name: file_name.clone(),
                            file_hash: hash.clone(),
                            status: "Offline - Queued in SQLite".into(),
                            verified: false,
                            has_debug_flag,
                            timestamp,
                        },
                    );
                }
            }
            Err(e) => {
                eprintln!("Upload network error: {}", e);
                println!("Failed/offline upload attempt. Adding to SQLite queue.");
                enqueue_offline_upload(
                    session_id,
                    &file_path.to_string_lossy(),
                    &hash,
                    &file_name,
                    &timestamp,
                );

                let _ = app.emit(
                    "telemetry-event",
                    TelemetryPayload {
                        file_name: file_name.clone(),
                        file_hash: hash.clone(),
                        status: "Offline - Queued in SQLite".into(),
                        verified: false,
                        has_debug_flag,
                        timestamp,
                    },
                );
            }
        }
    } else {
        println!("Server indicated upload is not required (Hash already verified).");
    }

    Ok(())
}

pub fn spawn_offline_retry_worker() {
    let retry_client = Client::new();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(30));
            if let Ok(conn) = init_sqlite_db() {
                let mut stmt = match conn.prepare(
                    "SELECT id, session_id, file_path, file_hash, file_name, timestamp, retry_count FROM queued_uploads WHERE retry_count < 10"
                ) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i32>(6)?,
                    ))
                });

                if let Ok(rows) = rows {
                    for item in rows.flatten() {
                        let (id, session_id, file_path_str, file_hash, file_name, timestamp, _retry_count) = item;
                        let path = PathBuf::from(&file_path_str);
                        if !path.exists() {
                            let _ = conn.execute("DELETE FROM queued_uploads WHERE id = ?", params![id]);
                            continue;
                        }

                        if let Ok(buffer) = fs::read(&path) {
                            let form = multipart::Form::new()
                                .text("session_id", session_id)
                                .text("player_steam_id", "local_player")
                                .text("file_hash", file_hash)
                                .text("timestamp", timestamp)
                                .part(
                                    "savefile",
                                    multipart::Part::bytes(buffer)
                                        .file_name(file_name)
                                        .mime_str("application/octet-stream").unwrap(),
                                );

                            let client_ref = retry_client.clone();
                            let retry_base_url = get_api_base_url();
                            let res = rt.block_on(async {
                                client_ref.post(format!("{}/api/parse-save", retry_base_url))
                                    .multipart(form)
                                    .send()
                                    .await
                            });

                            if let Ok(res) = res {
                                if res.status().is_success() {
                                    println!("Retry upload succeeded for queued item ID {}", id);
                                    let _ = conn.execute("DELETE FROM queued_uploads WHERE id = ?", params![id]);
                                } else {
                                    let _ = conn.execute("UPDATE queued_uploads SET retry_count = retry_count + 1 WHERE id = ?", params![id]);
                                }
                            } else {
                                let _ = conn.execute("UPDATE queued_uploads SET retry_count = retry_count + 1 WHERE id = ?", params![id]);
                            }
                        }
                    }
                }
            }
        }
    });
}

use crate::db::enqueue_offline_upload;
use crate::models::TelemetryPayload;
use crate::process::check_hoi4_process;
use crate::watcher::get_api_base_url;
use reqwest::multipart;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tokio_util::io::ReaderStream;

pub const MAX_SAVE_FILE_SIZE: u64 = 250 * 1024 * 1024; // 250 MB cap

/// P-2: Compute SHA-256 hash using a small 64 KB streaming buffer without reading entire file into RAM.
pub fn compute_file_sha256(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let file = fs::File::open(file_path)?;
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub async fn process_and_upload(
    app: &AppHandle,
    client: &Client,
    file_path: &Path,
    session_id: &str,
    sequence_index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let (_is_running, has_debug_flag) = check_hoi4_process();

    // Check file metadata and enforce maximum upload size cap
    let metadata = fs::metadata(file_path)?;
    let file_len = metadata.len();
    if file_len > MAX_SAVE_FILE_SIZE {
        eprintln!(
            "Save file {} rejected: size {} bytes exceeds 250 MB cap",
            file_name, file_len
        );
        let _ = app.emit(
            "telemetry-event",
            TelemetryPayload {
                file_name: file_name.clone(),
                file_hash: "OVERSIZED".into(),
                status: format!("REJECTED: File size ({:.1} MB) exceeds 250 MB limit", (file_len as f64) / (1024.0 * 1024.0)),
                verified: false,
                has_debug_flag,
                timestamp,
            },
        );
        return Err("File size exceeds 250 MB limit".into());
    }

    // P-2: Compute SHA-256 via 64KB streaming buffer without loading 50-200MB into memory
    let hash = compute_file_sha256(file_path)?;

    if has_debug_flag {
        eprintln!("WARNING: HOI4 launched with -debug flag!");
    }

    // Step 1: Verify Hash & Anti-Cheat telemetry (Preflight)
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
            file_len,
            hash,
            sequence_index
        );

        // P-2 & P-3: Stream file directly from disk into HTTP multipart body without buffering 200MB in RAM
        let async_file = tokio::fs::File::open(file_path).await?;
        let stream = ReaderStream::new(async_file);
        let body = reqwest::Body::wrap_stream(stream);
        let file_part = multipart::Part::stream_with_length(body, file_len)
            .file_name(file_name.clone())
            .mime_str("application/octet-stream")?;

        let form = multipart::Form::new()
            .text("session_id", session_id.to_string())
            .text("player_steam_id", "local_player")
            .text("file_hash", hash.clone())
            .text("sequence_index", sequence_index.to_string())
            .text("timestamp", timestamp.clone())
            .text("has_debug_flag", has_debug_flag.to_string())
            .part("savefile", file_part);

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

pub fn spawn_offline_retry_worker(client: Client) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;

            let pending = crate::db::get_pending_queued_uploads();
            for item in pending {
                let path = PathBuf::from(&item.file_path);
                if !path.exists() {
                    crate::db::delete_queued_upload(item.id);
                    continue;
                }

                if let Ok(meta) = fs::metadata(&path) {
                    let file_len = meta.len();
                    if file_len > MAX_SAVE_FILE_SIZE {
                        eprintln!("Queued retry item ID {} exceeds max file size limit, pruning", item.id);
                        crate::db::delete_queued_upload(item.id);
                        continue;
                    }

                    if let Ok(async_file) = tokio::fs::File::open(&path).await {
                        let stream = ReaderStream::new(async_file);
                        let body = reqwest::Body::wrap_stream(stream);
                        if let Ok(file_part) = multipart::Part::stream_with_length(body, file_len)
                            .file_name(item.file_name)
                            .mime_str("application/octet-stream")
                        {
                            let form = multipart::Form::new()
                                .text("session_id", item.session_id)
                                .text("player_steam_id", "local_player")
                                .text("file_hash", item.file_hash)
                                .text("timestamp", item.timestamp)
                                .part("savefile", file_part);

                            let retry_base_url = get_api_base_url();
                            let res = client
                                .post(format!("{}/api/parse-save", retry_base_url))
                                .multipart(form)
                                .send()
                                .await;

                            match res {
                                Ok(r) if r.status().is_success() => {
                                    println!("Retry upload succeeded for queued item ID {}", item.id);
                                    crate::db::delete_queued_upload(item.id);
                                }
                                _ => {
                                    crate::db::increment_queued_upload_retry(item.id);
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

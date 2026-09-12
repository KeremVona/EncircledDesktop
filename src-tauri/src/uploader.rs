use crate::db::{enqueue_offline_upload, NewOfflineUpload};
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

pub static RETRY_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// P-2: Compute SHA-256 hash using a small 64 KB streaming buffer without reading entire file into RAM.
/// Retries file open up to 4 times with 250ms backoff to handle Windows sharing locks (Error 32) when HOI4 is saving.
pub fn compute_file_sha256(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut attempts = 0;
    let file = loop {
        match fs::File::open(file_path) {
            Ok(f) => break f,
            Err(_) if attempts < 4 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(Box::new(e)),
        }
    };
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
    api_key: Option<&str>,
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
        let payload = TelemetryPayload {
            file_name: file_name.clone(),
            file_hash: "OVERSIZED".into(),
            status: format!("REJECTED: File size ({:.1} MB) exceeds 250 MB limit", (file_len as f64) / (1024.0 * 1024.0)),
            verified: false,
            has_debug_flag,
            timestamp,
        };
        crate::db::record_telemetry_event(session_id, &payload);
        let _ = app.emit("telemetry-event", payload);
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
    let mut verify_req = client
        .post(format!("{}/api/verify-hash", base_url))
        .json(&verify_payload);
    if let Some(k) = api_key {
        verify_req = verify_req.header("X-Companion-Key", k);
    }
    let verify_res = verify_req.send().await;

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
                let payload = TelemetryPayload {
                    file_name: file_name.clone(),
                    file_hash: hash.clone(),
                    status: format!("REJECTED: Preflight validation failure ({})", status),
                    verified: false,
                    has_debug_flag,
                    timestamp: timestamp.clone(),
                };
                crate::db::record_telemetry_event(session_id, &payload);
                let _ = app.emit("telemetry-event", payload);
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
        let mut attempts = 0;
        let async_file = loop {
            match tokio::fs::File::open(file_path).await {
                Ok(f) => break f,
                Err(_) if attempts < 4 => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(e) => return Err(Box::new(e)),
            }
        };
        let stream = ReaderStream::new(async_file);
        let body = reqwest::Body::wrap_stream(stream);
        let file_part = multipart::Part::stream_with_length(body, file_len)
            .file_name(file_name.clone())
            .mime_str("application/octet-stream")?;

        let mut form = multipart::Form::new()
            .text("session_id", session_id.to_string())
            .text("player_steam_id", "local_player")
            .text("file_hash", hash.clone())
            .text("sequence_index", sequence_index.to_string())
            .text("timestamp", timestamp.clone())
            .text("has_debug_flag", has_debug_flag.to_string())
            .part("savefile", file_part);

        if let Some(k) = api_key {
            form = form.text("companion_key", k.to_string());
        }

        let mut upload_req = client
            .post(format!("{}/api/parse-save", base_url))
            .multipart(form);
        if let Some(k) = api_key {
            upload_req = upload_req.header("X-Companion-Key", k);
        }
        let upload_res = upload_req.send().await;

        match upload_res {
            Ok(res) if res.status().is_success() => {
                println!("Successfully uploaded save file!");
                
                let status_msg = format!("Autosave {} (Seq #{}) uploaded & verified", file_name, sequence_index);
                let payload = TelemetryPayload {
                    file_name: file_name.clone(),
                    file_hash: hash.clone(),
                    status: status_msg.clone(),
                    verified,
                    has_debug_flag,
                    timestamp,
                };
                crate::db::record_telemetry_event(session_id, &payload);
                let _ = app.emit("telemetry-event", &payload);

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
                    let payload = TelemetryPayload {
                        file_name: file_name.clone(),
                        file_hash: hash.clone(),
                        status: format!("INTEGRITY REJECTED: {}", err_text),
                        verified: false,
                        has_debug_flag,
                        timestamp: timestamp.clone(),
                    };
                    crate::db::record_telemetry_event(session_id, &payload);
                    let _ = app.emit("telemetry-event", payload);
                } else {
                    println!("Failed/offline upload attempt. Adding to SQLite queue.");
                    enqueue_offline_upload(&NewOfflineUpload {
                        session_id,
                        file_path: &file_path.to_string_lossy(),
                        file_hash: &hash,
                        file_name: &file_name,
                        timestamp: &timestamp,
                        api_key,
                        sequence_index,
                        has_debug_flag,
                    });

                    let payload = TelemetryPayload {
                        file_name: file_name.clone(),
                        file_hash: hash.clone(),
                        status: "Offline - Queued in SQLite".into(),
                        verified: false,
                        has_debug_flag,
                        timestamp,
                    };
                    crate::db::record_telemetry_event(session_id, &payload);
                    let _ = app.emit("telemetry-event", payload);
                }
            }
            Err(e) => {
                eprintln!("Upload network error: {}", e);
                println!("Failed/offline upload attempt. Adding to SQLite queue.");
                enqueue_offline_upload(&NewOfflineUpload {
                    session_id,
                    file_path: &file_path.to_string_lossy(),
                    file_hash: &hash,
                    file_name: &file_name,
                    timestamp: &timestamp,
                    api_key,
                    sequence_index,
                    has_debug_flag,
                });

                let payload = TelemetryPayload {
                    file_name: file_name.clone(),
                    file_hash: hash.clone(),
                    status: "Offline - Queued in SQLite".into(),
                    verified: false,
                    has_debug_flag,
                    timestamp,
                };
                crate::db::record_telemetry_event(session_id, &payload);
                let _ = app.emit("telemetry-event", payload);
            }
        }
    } else {
        println!("Server indicated upload is not required (Hash already verified).");
        if verified {
            let status_msg = format!("Autosave {} (Seq #{}) verified on server", file_name, sequence_index);
            let payload = TelemetryPayload {
                file_name: file_name.clone(),
                file_hash: hash.clone(),
                status: status_msg.clone(),
                verified: true,
                has_debug_flag,
                timestamp: timestamp.clone(),
            };
            crate::db::record_telemetry_event(session_id, &payload);
            let _ = app.emit("telemetry-event", &payload);

            // Native Notification
            let _ = app
                .notification()
                .builder()
                .title("HOI4 Save Telemetry")
                .body(format!("{} (Debug Flag: {})", status_msg, has_debug_flag))
                .show();
        }
    }

    Ok(())
}

pub fn spawn_offline_retry_worker(client: Client, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;

            let _guard = match RETRY_MUTEX.try_lock() {
                Ok(g) => g,
                Err(_) => continue, // Manual or another background retry in progress
            };

            let pending = crate::db::get_pending_queued_uploads();
            for item in pending {
                let path = PathBuf::from(&item.file_path);
                if !path.exists() {
                    crate::db::delete_queued_upload(item.id);
                    continue;
                }

                // Anti-cheat / Hash mismatch check: If autosave.hoi4 was overwritten while offline,
                // retrying with the old hash will cause a hash mismatch / anti-cheat ban!
                match compute_file_sha256(&path) {
                    Ok(current_hash) => {
                        if current_hash != item.file_hash {
                            eprintln!(
                                "Queued item ID {} ({}) was overwritten while offline (hash changed from {} to {}). Pruning stale entry.",
                                item.id, item.file_name, item.file_hash, current_hash
                            );
                            crate::db::delete_queued_upload(item.id);
                            continue;
                        }
                    }
                    Err(e) => {
                        eprintln!("Error hashing queued file {:?}: {}", path, e);
                        continue;
                    }
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
                            .file_name(item.file_name.clone())
                            .mime_str("application/octet-stream")
                        {
                            let mut form = multipart::Form::new()
                                .text("session_id", item.session_id.clone())
                                .text("player_steam_id", "local_player")
                                .text("file_hash", item.file_hash.clone())
                                .text("sequence_index", item.sequence_index.to_string())
                                .text("timestamp", item.timestamp.clone())
                                .text("has_debug_flag", item.has_debug_flag.to_string())
                                .part("savefile", file_part);

                            if let Some(ref k) = item.api_key {
                                form = form.text("companion_key", k.clone());
                            }

                            let retry_base_url = get_api_base_url();
                            let mut retry_req = client
                                .post(format!("{}/api/parse-save", retry_base_url))
                                .multipart(form);
                            if let Some(ref k) = item.api_key {
                                retry_req = retry_req.header("X-Companion-Key", k);
                            }
                            let res = retry_req.send().await;

                            match res {
                                Ok(r) if r.status().is_success() => {
                                    println!("Retry upload succeeded for queued item ID {}", item.id);
                                    crate::db::delete_queued_upload(item.id);
                                    let status_msg = format!("Queued autosave {} (Seq #{}) synced & verified", item.file_name, item.sequence_index);
                                    let payload = TelemetryPayload {
                                        file_name: item.file_name.clone(),
                                        file_hash: item.file_hash.clone(),
                                        status: status_msg.clone(),
                                        verified: true,
                                        has_debug_flag: item.has_debug_flag,
                                        timestamp: item.timestamp.clone(),
                                    };
                                    crate::db::record_telemetry_event(&item.session_id, &payload);
                                    let _ = app.emit("telemetry-event", &payload);
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

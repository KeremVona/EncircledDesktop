use crate::models::{AppState, ProcessStatusPayload};
use crate::process::check_hoi4_process;
use crate::uploader::process_and_upload;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

pub fn get_api_base_url() -> String {
    std::env::var("ENCIRCLED_API_URL").unwrap_or_else(|_| "http://localhost:5292".to_string())
}

pub fn get_default_save_path() -> Result<PathBuf, String> {
    // 1. Check standard Documents directory
    if let Some(doc_dir) = dirs::document_dir() {
        let mut hoi4_dir = doc_dir.clone();
        hoi4_dir.push("Paradox Interactive");
        hoi4_dir.push("Hearts of Iron IV");
        hoi4_dir.push("save games");
        if hoi4_dir.exists() {
            return Ok(hoi4_dir);
        }

        // 2. Check OneDrive\Documents if standard path does not exist
        if let Some(home) = dirs::home_dir() {
            let mut onedrive_dir = home.clone();
            onedrive_dir.push("OneDrive");
            onedrive_dir.push("Documents");
            onedrive_dir.push("Paradox Interactive");
            onedrive_dir.push("Hearts of Iron IV");
            onedrive_dir.push("save games");
            if onedrive_dir.exists() {
                return Ok(onedrive_dir);
            }
        }
        Ok(hoi4_dir)
    } else {
        Err("Could not determine Documents directory".into())
    }
}

#[tauri::command]
pub fn get_process_status() -> ProcessStatusPayload {
    let (is_running, has_debug_flag) = check_hoi4_process();
    ProcessStatusPayload {
        is_running,
        has_debug_flag,
    }
}

#[tauri::command]
pub fn get_default_save_path_cmd() -> Result<String, String> {
    let path = get_default_save_path()?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn select_save_folder(initial_path: Option<String>) -> Result<Option<String>, String> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title("Select HOI4 Save Games Directory");

    if let Some(ref p) = initial_path {
        if !p.is_empty() {
            dialog = dialog.set_directory(p);
        }
    } else if let Ok(default_p) = get_default_save_path() {
        dialog = dialog.set_directory(&default_p);
    }

    let folder = dialog.pick_folder().await;
    if let Some(f) = folder {
        Ok(Some(f.path().to_string_lossy().to_string()))
    } else {
        Ok(None)
    }
}

fn is_valid_uuid_format(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            if b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn is_valid_api_key_format(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn is_safe_watch_path(p: &std::path::Path) -> bool {
    let p_str = p.to_string_lossy();
    if p_str.is_empty() || p_str.len() > 512 || p_str.contains('\0') || p_str.contains("..") {
        return false;
    }
    // Block system root directories
    if p == std::path::Path::new("/")
        || p == std::path::Path::new("C:\\")
        || p == std::path::Path::new("C:/")
        || p == std::path::Path::new("D:\\")
        || p == std::path::Path::new("D:/")
    {
        return false;
    }
    true
}

#[tauri::command]
pub async fn start_watching(
    app: AppHandle,
    session_id: String,
    api_key: Option<String>,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    if !is_valid_uuid_format(&session_id) {
        return Err("Invalid session_id: Must be a valid UUID".into());
    }

    if let Some(ref key) = api_key {
        if !is_valid_api_key_format(key) {
            return Err("Invalid api_key: Malformed characters or length exceeded".into());
        }
    }

    // If a watcher is already running, we stop it by signaling its channel.
    {
        let mut tx_guard = state.watcher_tx.lock().unwrap();
        if let Some(tx) = tx_guard.take() {
            let _ = tx.send(()); // Signal to stop previous watcher
        }
    }

    let watch_path = if let Some(p) = path.clone() {
        if p.is_empty() {
            get_default_save_path()?
        } else {
            let pb = PathBuf::from(p);
            if !is_safe_watch_path(&pb) {
                return Err("Invalid or unsafe directory path specified".into());
            }
            pb
        }
    } else {
        get_default_save_path()?
    };

    if !watch_path.exists() {
        if let Err(e) = fs::create_dir_all(&watch_path) {
            return Err(format!(
                "Path does not exist and could not be made: {:?} ({})",
                watch_path, e
            ));
        }
    }

    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);

    {
        let mut tx_guard = state.watcher_tx.lock().unwrap();
        *tx_guard = Some(stop_tx);
    }

    let session_id_for_thread = session_id.clone();
    let api_key_clone = api_key.clone();
    let watch_path_for_thread = watch_path.clone();
    let shared_client = state.client.clone();

    // Notify server of active watcher
    let connect_client = shared_client.clone();
    let connect_session_id = session_id.clone();
    let connect_key = api_key.clone();
    let connect_path_str = watch_path.to_string_lossy().to_string();
    let base_url = get_api_base_url();
    tauri::async_runtime::spawn(async move {
        let mut conn_req = connect_client
            .post(format!(
                "{}/api/lobbies/{}/desktop/connect",
                base_url, connect_session_id
            ));
        if let Some(ref k) = connect_key {
            conn_req = conn_req.header("X-Companion-Key", k);
        }
        let _ = conn_req.send().await;

        let watcher_payload = serde_json::json!({
            "isActive": true,
            "saveFolderPath": connect_path_str,
            "isValidated": true,
            "companionKey": connect_key
        });

        let mut watcher_req = connect_client
            .post(format!(
                "{}/api/lobbies/{}/desktop/watcher-status",
                base_url, connect_session_id
            ))
            .json(&watcher_payload);
        if let Some(ref k) = connect_key {
            watcher_req = watcher_req.header("X-Companion-Key", k);
        }
        let _ = watcher_req.send().await;
    });

    let app_for_thread = app.clone();
    std::thread::spawn(move || {
        let session_id = session_id_for_thread;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut debouncer = match new_debouncer(Duration::from_secs(3), tx) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Failed to make debouncer: {}", e);
                    let _ = app_for_thread.emit(
                        "telemetry-event",
                        serde_json::json!({
                            "file_name": "Watcher",
                            "file_hash": "N/A",
                            "status": format!("ERROR: Debouncer initialization failed ({})", e),
                            "verified": false,
                            "has_debug_flag": false,
                            "timestamp": ""
                        }),
                    );
                    return;
                }
            };

            if let Err(e) = debouncer
                .watcher()
                .watch(&watch_path_for_thread, RecursiveMode::NonRecursive)
            {
                eprintln!("Failed to watch path: {}", e);
                let _ = app_for_thread.emit(
                    "telemetry-event",
                    serde_json::json!({
                        "file_name": "Watcher",
                        "file_hash": "N/A",
                        "status": format!("ERROR: Failed to watch path ({})", e),
                        "verified": false,
                        "has_debug_flag": false,
                        "timestamp": ""
                    }),
                );
                return;
            }

            let client = shared_client;
            let mut sequence_index: u32 = 0;
            let mut last_heartbeat = std::time::Instant::now();

            println!("Started watching: {:?}", watch_path_for_thread);

            loop {
                // Check if we should stop
                if stop_rx.try_recv().is_ok() {
                    println!("Stopping watcher.");
                    break;
                }

                // Periodic heartbeat to server every 5 seconds
                if last_heartbeat.elapsed() >= Duration::from_secs(5) {
                    last_heartbeat = std::time::Instant::now();
                    let hb_client = client.clone();
                    let hb_session_id = session_id.clone();
                    let hb_base_url = get_api_base_url();
                    let hb_key = api_key_clone.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut hb_req = hb_client
                            .post(format!("{}/api/lobbies/{}/desktop/heartbeat", hb_base_url, hb_session_id));
                        if let Some(ref k) = hb_key {
                            hb_req = hb_req.header("X-Companion-Key", k);
                        }
                        let _ = hb_req.send().await;
                    });
                }

                // Check for file events
                if let Ok(Ok(events)) = rx.recv_timeout(Duration::from_millis(500)) {
                    for event in events {
                        let path = event.path;
                        let file_name_str = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();
                        if path.extension().and_then(|e| e.to_str()) == Some("hoi4")
                            && !file_name_str.contains("temp")
                            && !file_name_str.ends_with(".tmp")
                        {
                            if !path.exists() {
                                continue;
                            }
                            sequence_index += 1;
                            let seq = sequence_index;
                            println!("Detected save file change (Seq #{}): {:?}", seq, path);

                            let session_id_clone = session_id.clone();
                            let client_clone = client.clone();
                            let path_clone = path.clone();
                            let app_clone = app_for_thread.clone();

                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = process_and_upload(
                                    &app_clone,
                                    &client_clone,
                                    &path_clone,
                                    &session_id_clone,
                                    seq,
                                )
                                .await
                                {
                                    eprintln!("Error processing/uploading: {}", e);
                                }
                            });
                        }
                    }
                }
            }

            // On watcher stop, inform the server of disconnect
            let dc_client = client.clone();
            let dc_session_id = session_id.clone();
            let dc_key = api_key_clone.clone();
            let dc_base_url = get_api_base_url();
            tauri::async_runtime::spawn(async move {
                let mut dc_req = dc_client
                    .post(format!("{}/api/lobbies/{}/desktop/disconnect", dc_base_url, dc_session_id));
                if let Some(ref k) = dc_key {
                    dc_req = dc_req.header("X-Companion-Key", k);
                }
                let _ = dc_req.send().await;
            });
        }));

        if let Err(panic_err) = result {
            eprintln!("Unhandled panic in watcher thread: {:?}", panic_err);
            let _ = app_for_thread.emit(
                "telemetry-event",
                serde_json::json!({
                    "file_name": "Watcher Thread",
                    "file_hash": "CRASH",
                    "status": "FATAL: Watcher thread crashed unexpectedly. Please restart watcher.",
                    "verified": false,
                    "has_debug_flag": false,
                    "timestamp": ""
                }),
            );
        }
    });

    if let Some(tray) = app.tray_by_id("main-tray") {
        let short_id = if session_id.len() >= 8 { &session_id[..8] } else { &session_id };
        let _ = tray.set_tooltip(Some(format!("Encircled Desktop · Watching (Lobby: {})", short_id)));
    }

    Ok(format!("Started watching {:?}", watch_path))
}

#[tauri::command]
pub async fn stop_watching(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some("Encircled Desktop · Standby".to_string()));
    }
    let mut tx_guard = state.watcher_tx.lock().unwrap();
    if let Some(tx) = tx_guard.take() {
        let _ = tx.send(());
        Ok("Watcher stopped".into())
    } else {
        Ok("No active watcher was running".into())
    }
}

#[tauri::command]
pub fn update_tray_tooltip(tooltip: String, app: AppHandle) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(tooltip));
    }
    Ok(())
}

#[tauri::command]
pub fn get_offline_queue() -> Vec<crate::db::QueuedUpload> {
    crate::db::get_pending_queued_uploads()
}

#[tauri::command]
pub fn clear_offline_queue() -> Result<usize, String> {
    crate::db::clear_all_queued_uploads()
}

#[tauri::command]
pub async fn retry_offline_queue_now(state: State<'_, AppState>) -> Result<usize, String> {
    let client = state.client.clone();
    let pending = crate::db::get_pending_queued_uploads();
    let mut success_count = 0;
    for item in &pending {
        let path = PathBuf::from(&item.file_path);
        if !path.exists() {
            crate::db::delete_queued_upload(item.id);
            continue;
        }

        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > crate::uploader::MAX_SAVE_FILE_SIZE {
                crate::db::delete_queued_upload(item.id);
                continue;
            }
        }

        if let Ok(buffer) = fs::read(&path) {
            let form = reqwest::multipart::Form::new()
                .text("session_id", item.session_id.clone())
                .text("player_steam_id", "local_player")
                .text("file_hash", item.file_hash.clone())
                .text("timestamp", item.timestamp.clone())
                .part(
                    "savefile",
                    reqwest::multipart::Part::bytes(buffer)
                        .file_name(item.file_name.clone())
                        .mime_str("application/octet-stream")
                        .unwrap_or_else(|_| reqwest::multipart::Part::bytes(vec![])),
                );

            let retry_base_url = get_api_base_url();
            let res = client
                .post(format!("{}/api/parse-save", retry_base_url))
                .multipart(form)
                .send()
                .await;

            match res {
                Ok(r) if r.status().is_success() => {
                    crate::db::delete_queued_upload(item.id);
                    success_count += 1;
                }
                _ => {
                    crate::db::increment_queued_upload_retry(item.id);
                }
            }
        }
    }
    Ok(success_count)
}

#[tauri::command]
pub fn get_autostart_status() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "EncircledDesktop"])
            .output();
        if let Ok(out) = output {
            Ok(out.status.success())
        } else {
            Ok(false)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub fn set_autostart(enable: bool) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if enable {
            let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let exe_str = current_exe.to_string_lossy();
            let status = Command::new("reg")
                .args(["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "EncircledDesktop", "/t", "REG_SZ", "/d", &format!("\"{}\"", exe_str), "/f"])
                .status()
                .map_err(|e| e.to_string())?;
            Ok(status.success())
        } else {
            let _ = Command::new("reg")
                .args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "EncircledDesktop", "/f"])
                .status();
            Ok(false)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct UpdateCheckResponse {
    pub available: bool,
    pub version: Option<String>,
    pub current_version: String,
    pub body: Option<String>,
}

#[tauri::command]
pub async fn check_for_updates_cmd(app: AppHandle) -> Result<UpdateCheckResponse, String> {
    use tauri_plugin_updater::UpdaterExt;
    let current_version = app.package_info().version.to_string();
    match app.updater() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => Ok(UpdateCheckResponse {
                available: true,
                version: Some(update.version),
                current_version,
                body: update.body,
            }),
            Ok(None) => Ok(UpdateCheckResponse {
                available: false,
                version: None,
                current_version,
                body: None,
            }),
            Err(e) => Err(format!("Update check failed: {}", e)),
        },
        Err(e) => Err(format!("Updater unavailable: {}", e)),
    }
}

#[tauri::command]
pub fn get_minimize_to_tray_status(state: State<'_, AppState>) -> bool {
    *state.minimize_to_tray.lock().unwrap()
}

#[tauri::command]
pub fn set_minimize_to_tray(enable: bool, state: State<'_, AppState>) -> bool {
    let mut guard = state.minimize_to_tray.lock().unwrap();
    *guard = enable;
    enable
}

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

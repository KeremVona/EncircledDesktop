use crate::models::{AppState, ProcessStatusPayload};
use crate::process::check_hoi4_process;
use crate::uploader::process_and_upload;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use reqwest::Client;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, State};
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

#[tauri::command]
pub async fn start_watching(
    app: AppHandle,
    session_id: String,
    api_key: Option<String>,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
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
            PathBuf::from(p)
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

    let session_id = session_id.clone();
    let api_key_clone = api_key.clone();
    let watch_path_for_thread = watch_path.clone();

    // Notify server of active watcher
    let connect_client = Client::new();
    let connect_session_id = session_id.clone();
    let connect_key = api_key.clone();
    let connect_path_str = watch_path.to_string_lossy().to_string();
    let base_url = get_api_base_url();
    tokio::spawn(async move {
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

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = match new_debouncer(Duration::from_secs(3), tx) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to make debouncer: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer
            .watcher()
            .watch(&watch_path_for_thread, RecursiveMode::NonRecursive)
        {
            eprintln!("Failed to watch path: {}", e);
            return;
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = Client::new();
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
                rt.spawn(async move {
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
                    // We only care about file changes/makings that are .hoi4 files
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
                        let app_clone = app.clone();

                        rt.block_on(async {
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
        rt.block_on(async move {
            let mut dc_req = dc_client
                .post(format!("{}/api/lobbies/{}/desktop/disconnect", dc_base_url, dc_session_id));
            if let Some(ref k) = dc_key {
                dc_req = dc_req.header("X-Companion-Key", k);
            }
            let _ = dc_req.send().await;
        });
    });

    Ok(format!("Started watching {:?}", watch_path))
}

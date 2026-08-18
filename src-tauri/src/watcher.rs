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

const API_BASE_URL: &str = "http://localhost:5292";

pub fn get_default_save_path() -> Result<PathBuf, String> {
    if let Some(mut doc_dir) = dirs::document_dir() {
        doc_dir.push("Paradox Interactive");
        doc_dir.push("Hearts of Iron IV");
        doc_dir.push("save games");
        Ok(doc_dir)
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
pub async fn start_watching(
    app: AppHandle,
    session_id: String,
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
    let watch_path_for_thread = watch_path.clone();

    // Notify server of active watcher
    let connect_client = Client::new();
    let connect_session_id = session_id.clone();
    let connect_path_str = watch_path.to_string_lossy().to_string();
    tokio::spawn(async move {
        let _ = connect_client
            .post(format!(
                "{}/api/lobbies/{}/desktop/connect",
                API_BASE_URL, connect_session_id
            ))
            .send()
            .await;

        let watcher_payload = serde_json::json!({
            "isActive": true,
            "saveFolderPath": connect_path_str,
            "isValidated": true
        });

        let _ = connect_client
            .post(format!(
                "{}/api/lobbies/{}/desktop/watcher-status",
                API_BASE_URL, connect_session_id
            ))
            .json(&watcher_payload)
            .send()
            .await;
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

        println!("Started watching: {:?}", watch_path_for_thread);

        loop {
            // Check if we should stop
            if stop_rx.try_recv().is_ok() {
                println!("Stopping watcher.");
                break;
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
    });

    Ok(format!("Started watching {:?}", watch_path))
}

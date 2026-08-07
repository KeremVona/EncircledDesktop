#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use reqwest::multipart;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::State;
use tokio::sync::mpsc;
use zstd::stream::encode_all;

#[derive(Default)]
struct AppState {
    watcher_tx: Mutex<Option<mpsc::Sender<()>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WatchConfig {
    session_id: String,
    path: Option<String>,
}

#[tauri::command]
async fn start_watching(
    config: WatchConfig,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // If a watcher is already running, we stop it by dropping its channel.
    {
        let mut tx_guard = state.watcher_tx.lock().unwrap();
        if let Some(tx) = tx_guard.take() {
            let _ = tx.send(()); // Signal to stop
        }
    }

    let watch_path = if let Some(p) = config.path.clone() {
        if p.is_empty() {
            get_default_save_path()?
        } else {
            PathBuf::from(p)
        }
    } else {
        get_default_save_path()?
    };

    if !watch_path.exists() {
        return Err(format!("Path does not exist: {:?}", watch_path));
    }

    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    
    {
        let mut tx_guard = state.watcher_tx.lock().unwrap();
        *tx_guard = Some(stop_tx);
    }

    let session_id = config.session_id.clone();
    let watch_path_for_thread = watch_path.clone();
    
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = match new_debouncer(Duration::from_secs(3), tx) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to create debouncer: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer.watcher().watch(&watch_path_for_thread, RecursiveMode::NonRecursive) {
            eprintln!("Failed to watch path: {}", e);
            return;
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = Client::new();

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
                    // We only care about file modifications/creations that are .hoi4 files
                    let path = event.path;
                    if path.extension().and_then(|e| e.to_str()) == Some("hoi4") {
                        println!("Detected save file change: {:?}", path);
                        
                        let session_id_clone = session_id.clone();
                        let client_clone = client.clone();
                        let path_clone = path.clone();
                        
                        rt.block_on(async {
                            if let Err(e) = process_and_upload(&client_clone, &path_clone, &session_id_clone).await {
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

fn get_default_save_path() -> Result<PathBuf, String> {
    if let Some(mut doc_dir) = dirs::document_dir() {
        doc_dir.push("Paradox Interactive");
        doc_dir.push("Hearts of Iron IV");
        doc_dir.push("save games");
        Ok(doc_dir)
    } else {
        Err("Could not determine Documents directory".into())
    }
}

async fn process_and_upload(client: &Client, file_path: &Path, session_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = fs::File::open(file_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    // Compute SHA-256
    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    let hash = format!("{:x}", hasher.finalize());

    // Compress with zstd
    let level = 3; // default level
    let compressed_data = encode_all(&buffer[..], level)?;

    println!("Original size: {}, Compressed size: {}, Hash: {}", buffer.len(), compressed_data.len(), hash);

    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let form = multipart::Form::new()
        .text("session_id", session_id.to_string())
        .text("player_steam_id", "local_player") // Placeholder for Phase 1
        .text("file_hash", hash)
        .text("timestamp", timestamp)
        .part(
            "savefile",
            multipart::Part::bytes(compressed_data)
                .file_name(file_name)
                .mime_str("application/octet-stream")?,
        );

    // Using localhost:3000 as requested
    let res = client.post("http://localhost:3000/api/parse-save")
        .multipart(form)
        .send()
        .await?;

    if res.status().is_success() {
        println!("Successfully uploaded save file!");
    } else {
        println!("Failed to upload save file: {:?}", res.status());
    }

    Ok(())
}

fn main() {
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![start_watching])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

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
use tauri::{State, Emitter, AppHandle};
use tauri::{tray::TrayIconBuilder, menu::{Menu, MenuItem}};
use rusqlite::{params, Connection};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_notification::NotificationExt;
use sysinfo::System;
use tokio::sync::mpsc;
use zstd::stream::encode_all;

#[derive(Clone, Serialize, Deserialize)]
struct ProcessStatusPayload {
    is_running: bool,
    has_debug_flag: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct TelemetryPayload {
    file_name: String,
    file_hash: String,
    status: String,
    verified: bool,
    has_debug_flag: bool,
    timestamp: String,
}

fn check_hoi4_process() -> (bool, bool) {
    let mut sys = System::new_all();
    sys.refresh_all();
    let mut is_running = false;
    let mut has_debug = false;
    for process in sys.processes_by_exact_name("hoi4.exe") {
        is_running = true;
        for arg in process.cmd() {
            if arg.contains("-debug") || arg.contains("--debug") {
                has_debug = true;
            }
        }
    }
    (is_running, has_debug)
}

fn init_sqlite_db() -> Result<Connection, rusqlite::Error> {
    let mut db_path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    db_path.push("hoi4_companion.db");
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

fn enqueue_offline_upload(
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
    app: AppHandle,
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
                        let app_clone = app.clone();
                        
                        rt.block_on(async {
                            if let Err(e) = process_and_upload(&app_clone, &client_clone, &path_clone, &session_id_clone).await {
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

async fn process_and_upload(
    app: &AppHandle,
    client: &Client,
    file_path: &Path,
    session_id: &str,
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
        "timestamp": timestamp,
        "has_debug_flag": has_debug_flag,
    });

    println!("Verifying hash with server: {}", hash);

    let verify_res = client
        .post("http://localhost:3000/api/verify-hash")
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
        }
    } else {
        println!("Verify endpoint unreachable or returned error, proceeding with upload fallback.");
    }

    if should_upload {
        // Compress with zstd
        let level = 3;
        let compressed_data = encode_all(&buffer[..], level)?;

        println!(
            "Original size: {}, Compressed size: {}, Hash: {}",
            buffer.len(),
            compressed_data.len(),
            hash
        );

        let form = multipart::Form::new()
            .text("session_id", session_id.to_string())
            .text("player_steam_id", "local_player")
            .text("file_hash", hash.clone())
            .text("timestamp", timestamp.clone())
            .text("has_debug_flag", has_debug_flag.to_string())
            .part(
                "savefile",
                multipart::Part::bytes(compressed_data)
                    .file_name(file_name.clone())
                    .mime_str("application/octet-stream")?,
            );

        let upload_res = client
            .post("http://localhost:3000/api/parse-save")
            .multipart(form)
            .send()
            .await;

        match upload_res {
            Ok(res) if res.status().is_success() => {
                println!("Successfully uploaded save file!");
                
                let status_msg = format!("Autosave {} uploaded & verified", file_name);
                
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
                    .title("HOI4 Save Verified")
                    .body(format!("{} (Debug Flag: {})", status_msg, has_debug_flag))
                    .show();
            }
            _ => {
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

fn main() {
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let _ = init_sqlite_db();
            
            app.deep_link().on_open_url(move |event| {
                let urls = event.urls();
                println!("Received deep link urls: {:?}", urls);
                if let Some(url) = urls.first() {
                    let _ = handle.emit("deep-link-received", url.to_string());
                }
            });
            
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_i])?;
            
            // Generate a simple tray icon setup
            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("HOI4 Companion - Idle")
                .on_menu_event(|app, event| {
                    if event.id == tauri::menu::MenuId::new("quit") {
                        app.exit(0);
                    }
                })
                .build(app)?;

            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    let (is_running, has_debug_flag) = check_hoi4_process();
                    let _ = app_handle.emit(
                        "process-status",
                        ProcessStatusPayload {
                            is_running,
                            has_debug_flag,
                        },
                    );
                    std::thread::sleep(Duration::from_secs(5));
                }
            });

            // SQLite Offline Retry Loop Thread
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
                                let (id, session_id, file_path_str, file_hash, file_name, timestamp, retry_count) = item;
                                let path = PathBuf::from(&file_path_str);
                                if !path.exists() {
                                    let _ = conn.execute("DELETE FROM queued_uploads WHERE id = ?", params![id]);
                                    continue;
                                }

                                if let Ok(buffer) = fs::read(&path) {
                                    if let Ok(compressed_data) = encode_all(&buffer[..], 3) {
                                        let form = multipart::Form::new()
                                            .text("session_id", session_id)
                                            .text("player_steam_id", "local_player")
                                            .text("file_hash", file_hash)
                                            .text("timestamp", timestamp)
                                            .part(
                                                "savefile",
                                                multipart::Part::bytes(compressed_data)
                                                    .file_name(file_name)
                                                    .mime_str("application/octet-stream").unwrap(),
                                            );

                                        let client_ref = retry_client.clone();
                                        let res = rt.block_on(async {
                                            client_ref.post("http://localhost:3000/api/parse-save")
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
                }
            });

            Ok(())
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![start_watching])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

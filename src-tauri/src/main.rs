#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod db;
mod models;
mod process;
mod uploader;
mod watcher;

use db::init_sqlite_db;
use models::{AppState, ProcessStatusPayload};
use process::check_hoi4_process;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Emitter;
use tauri_plugin_deep_link::DeepLinkExt;
use uploader::spawn_offline_retry_worker;
use watcher::{get_default_save_path_cmd, get_process_status, select_save_folder, start_watching};

fn main() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let _ = init_sqlite_db();

            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                let _ = app.deep_link().register_all();
            }

            app.deep_link().on_open_url(move |event| {
                let urls = event.urls();
                println!("Received deep link urls: {:?}", urls);
                if let Some(url) = urls.first() {
                    let _ = handle.emit("deep-link-received", url.to_string());
                }
            });

            let handle_for_args = app.handle().clone();
            let args: Vec<String> = std::env::args().collect();
            if let Some(arg) = args.iter().find(|a| a.starts_with("encircled://") || a.starts_with("encircled-desktop://")) {
                let deep_url = arg.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(800));
                    let _ = handle_for_args.emit("deep-link-received", deep_url);
                });
            }

            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_i])?;

            // Setup system tray icon & menu
            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Encircled Desktop")
                .on_menu_event(|app, event| {
                    if event.id == tauri::menu::MenuId::new("quit") {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // Background HOI4 Process Polling Thread (emits process-status every 5s)
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
                    std::thread::sleep(Duration::from_secs(2));
                }
            });

            // SQLite Offline Retry Worker Thread (retries failed uploads every 30s)
            spawn_offline_retry_worker();

            Ok(())
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_watching,
            get_process_status,
            get_default_save_path_cmd,
            select_save_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

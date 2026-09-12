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
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;
use uploader::spawn_offline_retry_worker;
use watcher::{
    check_for_updates_cmd, clear_offline_queue, get_app_version, get_autostart_status,
    get_default_save_path_cmd, get_minimize_to_tray_status, get_offline_queue,
    get_process_status, get_telemetry_history, retry_offline_queue_now, select_save_folder,
    set_autostart, set_minimize_to_tray, start_watching, stop_watching, update_tray_tooltip,
};

fn main() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            if let Some(arg) = args.iter().find(|a| a.starts_with("encircled://") || a.starts_with("encircled-desktop://")) {
                let _ = app.emit("deep-link-received", arg.clone());
            }
        }))
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

            let toggle_i = MenuItem::with_id(app, "toggle", "Show / Hide Encircled", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_i, &quit_i])?;

            // Setup system tray icon & menu
            let mut tray_builder = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .tooltip("Encircled Desktop · Standby");

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            let _tray = tray_builder
                .on_menu_event(|app, event| {
                    if event.id == tauri::menu::MenuId::new("quit") {
                        app.exit(0);
                    } else if event.id == tauri::menu::MenuId::new("toggle") {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                    | TrayIconEvent::DoubleClick { .. } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // Background HOI4 Process Polling Thread (emits process-status on change and periodic heartbeat)
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_state: Option<(bool, bool)> = None;
                let mut tick_count: u32 = 0;
                loop {
                    let current_state = check_hoi4_process();
                    tick_count = tick_count.wrapping_add(1);
                    let state_changed = last_state != Some(current_state);
                    let heartbeat = tick_count.is_multiple_of(5);

                    if state_changed || heartbeat {
                        last_state = Some(current_state);
                        let (is_running, has_debug_flag) = current_state;
                        if state_changed {
                            println!(
                                "[Process] HOI4 status changed: is_running={}, has_debug_flag={}",
                                is_running, has_debug_flag
                            );
                        }
                        if let Err(e) = app_handle.emit(
                            "process-status",
                            ProcessStatusPayload {
                                is_running,
                                has_debug_flag,
                            },
                        ) {
                            eprintln!("[Process] Failed to emit process-status: {}", e);
                        }
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
            });

            // SQLite Offline Retry Worker Thread (retries failed uploads every 30s)
            let shared_client = app.state::<AppState>().client.clone();
            spawn_offline_retry_worker(shared_client, app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app_state = window.state::<AppState>();
                let minimize = *app_state.minimize_to_tray.lock().unwrap();
                if minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_watching,
            stop_watching,
            get_process_status,
            get_default_save_path_cmd,
            select_save_folder,
            get_offline_queue,
            clear_offline_queue,
            retry_offline_queue_now,
            get_telemetry_history,
            get_autostart_status,
            set_autostart,
            check_for_updates_cmd,
            get_minimize_to_tray_status,
            set_minimize_to_tray,
            get_app_version,
            update_tray_tooltip
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

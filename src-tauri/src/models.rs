use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ProcessStatusPayload {
    pub is_running: bool,
    pub has_debug_flag: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TelemetryPayload {
    pub file_name: String,
    pub file_hash: String,
    pub status: String,
    pub verified: bool,
    pub has_debug_flag: bool,
    pub timestamp: String,
}

pub struct AppState {
    pub watcher_tx: Mutex<Option<mpsc::Sender<()>>>,
    pub client: Client,
    pub minimize_to_tray: Mutex<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            watcher_tx: Mutex::new(None),
            client: Client::builder()
                .pool_idle_timeout(Some(Duration::from_secs(90)))
                .tcp_keepalive(Some(Duration::from_secs(30)))
                .build()
                .unwrap_or_else(|_| Client::new()),
            minimize_to_tray: Mutex::new(true),
        }
    }
}

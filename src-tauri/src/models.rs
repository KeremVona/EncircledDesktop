use serde::{Deserialize, Serialize};
use std::sync::Mutex;
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

#[derive(Default)]
pub struct AppState {
    pub watcher_tx: Mutex<Option<mpsc::Sender<()>>>,
}

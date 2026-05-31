pub mod bbs;
pub mod cli;
pub mod config;
pub mod peercast;
pub mod player;

/// Cheap health-check command used during early UI development.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

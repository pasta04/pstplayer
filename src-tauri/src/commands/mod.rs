pub mod bbs;
pub mod config;
pub mod peercast;

/// Cheap health-check command used during early UI development.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

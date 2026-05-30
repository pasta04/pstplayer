// PSTPlayer backend entry point.
//
// Module layout follows docs/architecture.md.

pub mod bbs;
pub mod cli;
pub mod commands;
pub mod config;
pub mod peercast;
pub mod player;
pub mod single_instance;
pub mod util;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::peercast::resolve_stream_url,
            commands::peercast::endpoint_for_url,
            commands::peercast::fetch_channel_info,
            commands::peercast::fetch_channel_status,
            commands::peercast::bump_channel,
            commands::peercast::stop_channel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// PSTPlayer Tauri shell. Pure logic lives in the `pst-core` crate
// (see crates/pst-core/) so it can be reused by future server / web builds.

pub mod commands;
pub mod player;

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
            commands::config::get_config,
            commands::config::set_config,
            commands::config::config_file_path,
            commands::bbs::list_threads,
            commands::bbs::fetch_thread,
            commands::bbs::post_to_thread,
            commands::bbs::classify_board,
            commands::bbs::sanitize_html,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

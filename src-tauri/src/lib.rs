// PSTPlayer Tauri shell. Pure logic lives in the `pst-core` crate
// (see crates/pst-core/) so it can be reused by future server / web builds.

pub mod commands;
pub mod player;

use player::engine::PlayerEngine;
use pst_core::cli;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli_args = cli::parse(&std::env::args().skip(1).collect::<Vec<_>>());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            app.handle().manage(cli_args);
            // Initialise libmpv once at startup. If this fails (e.g.
            // libmpv.so missing) we report and continue without the
            // player rather than aborting the whole app.
            match PlayerEngine::new() {
                Ok(engine) => {
                    app.handle().manage(engine);
                }
                Err(e) => {
                    eprintln!("warning: failed to initialise libmpv: {e}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::cli::get_cli_args,
            commands::cli::resolve_default_endpoint,
            commands::peercast::resolve_stream_url,
            commands::peercast::endpoint_for_url,
            commands::peercast::fetch_channel_info,
            commands::peercast::fetch_channel_status,
            commands::peercast::bump_channel,
            commands::peercast::stop_channel,
            commands::peercast::fetch_yp_index,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::config_file_path,
            commands::config::push_history,
            commands::config::get_history,
            commands::config::clear_history,
            commands::config::save_window_geometry,
            commands::config::push_recent_host,
            commands::bbs::list_threads,
            commands::bbs::fetch_thread,
            commands::bbs::post_to_thread,
            commands::bbs::classify_board,
            commands::bbs::sanitize_html,
            commands::player::player_load,
            commands::player::player_stop,
            commands::player::player_set_pause,
            commands::player::player_set_volume,
            commands::player::player_set_mute,
            commands::player::player_status,
            commands::player::player_attach,
            commands::player::player_snapshot,
            commands::player::snapshot_target_dir,
            commands::player::player_set_aspect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

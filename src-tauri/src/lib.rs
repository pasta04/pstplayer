// PSTPlayer Tauri shell. Pure logic lives in the `pst-core` crate
// (see crates/pst-core/) so it can be reused by future server / web builds.

pub mod channel_polling;
pub mod commands;
pub mod player;

use std::path::Path;

use channel_polling::ChannelPolling;
use player::engine::PlayerEngine;
use pst_core::cli::{self, CliArgs};
use pst_core::single_instance::{self, AcquireResult, LockHandle};
use tauri::{AppHandle, Manager, Runtime};

/// URL 引数付き起動なら、その `channel_id` の single_instance ロックを
/// 取りに行く。既に他プロセスが視聴している場合は focus 要求を送って
/// `None` を返し (= 上位の `run` は即終了)、自分が取れたら
/// `Some(LockHandle)` を返す。
fn maybe_acquire_lock(cli: &CliArgs) -> AcquireOutcome {
    let Some(url) = cli.url.as_ref() else {
        return AcquireOutcome::Skip;
    };
    let parsed = match pst_core::peercast::url::parse(url) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("URL 解析失敗 (single_instance skip): {e}");
            return AcquireOutcome::Skip;
        }
    };
    match single_instance::acquire(parsed.channel_id.as_str()) {
        Ok(AcquireResult::Owned(h)) => AcquireOutcome::Owned(h),
        Ok(AcquireResult::Conflict(info)) => {
            // 既存プロセスを前面化して自分は終了。
            if let Err(e) = single_instance::request_focus(info.ipc_addr) {
                eprintln!("既存ウィンドウへのフォーカス要求が失敗: {e}");
            }
            AcquireOutcome::ConflictResolved
        }
        Err(e) => {
            // ロックディレクトリ作成失敗等。継続して起動する。
            eprintln!("single_instance acquire failed (continuing): {e}");
            AcquireOutcome::Skip
        }
    }
}

enum AcquireOutcome {
    /// 既存ウィンドウへフォーカスを送った。自分は何もせず終了して良い。
    ConflictResolved,
    /// ロックを取った。`LockHandle` の lifetime をアプリ全体と一致させる。
    Owned(LockHandle),
    /// URL 引数なし、もしくは解析失敗。single_instance は使わず通常起動。
    Skip,
}

/// `acquire` で受け取ったリスナーを別スレッドで回し、`focus` / `close`
/// 要求に応える。`LockHandle` は app state に持たせて drop 時に lock
/// ファイル削除されるようにする。
fn start_focus_listener<R: Runtime>(mut handle: LockHandle, app: AppHandle<R>) -> LockHandle {
    if let Some(listener) = handle.take_listener() {
        let app_focus = app.clone();
        let app_close = app.clone();
        let app_state = app.clone();
        let app_start = app.clone();
        let app_stop = app;
        std::thread::spawn(move || {
            single_instance::serve(
                listener,
                move || {
                    if let Some(win) = app_focus.get_webview_window("main") {
                        let _ = win.unminimize();
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                },
                move || app_close.exit(0),
                move || {
                    app_state.try_state::<PlayerEngine>().and_then(|e| e.record_path()).is_some()
                },
                move || {
                    // ハブからの録画開始要求。CLI 引数の channel_name を
                    // ファイル名に使い、設定の recording_dir に保存する。
                    // libmpv の stream-record property を設定するだけなので
                    // 既に録画中の時は no-op (上書きはしない)。
                    // 各失敗パスで stderr ログを残す (silent fail で
                    // ハブの ● 録画中 badge が付かない → 原因不明
                    // という UX を避ける)。
                    let Some(engine) = app_start.try_state::<PlayerEngine>() else {
                        eprintln!(
                            "start_record IPC: PlayerEngine not initialised (libmpv 未起動?)"
                        );
                        return;
                    };
                    if engine.record_path().is_some() {
                        return; // 既に録画中
                    }
                    let cli = app_start.try_state::<CliArgs>();
                    let channel_name = cli.and_then(|c| c.channel_name.clone()).unwrap_or_default();
                    let cfg = match pst_core::config::load() {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("start_record IPC: config load failed: {e}");
                            return;
                        }
                    };
                    let exe_dir = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(Path::to_path_buf));
                    let resolved =
                        pst_core::snapshot::resolve_record_dir(&cfg.player, exe_dir.as_deref());
                    if let Err(e) = std::fs::create_dir_all(&resolved.dir) {
                        eprintln!(
                            "start_record IPC: recording dir create failed ({}): {e}",
                            resolved.dir.display()
                        );
                        return;
                    }
                    let raw_ext = cfg.player.recording_ext.trim().trim_start_matches('.');
                    let ext = if raw_ext.is_empty() { "flv" } else { raw_ext };
                    let name = pst_core::snapshot::make_filename(&channel_name, ext);
                    let full = resolved.dir.join(&name);
                    if let Err(e) = engine.start_record(&full.to_string_lossy()) {
                        eprintln!("start_record IPC: libmpv start_record failed: {e}");
                    }
                },
                move || {
                    // ハブ側からの録画停止要求。
                    if let Some(e) = app_stop.try_state::<PlayerEngine>() {
                        let _ = e.stop_record();
                    }
                },
            );
        });
    }
    handle
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cli_args = cli::parse(&std::env::args().skip(1).collect::<Vec<_>>());

    // channel_id 単位の single_instance チェック。URL 起動 + 既存プロセスが
    // 同 ch を視聴中の場合は何もせず終了 (フォーカス要求は送る)。
    let lock_handle = match maybe_acquire_lock(&cli_args) {
        AcquireOutcome::ConflictResolved => return,
        AcquireOutcome::Owned(h) => Some(h),
        AcquireOutcome::Skip => None,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            app.handle().manage(cli_args);
            app.handle().manage(ChannelPolling::new());
            app.handle().manage(player::embed::VideoEmbed::new());
            // Initialise libmpv once at startup. If this fails (e.g.
            // libmpv.so missing) we report and continue without the
            // player rather than aborting the whole app.
            match PlayerEngine::new() {
                Ok(engine) => {
                    // 起動時の config から auto_reconnect の初期値を反映
                    // してから event loop を起動する。設定読み込み失敗時
                    // (新規環境等) は既定 false = 観察モード扱い。
                    if let Ok(cfg) = pst_core::config::load() {
                        engine.set_auto_reconnect(cfg.player.auto_reconnect);
                    }
                    engine.attach_event_loop(app.handle().clone());
                    app.handle().manage(engine);
                }
                Err(e) => {
                    eprintln!("warning: failed to initialise libmpv: {e}");
                }
            }
            // single_instance のロックを持っているなら focus 受け取り用
            // listener を別スレッドで起動し、handle を app state に保持
            // する (drop で lock ファイル削除)。
            if let Some(handle) = lock_handle {
                let h = start_focus_listener(handle, app.handle().clone());
                app.handle().manage(h);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::cli::get_cli_args,
            commands::cli::resolve_default_endpoint,
            commands::peercast::resolve_stream_url,
            commands::peercast::endpoint_for_url,
            commands::peercast::peercast_ping,
            commands::peercast::fetch_channel_info,
            commands::peercast::fetch_channel_status,
            commands::peercast::bump_channel,
            commands::peercast::stop_channel,
            commands::peercast::start_channel_polling,
            commands::peercast::stop_channel_polling,
            commands::peercast::fetch_yp_index,
            commands::peercast::fetch_yp_sources,
            commands::peercast::spawn_viewer,
            commands::peercast::list_active_viewers,
            commands::peercast::list_recording_viewers,
            commands::peercast::close_viewer,
            commands::peercast::close_all_viewers,
            commands::peercast::stop_viewer_recording,
            commands::peercast::start_viewer_recording,
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
            commands::player::player_set_video_rect,
            commands::player::player_load,
            commands::player::player_stop,
            commands::player::player_set_pause,
            commands::player::player_set_volume,
            commands::player::player_set_mute,
            commands::player::player_status,
            commands::player::player_attach,
            commands::player::player_snapshot,
            commands::player::snapshot_target_dir,
            commands::player::player_record_start,
            commands::player::player_record_stop,
            commands::player::player_record_path,
            commands::player::recording_target_dir,
            commands::player::player_set_aspect,
            commands::player::player_set_auto_reconnect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

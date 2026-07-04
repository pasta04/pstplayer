// PSTPlayer Tauri shell. Pure logic lives in the `pst-core` crate
// (see crates/pst-core/) so it can be reused by future server / web builds.

pub mod channel_polling;
pub mod commands;
pub mod embedded_server;
pub mod player;

use std::path::Path;

use channel_polling::ChannelPolling;
use player::engine::PlayerEngine;
use pst_core::cli::{self, CliArgs};
use pst_core::single_instance::{self, AcquireResult, LockHandle};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// 動画ウィンドウのマウス操作 (mpv 描画窓が食う分) をフロントへ転送する
/// ときのイベントペイロード。x/y は wid クライアント座標、delta はホイール量。
#[derive(Clone, serde::Serialize)]
struct PlayerInputPayload {
    x: i32,
    y: i32,
    delta: i32,
}

/// URL 引数付き起動なら、その `channel_id` の single_instance ロックを
/// 取りに行く。既に他プロセスが視聴している場合は focus 要求を送って
/// `None` を返し (= 上位の `run` は即終了)、自分が取れたら
/// `Some(LockHandle)` を返す。
/// 閉じシーケンスの実機デバッグ用ログ (一時的な計測コード)。exe と同じ
/// ディレクトリの close-debug.log に追記する。プロセス残留バグの
/// 原因特定後に削除する。
fn close_debug_log(msg: &str) {
    use std::io::Write;
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(dir) = exe.parent() else { return };
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("close-debug.log"))
    else {
        return;
    };
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = writeln!(f, "[{ms}] pid={} {msg}", std::process::id());
}

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
            // `--record-on-start` 付き (= ハブの「視聴+録画」) で起動された
            // のに既存ウィンドウがあった場合、focus だけだと録画指示が失わ
            // れる。既存ウィンドウへ録画開始 IPC も送る (D3)。
            if cli.record_on_start {
                if let Err(e) = single_instance::request_start_recording(info.ipc_addr) {
                    eprintln!("既存ウィンドウへの録画開始要求が失敗: {e}");
                }
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
                move || {
                    close_debug_log("ipc close: app.exit(0)");
                    app_close.exit(0);
                    close_debug_log("ipc close: app.exit returned");
                },
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

/// メインウィンドウを復帰させる (トレイアイコン / トレイメニューから)。
fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 最小化トレイ用のトレイアイコンを生成する。左クリック / メニュー「表示」で
/// 復帰、メニュー「終了」でアプリ終了。`window.minimize_to_tray` が ON の
/// ハブ起動時のみ呼ぶ (フロントの最小化→hide と起動時の同じ値に従わせて
/// 「トレイ無しでウィンドウが消える」事故を避ける)。
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let Some(icon) = app.default_window_icon().cloned() else {
        return Ok(()); // アイコンが無ければトレイは出さない
    };
    let show = MenuItemBuilder::with_id("tray_show", "表示").build(app)?;
    let quit = MenuItemBuilder::with_id("tray_quit", "終了").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("PSTPlayer")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_main(app),
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Windows: WebView2 (Chromium) は前面に居ないウィンドウのタイマー /
    // レンダラを throttling し、occlusion 判定で renderer を凍結することが
    // ある。本アプリはハブ + 視聴 (複数) を併用するので、背面ウィンドウでも
    // BBS 自動更新やプレイヤー状態ポーリングを止めないよう、該当フラグを
    // 無効化する。WebView2 環境作成より前 (= ウィンドウ生成より前) に設定
    // する必要があるので run() 冒頭で行う。
    #[cfg(target_os = "windows")]
    {
        const NO_THROTTLE: &str = "--disable-background-timer-throttling \
             --disable-renderer-backgrounding --disable-backgrounding-occluded-windows \
             --disable-features=CalculateNativeWinOcclusion";
        match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
            Ok(existing) if existing.contains("--disable-background-timer-throttling") => {}
            Ok(existing) => std::env::set_var(
                "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
                format!("{existing} {NO_THROTTLE}"),
            ),
            Err(_) => std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", NO_THROTTLE),
        }
    }

    let cli_args = cli::parse(&std::env::args().skip(1).collect::<Vec<_>>());

    // channel_id 単位の single_instance チェック。URL 起動 + 既存プロセスが
    // 同 ch を視聴中の場合は何もせず終了 (フォーカス要求は送る)。
    let lock_handle = match maybe_acquire_lock(&cli_args) {
        AcquireOutcome::ConflictResolved => return,
        AcquireOutcome::Owned(h) => Some(h),
        AcquireOutcome::Skip => None,
    };

    // on_window_event の閉じウォッチドッグ用 (viewer = URL 起動のみ)。
    // ハブは録画中の閉じ確認 (JS onCloseRequested の prevent) があるため
    // CloseRequested 起点の強制終了を張ってはいけない。
    let is_viewer = cli_args.url.is_some();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let start_hidden = cli_args.hidden;
            // ハブ起動 (URL 引数なし) のときだけ内蔵録画サーバを動かす
            // (viewer は別プロセスなので起動させない)。
            let is_hub = cli_args.url.is_none();
            app.handle().manage(cli_args);
            app.handle().manage(ChannelPolling::new());
            app.handle().manage(player::embed::VideoEmbed::new());
            // 内蔵録画サーバの state は常に manage する (viewer プロセスや
            // bind 失敗時は None のまま)。get_config / set_config が参照する。
            app.handle().manage(embedded_server::EmbeddedServerState::default());
            // 動画ウィンドウのマウス操作 (mpv 描画窓が食う右クリック/ホイール/
            // クリック/ダブルクリック) を embed の WNDPROC/サブクラスから受けて
            // `player:*` イベントとしてフロントへ転送する。
            {
                let app_emit = app.handle().clone();
                player::embed::set_input_emitter(Box::new(move |event, x, y, delta| {
                    let _ = app_emit
                        .emit(&format!("player:{event}"), PlayerInputPayload { x, y, delta });
                }));
            }
            // single_instance のロックを持っているなら focus / IPC 受け取り
            // listener を先に起動する。libmpv 初期化より前に serve を回す
            // ことで、起動直後でもハブの list_active_viewers の probe に即
            // 応答でき「視聴中」バッジ反映のラグを減らす (E1)。serve の
            // コールバックは PlayerEngine を try_state 参照するため、まだ
            // 未 manage の一瞬は no-op になるだけで安全。
            if let Some(handle) = lock_handle {
                let h = start_focus_listener(handle, app.handle().clone());
                app.handle().manage(h);
            }
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
            // 「録画のみ」(--hidden) で起動された場合はウィンドウを非表示に
            // する (D2)。録画 (stream-record) はウィンドウ表示に依存しないので
            // 非表示のまま録り続けられる。停止はハブの「視聴中」操作から行う。
            if start_hidden {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }
            // ゼロ設定バックグラウンド録画: ハブ起動時に内蔵録画サーバを
            // 立ち上げる (外部 pst-server 設定時は内部で skip)。録画は HTTP で
            // 配信を受信しファイル保存するだけで、ウィンドウも再生も無い。
            if is_hub {
                embedded_server::start_if_enabled(app.handle().clone());
                // 最小化トレイ設定が ON のときだけ復帰用トレイを生成する
                // (起動時固定。変更は次回起動から反映)。
                let tray_on =
                    pst_core::config::load().map(|c| c.window.minimize_to_tray).unwrap_or(false);
                if tray_on {
                    if let Err(e) = setup_tray(app) {
                        eprintln!("tray setup failed: {e}");
                    }
                }
            }
            Ok(())
        })
        .on_window_event(move |window, event| {
            // 視聴ウィンドウを閉じる前に録画を確実にファイナライズする。
            // stream-record を空に設定すると libmpv が出力ファイルを正しく
            // 閉じる (D4)。録画していなければ no-op。メインウィンドウのみ対象。
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    if let Some(engine) = window.app_handle().try_state::<PlayerEngine>() {
                        // 録画をファイナライズしつつ、保留中の自動再接続も止めてから
                        // 閉じる (user_stop / generation で reconnect ワーカーを
                        // キャンセルし、閉じ際の再接続の残り火を防ぐ)。
                        let _ = engine.stop_record();
                        let _ = engine.stop();
                    }
                    // 終了ウォッチドッグ (viewer のみ)。実機では × / Alt+X で
                    // ウィンドウ消滅後も Destroyed イベントが届かず (破棄中に
                    // イベントループが mpv 子ウィンドウ / WebView2 の teardown
                    // で停止するとみられる)、プロセスが数分残留して
                    // single-instance lock を握り続けた。viewer は close を
                    // prevent しないので、CloseRequested = 確実に閉じる、で
                    // 猶予後に段階的に強制終了する。自然終了が先なら no-op。
                    close_debug_log("close-requested (main)");
                    if is_viewer {
                        let app = window.app_handle().clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            close_debug_log("watchdog: app.exit(0)");
                            app.exit(0);
                            close_debug_log("watchdog: app.exit returned");
                        });
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(8));
                            close_debug_log("watchdog: process::exit(0)");
                            std::process::exit(0);
                        });
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_secs(11));
                            // process::exit が DLL detach 等でデッドロックした
                            // 場合の最終手段。abort は detach をスキップして
                            // 即プロセスを落とす。
                            close_debug_log("watchdog: abort()");
                            std::process::abort();
                        });
                    }
                }
            }
            // main 破棄 = アプリ終了を明示する。Tauri の「全ウィンドウ破棄で
            // 自然終了」に任せると、libmpv / WebView2 の終了処理に引きずられて
            // プロセス終了が数分単位で遅延し、windowless のゾンビが
            // single-instance lock を IPC 応答つきで握り続けてハブの「視聴中」
            // カウントが固着した (実機 QA)。IPC close (ハブの全閉じ) の
            // app.exit(0) は別スレッドから呼ばれて即終了することを確認済み。
            // イベントディスパッチ中の同期 exit は破棄処理と競合して効かない
            // ため同様に別スレッドへ逃がし、クリーンアップが固まった場合の
            // 保険として一定時間後に process::exit で強制終了する。settings
            // 等のサブウィンドウは対象外 (main が生きている限り続行)。
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    close_debug_log("destroyed (main)");
                    let app = window.app_handle().clone();
                    std::thread::spawn(move || {
                        app.exit(0);
                        // exit(0) がプラグイン / WebView2 のクリーンアップで
                        // 返ってこない・進まない場合の最終手段。録画は
                        // CloseRequested 時点で finalize 済みなので安全。
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        std::process::exit(0);
                    });
                }
            }
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
            commands::bbs::fetch_board_setting,
            commands::bbs::board_url_of,
            commands::bbs::thread_url_of,
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
            commands::server::server_record_start,
            commands::server::server_record_stop,
            commands::server::server_record_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

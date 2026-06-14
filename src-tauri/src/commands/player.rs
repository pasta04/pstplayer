use crate::player::{embed::VideoEmbed, engine::PlayerEngine, window as player_window};
use pst_core::config;
use pst_core::snapshot;
use pst_core::util::errors::{AppError, IpcError};
use std::env;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime, State};

#[tauri::command]
pub fn player_load(url: String, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    // load 時に config の auto_reconnect 設定を engine state に反映
    // しておく (= 設定変更を毎回反映)。`set_config` 後に明示同期する
    // のは `player_set_auto_reconnect` 経由。
    if let Ok(cfg) = config::load() {
        engine.set_auto_reconnect(cfg.player.auto_reconnect);
    }
    engine.load(&url)?;
    // 開発中 (QA スクリプト起動) はうるさいので配信音声をミュートする。
    // 環境変数 PST_DEV_MUTE が立っているときだけ効くので、通常起動や
    // リリースビルドには一切影響しない (run-viewer.ps1 が env を立てる)。
    if env::var("PST_DEV_MUTE").is_ok_and(|v| !v.is_empty() && v != "0") {
        let _ = engine.set_mute(true);
    }
    Ok(())
}

/// 設定ダイアログでユーザが auto_reconnect の ON/OFF を変えた直後に
/// 呼ぶ。次回 load() を待たずに反映される。
#[tauri::command]
pub fn player_set_auto_reconnect(enabled: bool, engine: State<'_, PlayerEngine>) {
    engine.set_auto_reconnect(enabled);
}

#[tauri::command]
pub fn player_stop(engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.stop().map_err(Into::into)
}

#[tauri::command]
pub fn player_set_pause(pause: bool, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.set_pause(pause).map_err(Into::into)
}

#[tauri::command]
pub fn player_set_volume(percent: u8, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.set_volume(percent).map_err(Into::into)
}

#[tauri::command]
pub fn player_set_mute(mute: bool, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.set_mute(mute).map_err(Into::into)
}

#[derive(serde::Serialize)]
pub struct PlayerStatus {
    pub fps: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub time_pos: Option<f64>,
    pub paused: Option<bool>,
}

/// Attach the libmpv player to a dedicated child window that only covers
/// the player area, so the video does not paint over the BBS pane / bars.
/// On Windows a `STATIC` child HWND is created under the main window and
/// handed to mpv as `wid`; the frontend keeps it positioned via
/// [`player_set_video_rect`]. On other platforms this attaches directly to
/// the main window (the old behaviour). Idempotent.
#[tauri::command]
pub fn player_attach<R: Runtime>(
    window_label: String,
    app: AppHandle<R>,
    engine: State<'_, PlayerEngine>,
    embed: State<'_, VideoEmbed>,
) -> Result<(), IpcError> {
    let window = app
        .get_webview_window(&window_label)
        .ok_or_else(|| AppError::InvalidUrl(format!("no such window: {window_label}")))?;
    let parent_wid = player_window::main_window_wid(&window)?;
    let child_wid = embed
        .ensure_child(parent_wid as isize)
        .map_err(|e| AppError::Network(format!("create video child window: {e}")))?;
    let handle = engine.handle();
    player_window::set_wid(&handle, child_wid as i64).map_err(Into::into)
}

/// プレイヤー領域 (`.player-canvas`) の物理ピクセル矩形を受け取り、
/// libmpv 描画用の子ウィンドウをその位置 / サイズに合わせる。フロントが
/// マウント時とリサイズ / レイアウト変化のたびに呼ぶ。Windows 以外は no-op。
#[tauri::command]
pub fn player_set_video_rect(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    embed: State<'_, VideoEmbed>,
) -> Result<(), IpcError> {
    embed
        .set_rect(x, y, width, height)
        .map_err(|e| AppError::Network(format!("set video rect: {e}")).into())
}

/// Take a snapshot of the current video frame, saving it under the
/// configured snapshot directory (or its fallback if the configured
/// one is unwritable). Returns the absolute path of the saved file.
#[tauri::command]
pub fn player_snapshot(
    channel_name: Option<String>,
    engine: State<'_, PlayerEngine>,
) -> Result<String, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    let exe_dir = exe_dir();
    let resolved = snapshot::resolve_dir(&cfg.player, exe_dir.as_deref());
    if let Err(e) = fs::create_dir_all(&resolved.dir) {
        return Err(AppError::Decode(format!(
            "cannot create snapshot dir {}: {e}",
            resolved.dir.display()
        ))
        .into());
    }
    let ext = if cfg.player.snapshot_format.eq_ignore_ascii_case("jpeg")
        || cfg.player.snapshot_format.eq_ignore_ascii_case("jpg")
    {
        "jpg"
    } else {
        "png"
    };
    let name = snapshot::make_filename(channel_name.as_deref().unwrap_or(""), ext);
    // 同秒に F2 連打されてもファイル上書きしないよう `_2`, `_3`, ... を
    // 付ける。F2 自体は通知側で 1 秒 debounce されるが、連打を受け付け
    // ないとしても snapshot を撮りたい全てのケース (右クリック等) で
    // 同秒衝突は起き得るので保険として入れる。
    let full = snapshot::unique_path(&resolved.dir, &name, ext);
    let full_str = full.to_string_lossy().into_owned();
    engine.screenshot_to_file(&full_str, "subtitles").map_err(IpcError::from)?;
    Ok(full_str)
}

/// 録画を開始する。`channel_name` は出力ファイル名の組み立てに使う。
/// 戻り値は保存先の絶対パス (ユーザーに通知する用)。
#[tauri::command]
pub fn player_record_start(
    channel_name: Option<String>,
    engine: State<'_, PlayerEngine>,
) -> Result<String, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    let exe_dir = exe_dir();
    let resolved = snapshot::resolve_record_dir(&cfg.player, exe_dir.as_deref());
    fs::create_dir_all(&resolved.dir).map_err(|e| {
        AppError::Decode(format!("cannot create recording dir {}: {e}", resolved.dir.display()))
    })?;
    // 拡張子: 設定で明示があればそれ、なければ FLV (PeerCast の主流)。
    // libmpv はファイル名の拡張子からコンテナを推測するため、ここで
    // 何かを決めないと "拡張子なし" のファイルが出来てしまう。
    let raw_ext = cfg.player.recording_ext.trim().trim_start_matches('.');
    let ext = if raw_ext.is_empty() { "flv" } else { raw_ext };
    let name = snapshot::make_filename(channel_name.as_deref().unwrap_or(""), ext);
    // 同秒に録画停止 → 即再開した時に旧ファイルを上書きしないよう
    // `_2`, `_3`, ... を付ける (pst-server::recording と同じパターン)。
    let full = snapshot::unique_path(&resolved.dir, &name, ext);
    let full_str = full.to_string_lossy().into_owned();
    engine.start_record(&full_str).map_err(IpcError::from)?;
    Ok(full_str)
}

/// 録画を停止する。録画中でなければ何もしない (libmpv 仕様で
/// stream-record を空にするだけ)。
#[tauri::command]
pub fn player_record_stop(engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.stop_record().map_err(Into::into)
}

/// 録画中のパスを返す。録画していなければ None。
#[tauri::command]
pub fn player_record_path(engine: State<'_, PlayerEngine>) -> Option<String> {
    engine.record_path()
}

/// 録画ファイルの保存先 (現在の設定で解決した結果)。設定ダイアログの
/// プレビュー表示用。
#[tauri::command]
pub fn recording_target_dir() -> Result<String, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    let resolved = snapshot::resolve_record_dir(&cfg.player, exe_dir().as_deref());
    Ok(resolved.dir.to_string_lossy().into_owned())
}

/// Set the video aspect override. 0.0 = auto, -1.0 = stretch.
#[tauri::command]
pub fn player_set_aspect(aspect: f64, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.set_aspect(aspect).map_err(IpcError::from)
}

/// Where libmpv will write snapshots by default, given the current
/// config. Returned for previewing in the settings UI.
#[tauri::command]
pub fn snapshot_target_dir() -> Result<String, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    let resolved = snapshot::resolve_dir(&cfg.player, exe_dir().as_deref());
    Ok(resolved.dir.to_string_lossy().into_owned())
}

fn exe_dir() -> Option<PathBuf> {
    env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

#[tauri::command]
pub fn player_status(engine: State<'_, PlayerEngine>) -> PlayerStatus {
    PlayerStatus {
        fps: engine.get_property::<f64>("estimated-vf-fps"),
        width: engine.get_property::<i64>("video-params/w"),
        height: engine.get_property::<i64>("video-params/h"),
        time_pos: engine.get_property::<f64>("time-pos"),
        paused: engine.get_property::<bool>("pause"),
    }
}

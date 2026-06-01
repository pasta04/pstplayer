use crate::channel_polling::ChannelPolling;
use pst_core::config;
use pst_core::peercast::{
    client, jsonrpc,
    types::{ChannelInfo, ChannelStatus, PeerCastEndpoint},
    yp::{self, YpEntry},
};
use pst_core::single_instance;
use pst_core::util::errors::{AppError, IpcError};
use serde::Serialize;
use std::process::Command;
use tauri::{AppHandle, Runtime, State};

/// Resolve a user-supplied PeerCast URL into the concrete stream URL
/// that the embedded media player should load.
#[tauri::command]
pub async fn resolve_stream_url(url: String) -> Result<String, IpcError> {
    client::resolve_stream_url(&url).await.map_err(Into::into)
}

/// Extract the host/port that a PeerCast URL points to, with the
/// user's saved Basic-auth credentials stamped on so that subsequent
/// JSON-RPC / legacy-admin calls succeed against a protected endpoint.
#[tauri::command]
pub fn endpoint_for_url(url: String) -> Result<PeerCastEndpoint, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    client::endpoint_for_with_auth(&url, &cfg.peercast).map_err(Into::into)
}

/// 起動時の疎通チェック。config の peercast endpoint に対して
/// `getVersionInfo` JSON-RPC を投げ、応答があるかどうかだけを返す。
/// 失敗時のエラーコードでフロントが「PeerCast 未起動」と判断できる。
#[tauri::command]
pub async fn peercast_ping() -> Result<(), IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    let endpoint = PeerCastEndpoint {
        host: cfg.peercast.host.clone(),
        port: cfg.peercast.port,
        auth: client::auth_from_cfg(&cfg.peercast),
    };
    if jsonrpc::get_version_info(&endpoint).await.is_ok() {
        return Ok(());
    }
    Err(AppError::PeerCastUnreachable(format!(
        "{}:{} に応答がありません",
        endpoint.host, endpoint.port
    ))
    .into())
}

#[tauri::command]
pub async fn fetch_channel_info(
    endpoint: PeerCastEndpoint,
    channel_id: String,
) -> Result<ChannelInfo, IpcError> {
    client::fetch_info(&endpoint, &channel_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn fetch_channel_status(
    endpoint: PeerCastEndpoint,
    channel_id: String,
) -> Result<ChannelStatus, IpcError> {
    client::fetch_status(&endpoint, &channel_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn bump_channel(endpoint: PeerCastEndpoint, channel_id: String) -> Result<(), IpcError> {
    client::bump(&endpoint, &channel_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn stop_channel(endpoint: PeerCastEndpoint, channel_id: String) -> Result<(), IpcError> {
    client::stop(&endpoint, &channel_id).await.map_err(Into::into)
}

/// バックエンド側のチャンネル状態ポーラーを起動。指定 endpoint /
/// channel_id に対して 5 秒間隔で fetch_status を呼び、結果を
/// `channel:status` event でフロントに emit する。既に動いている
/// タスクは abort してから差し替える。
#[tauri::command]
pub fn start_channel_polling<R: Runtime>(
    endpoint: PeerCastEndpoint,
    channel_id: String,
    app: AppHandle<R>,
    polling: State<'_, ChannelPolling>,
) {
    polling.start(app, endpoint, channel_id);
}

/// バックエンドのポーラーを停止。視聴を停止した時 / アプリ終了時に
/// フロントから呼ぶ。
#[tauri::command]
pub fn stop_channel_polling(polling: State<'_, ChannelPolling>) {
    polling.stop();
}

/// Fetch the configured YP `index.txt` and return parsed entries.
/// `override_url` を渡すと config の YP 設定を無視して単独 URL を fetch
/// する (手入力 / プレビュー用)。それ以外は `[[yp.sources]]` (旧
/// `peercast.yp_url` からのマイグレ込み) を **全部並行 fetch** し、
/// `channel_id` 重複は前者のソース優先で 1 つに纏める。
#[tauri::command]
pub async fn fetch_yp_index(override_url: Option<String>) -> Result<Vec<YpEntry>, IpcError> {
    if let Some(url) = override_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return yp::fetch_index(url).await.map_err(Into::into);
    }
    let cfg = config::load().map_err(IpcError::from)?;
    let sources = cfg.effective_yp_sources();
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    Ok(yp::fetch_indexes(&sources).await.entries)
}

/// 全 YP ソースの fetch + 失敗一覧。フロントは `failures` を見て
/// ステータスバーに「YP X: 取得失敗」を出せる。
#[tauri::command]
pub async fn fetch_yp_sources() -> Result<yp::MultiFetchOutcome, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    let sources = cfg.effective_yp_sources();
    if sources.is_empty() {
        return Ok(yp::MultiFetchOutcome { entries: Vec::new(), failures: Vec::new() });
    }
    Ok(yp::fetch_indexes(&sources).await)
}

/// `spawn_viewer` の結果。フロント側で「新規ウィンドウが立ち上がった」
/// のか「既存ウィンドウにフォーカスが当たった」のかを判別できる。
#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpawnViewerOutcome {
    /// 既存ウィンドウにフォーカスを当てた。
    Focused,
    /// 新規プロセスを起動した。
    Spawned,
}

/// 現在「視聴中」(= single_instance ロックが生きている) チャンネル ID
/// の一覧を返す。ハブ画面の「視聴中」タブ用。stale lock は best-effort
/// でこの呼び出しで掃除される。
#[tauri::command]
pub fn list_active_viewers() -> Vec<String> {
    single_instance::list_active().into_iter().map(|i| i.channel_id).collect()
}

/// 現在録画中の channel_id 一覧を返す。各 active viewer の IPC に
/// `state\n` を並行に投げて録画中なら返答 `1\n` をもらう。失敗 /
/// not recording は無視。viewer の数が多くても直列の総当たり待ちで
/// UI が固まらないよう、スレッドで並行 query する。
#[tauri::command]
pub async fn list_recording_viewers() -> Vec<String> {
    let actives = single_instance::list_active();
    if actives.is_empty() {
        return Vec::new();
    }
    // tokio::task::spawn_blocking で TCP I/O (同期 API) を並行に。
    let mut handles = Vec::with_capacity(actives.len());
    for info in actives {
        handles.push(tokio::task::spawn_blocking(move || {
            match single_instance::query_recording(info.ipc_addr) {
                Ok(true) => Some(info.channel_id),
                _ => None,
            }
        }));
    }
    let mut out = Vec::new();
    for h in handles {
        if let Ok(Some(id)) = h.await {
            out.push(id);
        }
    }
    out
}

/// 指定 channel_id の視聴ウィンドウを閉じる。lock がなければ no-op。
/// 失敗してもエラーにはせず false を返す (already closed 等の race を想定)。
#[tauri::command]
pub fn close_viewer(channel_id: String) -> bool {
    let Some(info) = single_instance::read_existing(&channel_id) else {
        return false;
    };
    single_instance::request_close(info.ipc_addr).is_ok()
}

/// 全視聴ウィンドウを一括クローズ。閉じられた件数を返す。各 viewer
/// への IPC は並行送信 (直列で 1 つずつだと timeout が累積するため)。
#[tauri::command]
pub async fn close_all_viewers() -> usize {
    let actives = single_instance::list_active();
    if actives.is_empty() {
        return 0;
    }
    let mut handles = Vec::with_capacity(actives.len());
    for info in actives {
        handles.push(tokio::task::spawn_blocking(move || {
            single_instance::request_close(info.ipc_addr).is_ok()
        }));
    }
    let mut closed = 0;
    for h in handles {
        if let Ok(true) = h.await {
            closed += 1;
        }
    }
    closed
}

/// 指定 channel_id の視聴ウィンドウに録画停止を要求。lock がなければ
/// false。失敗時も false。録画していないチャンネルは no-op だが true。
#[tauri::command]
pub fn stop_viewer_recording(channel_id: String) -> bool {
    let Some(info) = single_instance::read_existing(&channel_id) else {
        return false;
    };
    single_instance::request_stop_recording(info.ipc_addr).is_ok()
}

/// YP / お気に入り行クリックから呼ばれる「視聴用 pstplayer プロセスを
/// 立ち上げる」コマンド。
///
/// 1. `channel_id` のロックを check して既存プロセスがあればフォーカス
///    要求を送って `Focused` を返す
/// 2. 無ければ `current_exe()` を URL 引数付きで `Command::spawn` し
///    `Spawned` を返す
///
/// URL は config の `peercast.host:port` から `/pls/{id}` を組み立てる
/// (YP の `tip` 直叩きはせず必ず自分の PeerCast にリレー要求する)。
/// `record` = true で `--record-on-start` を追加 (= viewer 側で
/// favorites の auto_record と独立に強制録画開始)。
#[tauri::command]
pub fn spawn_viewer(
    channel_id: String,
    record: Option<bool>,
) -> Result<SpawnViewerOutcome, IpcError> {
    // 既存ロックがあれば focus 要求を送る。送信に成功した場合のみ
    // Focused を返す。失敗 (= viewer プロセスが probe → focus の間に
    // 落ちた、ファイアウォール等) なら新規 spawn にフォールバック。
    if let Some(info) = single_instance::read_existing(&channel_id) {
        if single_instance::request_focus(info.ipc_addr).is_ok() {
            return Ok(SpawnViewerOutcome::Focused);
        }
        // probe → focus の間に死んだ可能性 → spawn にフォールスルー。
        // 死んだ lock は次回 read_existing で除去される。
    }
    let cfg = config::load().map_err(IpcError::from)?;
    let url = format!("http://{}:{}/pls/{}", cfg.peercast.host, cfg.peercast.port, channel_id);
    let exe = std::env::current_exe().map_err(|e| {
        IpcError::from(AppError::Network(format!("current_exe を取得できません: {e}")))
    })?;
    let mut cmd = Command::new(exe);
    cmd.arg(&url);
    if record.unwrap_or(false) {
        cmd.arg("--record-on-start");
    }
    cmd.spawn().map_err(|e| {
        IpcError::from(AppError::Network(format!("別プロセスの pstplayer を起動できません: {e}")))
    })?;
    Ok(SpawnViewerOutcome::Spawned)
}

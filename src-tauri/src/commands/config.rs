use crate::embedded_server::{exe_dir, map_config, EmbeddedServerState};
use pst_core::config::schema::{HistoryEntry, MAX_HISTORY, MAX_RECENT_HOSTS};
use pst_core::config::{self, Config};
use pst_core::util::errors::IpcError;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

#[tauri::command]
pub fn get_config(state: State<'_, EmbeddedServerState>) -> Result<Config, IpcError> {
    // 破損 TOML でアプリ起動を阻害しないよう load_or_default を使う。
    // バックアップは backend ログに出力するのみ (フロントへの通知は
    // 別途 config:corrupted event 等で連動させる余地あり)。
    let (mut cfg, bak) = config::load_or_default();
    if let Some(path) = bak {
        eprintln!("warning: config.toml was corrupt; backed up to {}", path.display());
    }
    // 内蔵録画サーバが起動していて、かつユーザーが外部 URL を設定していない
    // ときは、ハブの pst_server_url に内蔵サーバの URL を注入する。これで
    // 「ダブルクリック起動 → そのまま右クリック / 自動で録画」がゼロ設定で
    // 動く (フロント側は従来どおり pst_server_url を消費するだけ)。
    if cfg.hub.pst_server_url.trim().is_empty() {
        if let Ok(guard) = state.0.lock() {
            if let Some(server) = guard.as_ref() {
                cfg.hub.pst_server_url = server.base_url();
            }
        }
    }
    Ok(cfg)
}

#[tauri::command]
pub async fn set_config(
    config: Config,
    state: State<'_, EmbeddedServerState>,
) -> Result<(), IpcError> {
    let mut config = config;
    // 注入した内蔵サーバ URL を config.toml に焼き付けない (一致時は空へ戻す)。
    // 併せてホットスワップ用の cfg ハンドルと実ポートを取り出す。std Mutex を
    // await 跨ぎで保持しないよう、ここで clone してから guard を解放する。
    let (base_url, cfg_handle, port) = match state.0.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(s) => (Some(s.base_url()), Some(s.config_handle()), s.addr.port()),
            None => (None, None, 0),
        },
        Err(_) => (None, None, 0),
    };
    if let Some(url) = &base_url {
        if config.hub.pst_server_url == *url {
            config.hub.pst_server_url = String::new();
        }
    }
    config::save(&config).map_err(IpcError::from)?;
    // 稼働中の内蔵サーバへ設定変更 (お気に入り / YP / 録画先 / peercast) を
    // 即反映する。
    if let Some(handle) = cfg_handle {
        let mapped = map_config(&config, exe_dir().as_deref(), port);
        *handle.write().await = mapped;
    }
    Ok(())
}

#[tauri::command]
pub fn config_file_path() -> Result<String, IpcError> {
    config::config_path().map(|p| p.to_string_lossy().into_owned()).map_err(Into::into)
}

/// Append a viewing-history entry: deduplicate by URL, push to the
/// front (most recent), and cap the list at MAX_HISTORY.
#[tauri::command]
pub fn push_history(url: String, channel_name: String) -> Result<(), IpcError> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    config::update(|cfg| {
        cfg.history.recent.retain(|e| e.url != url);
        cfg.history.recent.insert(0, HistoryEntry { url, channel_name, last_opened_at: now });
        cfg.history.recent.truncate(MAX_HISTORY);
    })
    .map(|_| ())
    .map_err(Into::into)
}

#[tauri::command]
pub fn get_history() -> Result<Vec<HistoryEntry>, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    Ok(cfg.history.recent)
}

#[tauri::command]
pub fn clear_history() -> Result<(), IpcError> {
    config::update(|cfg| {
        cfg.history.recent.clear();
    })
    .map(|_| ())
    .map_err(Into::into)
}

/// Persist physical window geometry. Other `window.*` fields
/// (bbs_pane_ratio, always_on_top, etc.) are preserved.
#[tauri::command]
pub fn save_window_geometry(x: i32, y: i32, width: u32, height: u32) -> Result<(), IpcError> {
    config::update(|cfg| {
        cfg.window.x = Some(x);
        cfg.window.y = Some(y);
        cfg.window.width = Some(width);
        cfg.window.height = Some(height);
    })
    .map(|_| ())
    .map_err(Into::into)
}

/// Push a PeerCast host:port into the MRU list. Deduplicates by
/// canonical "host:port" form and caps the list at MAX_RECENT_HOSTS.
#[tauri::command]
pub fn push_recent_host(host: String, port: u16) -> Result<(), IpcError> {
    let entry = format!("{host}:{port}");
    if entry.trim().is_empty() {
        return Ok(());
    }
    config::update(|cfg| {
        cfg.peercast.recent_hosts.retain(|e| e != &entry);
        cfg.peercast.recent_hosts.insert(0, entry);
        cfg.peercast.recent_hosts.truncate(MAX_RECENT_HOSTS);
    })
    .map(|_| ())
    .map_err(Into::into)
}

use pst_core::config::schema::{HistoryEntry, MAX_HISTORY, MAX_RECENT_HOSTS};
use pst_core::config::{self, Config};
use pst_core::util::errors::IpcError;
use std::time::{SystemTime, UNIX_EPOCH};

#[tauri::command]
pub fn get_config() -> Result<Config, IpcError> {
    // 破損 TOML でアプリ起動を阻害しないよう load_or_default を使う。
    // バックアップは backend ログに出力するのみ (フロントへの通知は
    // 別途 config:corrupted event 等で連動させる余地あり)。
    let (cfg, bak) = config::load_or_default();
    if let Some(path) = bak {
        eprintln!("warning: config.toml was corrupt; backed up to {}", path.display());
    }
    Ok(cfg)
}

#[tauri::command]
pub fn set_config(config: Config) -> Result<(), IpcError> {
    config::save(&config).map_err(Into::into)
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

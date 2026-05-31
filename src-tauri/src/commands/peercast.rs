use pst_core::config;
use pst_core::peercast::{
    client,
    types::{ChannelInfo, ChannelStatus, PeerCastEndpoint},
    yp::{self, YpEntry},
};
use pst_core::util::errors::IpcError;

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

/// Fetch the configured YP `index.txt` and return parsed entries.
/// `override_url` を渡すと config の `peercast.yp_url` ではなくそれを
/// 使用 (将来の複数 YP 切替や手入力に備える)。
#[tauri::command]
pub async fn fetch_yp_index(override_url: Option<String>) -> Result<Vec<YpEntry>, IpcError> {
    let url = match override_url {
        Some(s) if !s.trim().is_empty() => s,
        _ => {
            let cfg = config::load().map_err(IpcError::from)?;
            cfg.peercast.yp_url
        }
    };
    yp::fetch_index(&url).await.map_err(Into::into)
}

use pst_core::peercast::{
    client,
    types::{ChannelInfo, ChannelStatus, PeerCastEndpoint},
};
use pst_core::util::errors::IpcError;

/// Resolve a user-supplied PeerCast URL into the concrete stream URL
/// that the embedded media player should load.
#[tauri::command]
pub async fn resolve_stream_url(url: String) -> Result<String, IpcError> {
    client::resolve_stream_url(&url).await.map_err(Into::into)
}

/// Extract the host/port that a PeerCast URL points to.
#[tauri::command]
pub fn endpoint_for_url(url: String) -> Result<PeerCastEndpoint, IpcError> {
    client::endpoint_for(&url).map_err(Into::into)
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

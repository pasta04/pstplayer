use crate::peercast::client;
use crate::util::errors::IpcError;

/// Resolve a user-supplied PeerCast URL into the concrete stream URL
/// that the embedded media player should load.
#[tauri::command]
pub async fn resolve_stream_url(url: String) -> Result<String, IpcError> {
    client::resolve_stream_url(&url).await.map_err(Into::into)
}

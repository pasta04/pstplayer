//! High-level PeerCast client. Resolves a user-supplied URL into a
//! concrete stream URL, and dispatches control commands (bump/stop)
//! to either the JSON-RPC or the legacy admin endpoint.
//!
//! Strategy selection follows `docs/protocols/peercast.md` §3.3.

use crate::util::{
    errors::{AppError, AppResult},
    http::CLIENT,
};

use super::{
    jsonrpc, legacy_admin, playlist,
    types::{ChannelInfo, ChannelStatus, PeerCastEndpoint},
    url::{self, UrlKind},
};

/// Resolve a user-supplied PeerCast URL into a stream URL ready for the
/// media player. For `pls` URLs we fetch the playlist body and pick the
/// first entry. For `stream` URLs we pass the URL through verbatim.
/// `play.html` is rewritten to `pls/{id}`.
pub async fn resolve_stream_url(url: &str) -> AppResult<String> {
    let parsed = url::parse(url)?;

    let direct_url = match parsed.kind {
        UrlKind::Stream => return Ok(url.to_string()),
        UrlKind::Playlist => url.to_string(),
        UrlKind::PlayHtml => {
            format!("{}/pls/{}", parsed.endpoint.base_url(), parsed.channel_id.as_str())
        }
    };

    let resp = CLIENT.get(&direct_url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!("playlist fetch returned {}", resp.status())));
    }
    let body = resp.text().await?;
    playlist::first_stream_url(&body)
}

/// Extract the PeerCast host/port that the URL points at.
pub fn endpoint_for(url: &str) -> AppResult<PeerCastEndpoint> {
    Ok(url::parse(url)?.endpoint)
}

/// Fetch channel info, preferring JSON-RPC and falling back to viewxml.
pub async fn fetch_info(endpoint: &PeerCastEndpoint, channel_id: &str) -> AppResult<ChannelInfo> {
    match jsonrpc::get_channel_info(endpoint, channel_id).await {
        Ok(info) => Ok(info),
        Err(_) => {
            let list = legacy_admin::view_xml(endpoint).await?;
            list.into_iter()
                .find(|c| c.channel_id.eq_ignore_ascii_case(channel_id))
                .map(|c| c.info)
                .ok_or_else(|| AppError::Network(format!("channel {channel_id} not found")))
        }
    }
}

/// Fetch channel status with the same strategy as `fetch_info`.
pub async fn fetch_status(
    endpoint: &PeerCastEndpoint,
    channel_id: &str,
) -> AppResult<ChannelStatus> {
    match jsonrpc::get_channel_status(endpoint, channel_id).await {
        Ok(st) => Ok(st),
        Err(_) => {
            let list = legacy_admin::view_xml(endpoint).await?;
            list.into_iter()
                .find(|c| c.channel_id.eq_ignore_ascii_case(channel_id))
                .map(|c| c.status)
                .ok_or_else(|| AppError::Network(format!("channel {channel_id} not found")))
        }
    }
}

pub async fn bump(endpoint: &PeerCastEndpoint, channel_id: &str) -> AppResult<()> {
    match jsonrpc::bump_channel(endpoint, channel_id).await {
        Ok(_) => Ok(()),
        Err(_) => legacy_admin::bump(endpoint, channel_id).await,
    }
}

pub async fn stop(endpoint: &PeerCastEndpoint, channel_id: &str) -> AppResult<()> {
    match jsonrpc::stop_channel(endpoint, channel_id).await {
        Ok(_) => Ok(()),
        Err(_) => legacy_admin::stop(endpoint, channel_id).await,
    }
}

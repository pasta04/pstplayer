//! High-level PeerCast client. Resolves a user-supplied URL into a
//! concrete stream URL that libmpv can play.
//!
//! See `docs/protocols/peercast.md` §2.

use crate::util::{
    errors::{AppError, AppResult},
    http::CLIENT,
};

use super::{
    playlist,
    types::PeerCastEndpoint,
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

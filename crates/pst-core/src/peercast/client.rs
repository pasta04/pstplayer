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
    types::{BasicAuth, ChannelInfo, ChannelStatus, PeerCastEndpoint},
    url::{self, UrlKind},
};
use crate::cli::CliArgs;
use crate::config::schema::PeerCastConfig;

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
            format!(
                "{}/pls/{}",
                parsed.endpoint.base_url(),
                parsed.channel_id.as_str()
            )
        }
    };

    let resp = CLIENT.get(&direct_url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!(
            "playlist fetch returned {}",
            resp.status()
        )));
    }
    let body = resp.text().await?;
    playlist::first_stream_url(&body)
}

/// Extract the PeerCast host/port that the URL points at.
pub fn endpoint_for(url: &str) -> AppResult<PeerCastEndpoint> {
    Ok(url::parse(url)?.endpoint)
}

/// Same as [`endpoint_for`] but stamps the user's saved Basic auth
/// onto the endpoint. URLs do not carry credentials, so we pull
/// them from `config.toml` whenever the bare endpoint is going to be
/// used to drive JSON-RPC / legacy admin calls.
pub fn endpoint_for_with_auth(url: &str, cfg: &PeerCastConfig) -> AppResult<PeerCastEndpoint> {
    let parsed = url::parse(url)?;
    Ok(PeerCastEndpoint {
        host: parsed.endpoint.host,
        port: parsed.endpoint.port,
        auth: auth_from_cfg(cfg),
    })
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

/// Resolve which PeerCast endpoint to talk to.
///
/// Order, highest to lowest priority (`docs/features.md` §2.2):
///
/// 1. The host:port embedded in a CLI-supplied URL (session-only, not
///    persisted).
/// 2. The user's saved settings (`[peercast]` in `config.toml`).
/// 3. Hard-coded default `localhost:7144`.
pub fn resolve_endpoint(cli: &CliArgs, cfg: &PeerCastConfig) -> PeerCastEndpoint {
    if let Some(url) = &cli.url {
        if let Ok(parsed) = url::parse(url) {
            // CLI URLs don't carry auth; fall back to config auth.
            return PeerCastEndpoint {
                host: parsed.endpoint.host,
                port: parsed.endpoint.port,
                auth: auth_from_cfg(cfg),
            };
        }
    }

    PeerCastEndpoint {
        host: cfg.host.clone(),
        port: cfg.port,
        auth: auth_from_cfg(cfg),
    }
}

pub(crate) fn auth_from_cfg(cfg: &PeerCastConfig) -> Option<BasicAuth> {
    match (&cfg.auth_user, &cfg.auth_pass) {
        (Some(u), Some(p)) if !u.is_empty() => Some(BasicAuth {
            user: u.clone(),
            pass: p.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_config_when_cli_url_absent() {
        let cli = CliArgs::default();
        let cfg = PeerCastConfig {
            host: "192.0.2.55".into(),
            port: 7148,
            auth_user: Some("u".into()),
            auth_pass: Some("p".into()),
            timeout_sec: 5,
            recent_hosts: vec![],
        };
        let ep = resolve_endpoint(&cli, &cfg);
        assert_eq!(ep.host, "192.0.2.55");
        assert_eq!(ep.port, 7148);
        assert_eq!(ep.auth.unwrap().user, "u");
    }

    #[test]
    fn cli_url_host_wins_over_config() {
        let cli = CliArgs {
            url: Some("http://192.0.2.99:7150/pls/0123456789ABCDEF0123456789ABCDEF".into()),
            ..Default::default()
        };
        let cfg = PeerCastConfig {
            host: "localhost".into(),
            port: 7144,
            ..Default::default()
        };
        let ep = resolve_endpoint(&cli, &cfg);
        assert_eq!(ep.host, "192.0.2.99");
        assert_eq!(ep.port, 7150);
    }

    #[test]
    fn invalid_cli_url_falls_back_to_config() {
        let cli = CliArgs {
            url: Some("not a url".into()),
            ..Default::default()
        };
        let cfg = PeerCastConfig {
            host: "h".into(),
            port: 1234,
            ..Default::default()
        };
        let ep = resolve_endpoint(&cli, &cfg);
        assert_eq!(ep.host, "h");
        assert_eq!(ep.port, 1234);
    }

    #[test]
    fn endpoint_for_with_auth_attaches_config_creds() {
        let cfg = PeerCastConfig {
            auth_user: Some("admin".into()),
            auth_pass: Some("secret".into()),
            ..Default::default()
        };
        let ep = endpoint_for_with_auth(
            "http://192.0.2.55:7148/pls/0123456789ABCDEF0123456789ABCDEF",
            &cfg,
        )
        .unwrap();
        assert_eq!(ep.host, "192.0.2.55");
        assert_eq!(ep.port, 7148);
        let auth = ep.auth.expect("auth attached");
        assert_eq!(auth.user, "admin");
        assert_eq!(auth.pass, "secret");
    }

    #[test]
    fn endpoint_for_with_auth_omits_creds_when_blank() {
        let cfg = PeerCastConfig::default();
        let ep = endpoint_for_with_auth(
            "http://localhost:7144/pls/0123456789ABCDEF0123456789ABCDEF",
            &cfg,
        )
        .unwrap();
        assert!(ep.auth.is_none());
    }

    #[test]
    fn empty_auth_is_ignored() {
        let cli = CliArgs::default();
        let cfg = PeerCastConfig {
            auth_user: Some(String::new()),
            auth_pass: Some(String::new()),
            ..Default::default()
        };
        let ep = resolve_endpoint(&cli, &cfg);
        assert!(ep.auth.is_none());
    }
}

//! PeerCast URL parsing.
//!
//! Recognises the three URL families documented in
//! `docs/protocols/peercast.md` §2:
//!
//! * `http://host:port/pls/{id}[.{ext}][?tip={tip}]`
//! * `http://host:port/stream/{id}.{ext}`
//! * `http://host:port/play.html?id={id}`
//!
//! The parser returns the channel id, the kind of URL, and the
//! endpoint (host/port) that the caller can use as the PeerCast
//! contact host.

use crate::util::errors::{AppError, AppResult};
use once_cell::sync::Lazy;
use regex::Regex;

use super::types::{ChannelId, PeerCastEndpoint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlKind {
    Playlist,
    Stream,
    PlayHtml,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUrl {
    pub kind: UrlKind,
    pub endpoint: PeerCastEndpoint,
    pub channel_id: ChannelId,
    pub ext: Option<String>,
    pub tip: Option<String>,
}

static PLS_OR_STREAM: Lazy<Regex> = Lazy::new(|| {
    // Allow IPv4 / hostnames / [IPv6] hosts.
    Regex::new(
        r"^https?://(?P<host>[^:/]+|\[[^\]]+\]):(?P<port>\d+)/(?P<kind>pls|stream)/(?P<id>[0-9A-Fa-f]{32})(?:\.(?P<ext>[A-Za-z0-9]+))?(?:\?(?P<query>.*))?$",
    )
    .expect("PLS_OR_STREAM regex compiles")
});

static PLAY_HTML: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^https?://(?P<host>[^:/]+|\[[^\]]+\]):(?P<port>\d+)/play\.html\?(?:.*&)?id=(?P<id>[0-9A-Fa-f]{32})",
    )
    .expect("PLAY_HTML regex compiles")
});

fn extract_tip(query: Option<&str>) -> Option<String> {
    let q = query?;
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=')?;
        if k == "tip" {
            return Some(v.to_string());
        }
    }
    None
}

pub fn parse(url: &str) -> AppResult<ParsedUrl> {
    if let Some(c) = PLS_OR_STREAM.captures(url) {
        let host = c.name("host").unwrap().as_str().to_string();
        let port: u16 = c
            .name("port")
            .unwrap()
            .as_str()
            .parse()
            .map_err(|_| AppError::InvalidUrl(format!("invalid port in {url}")))?;
        let kind = match c.name("kind").unwrap().as_str() {
            "pls" => UrlKind::Playlist,
            "stream" => UrlKind::Stream,
            other => return Err(AppError::InvalidUrl(format!("unexpected kind: {other}"))),
        };
        let id = c.name("id").unwrap().as_str();
        let channel_id =
            ChannelId::parse(id).map_err(|e| AppError::InvalidUrl(format!("{e}: {id}")))?;
        let ext = c.name("ext").map(|m| m.as_str().to_ascii_lowercase());
        let tip = extract_tip(c.name("query").map(|m| m.as_str()));

        return Ok(ParsedUrl {
            kind,
            endpoint: PeerCastEndpoint {
                host,
                port,
                auth: None,
            },
            channel_id,
            ext,
            tip,
        });
    }

    if let Some(c) = PLAY_HTML.captures(url) {
        let host = c.name("host").unwrap().as_str().to_string();
        let port: u16 = c
            .name("port")
            .unwrap()
            .as_str()
            .parse()
            .map_err(|_| AppError::InvalidUrl(format!("invalid port in {url}")))?;
        let id = c.name("id").unwrap().as_str();
        let channel_id =
            ChannelId::parse(id).map_err(|e| AppError::InvalidUrl(format!("{e}: {id}")))?;

        return Ok(ParsedUrl {
            kind: UrlKind::PlayHtml,
            endpoint: PeerCastEndpoint {
                host,
                port,
                auth: None,
            },
            channel_id,
            ext: None,
            tip: None,
        });
    }

    Err(AppError::InvalidUrl(format!(
        "not a recognised PeerCast URL: {url}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789ABCDEF0123456789ABCDEF";
    const ID_LOWER: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn pls_minimum() {
        let p = parse(&format!("http://localhost:7144/pls/{ID}")).unwrap();
        assert_eq!(p.kind, UrlKind::Playlist);
        assert_eq!(p.endpoint.host, "localhost");
        assert_eq!(p.endpoint.port, 7144);
        assert_eq!(p.channel_id.as_str(), ID_LOWER);
        assert_eq!(p.ext, None);
        assert_eq!(p.tip, None);
    }

    #[test]
    fn pls_with_ext_and_tip() {
        let p = parse(&format!(
            "http://192.0.2.10:7144/pls/{ID}.flv?tip=192.0.2.99:7144"
        ))
        .unwrap();
        assert_eq!(p.kind, UrlKind::Playlist);
        assert_eq!(p.endpoint.host, "192.0.2.10");
        assert_eq!(p.endpoint.port, 7144);
        assert_eq!(p.ext.as_deref(), Some("flv"));
        assert_eq!(p.tip.as_deref(), Some("192.0.2.99:7144"));
    }

    #[test]
    fn stream_form() {
        let p = parse(&format!("http://localhost:7144/stream/{ID}.flv")).unwrap();
        assert_eq!(p.kind, UrlKind::Stream);
        assert_eq!(p.ext.as_deref(), Some("flv"));
    }

    #[test]
    fn play_html() {
        let p = parse(&format!("http://localhost:7144/play.html?id={ID}")).unwrap();
        assert_eq!(p.kind, UrlKind::PlayHtml);
        assert_eq!(p.channel_id.as_str(), ID_LOWER);
    }

    #[test]
    fn rejects_unknown_path() {
        assert!(parse("http://localhost:7144/foo/bar").is_err());
    }

    #[test]
    fn rejects_bad_id_length() {
        // 31 hex chars
        let bad = "0123456789abcdef0123456789abcde";
        assert!(parse(&format!("http://localhost:7144/pls/{bad}")).is_err());
    }

    #[test]
    fn normalises_case() {
        let mixed = "0123456789AbCdEf0123456789aBcDeF";
        let p = parse(&format!("http://localhost:7144/pls/{mixed}")).unwrap();
        assert_eq!(p.channel_id.as_str(), ID_LOWER);
    }
}

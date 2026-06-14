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

/// `host:port`(IPv4 / hostname)または `[v6]:port` 形式の YP `tip` 値が
/// URL クエリに直接埋め込んでも安全かを検証する。受け付けるのは英数字
/// と `. - _ : [ ]` のみ。`&` / `?` / 空白 / 改行 / マルチバイト文字を
/// 含む値は弾く (URL クエリ注入 / spawn 引数注入対策)。
fn is_safe_tip(tip: &str) -> bool {
    static TIP_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^[A-Za-z0-9.\-_:\[\]]{1,128}$").expect("TIP_RE compiles"));
    TIP_RE.is_match(tip) && tip.contains(':')
}

/// `http://host:port/pls/{channel_id}[?tip=<tip>]` を組み立てる。
///
/// `channel_id` は呼び出し側で `ChannelId::parse` 済みの値を渡すこと
/// (32 hex 小文字)。`tip` が `Some` で値が `is_safe_tip` を満たすときだけ
/// クエリに付加する。安全でない / 空文字列は黙って無視 (= tip 無し URL)。
///
/// この関数は spawn_viewer から呼ばれて子プロセスに渡る URL を作るため、
/// 注入耐性が要件: `tip` 経由で追加クエリや余計な引数を埋め込まれない
/// ようにする (詳細は [`is_safe_tip`])。
pub fn build_pls_url(host: &str, port: u16, channel_id: &str, tip: Option<&str>) -> String {
    let safe_tip = tip
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .filter(|t| is_safe_tip(t));
    match safe_tip {
        Some(t) => format!("http://{host}:{port}/pls/{channel_id}?tip={t}"),
        None => format!("http://{host}:{port}/pls/{channel_id}"),
    }
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

    // build_pls_url: spawn_viewer から PeerCast 本体に投げる URL の組み立て。
    // tip が抜けていると未 subscribe のチャンネルで PeerCast が 404 を返す
    // ため、YP 経由起動では tip を必ず付ける (= 回帰防止テスト)。

    #[test]
    fn build_pls_url_without_tip() {
        let u = build_pls_url("localhost", 7144, ID_LOWER, None);
        assert_eq!(u, format!("http://localhost:7144/pls/{ID_LOWER}"));
    }

    #[test]
    fn build_pls_url_with_tip() {
        let u = build_pls_url("localhost", 7144, ID_LOWER, Some("192.0.2.99:7144"));
        assert_eq!(
            u,
            format!("http://localhost:7144/pls/{ID_LOWER}?tip=192.0.2.99:7144")
        );
        // 組み立てた URL は parser でも tip を抽出できる (= round-trip)。
        let p = parse(&u).unwrap();
        assert_eq!(p.tip.as_deref(), Some("192.0.2.99:7144"));
    }

    #[test]
    fn build_pls_url_drops_empty_or_blank_tip() {
        let cases = ["", "   ", "\t"];
        for tip in cases {
            let u = build_pls_url("h", 1, ID_LOWER, Some(tip));
            assert!(!u.contains("tip="), "expected no tip for {tip:?}: {u}");
        }
    }

    #[test]
    fn build_pls_url_rejects_unsafe_tip() {
        // クエリ注入 / spawn 引数注入を試みる文字列はすべて drop されること。
        let cases = [
            "1.2.3.4:80&record_on_start=1",
            "1.2.3.4:80?evil=1",
            "1.2.3.4:80 evil",
            "1.2.3.4:80\nevil",
            "1.2.3.4",      // ":" 必須
            "evil",         // ":" 必須
            "ホスト:7144", // マルチバイト不可
        ];
        for tip in cases {
            let u = build_pls_url("h", 1, ID_LOWER, Some(tip));
            assert!(!u.contains("tip="), "expected no tip for {tip:?}: {u}");
        }
    }

    #[test]
    fn build_pls_url_accepts_ipv6_bracketed_tip() {
        let u = build_pls_url("h", 1, ID_LOWER, Some("[2001:db8::1]:7144"));
        assert!(u.contains("?tip=[2001:db8::1]:7144"), "got: {u}");
    }
}

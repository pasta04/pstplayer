//! PeerCastStation の HLS 出力を中継するプロキシ。
//!
//! 実機の PeerCastStation で確認した挙動 (2026-07 実測):
//!   * playlist は `GET /hls/{id}` (ベアパス)。`/hls/{id}/index.m3u8` は
//!     404/503 になる。
//!   * 未リレーのチャンネルは `?tip=host:port` を付けないと 503
//!     (join できない)。
//!   * playlist は `?session=...` へのリダイレクト + m3u8 内の URI も
//!     クエリ付き自己参照になるため、クエリは常に上流へ透過する。
//!   * m3u8 内に上流の絶対 URL が含まれる場合はブラウザから直接
//!     上流へ行ってしまう (CORS で死ぬ) ため、本サーバのパスへ
//!     書き換えて返す。
//!
//! 本サーバは:
//!   `GET /hls/{id}[?tip=|?session=...]` → 上流 `/hls/{id}` (クエリ透過、
//!     m3u8 は URL 書き換えして返す)
//!   `GET /hls/{id}/{segment}[?...]`     → 上流 `/hls/{id}/{segment}`
//!     (クエリ透過・stream 透過)

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use pst_core::util::http::STREAM_CLIENT;
use reqwest::header::{
    HeaderName, HeaderValue, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, IF_MODIFIED_SINCE,
    LAST_MODIFIED, LOCATION, RANGE,
};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// 上流 PeerCastStation の HLS playlist URL (ベアパス + クエリ透過)。
async fn upstream_playlist_url(state: &AppState, channel_id: &str, query: Option<&str>) -> String {
    let (host, port) = upstream_host_port(state).await;
    match query.filter(|q| !q.is_empty()) {
        Some(q) => format!("http://{host}:{port}/hls/{channel_id}?{q}"),
        None => format!("http://{host}:{port}/hls/{channel_id}"),
    }
}

/// 上流 HLS の TS セグメント URL (クエリ透過)。
async fn upstream_segment_url(
    state: &AppState,
    channel_id: &str,
    segment: &str,
    query: Option<&str>,
) -> String {
    let (host, port) = upstream_host_port(state).await;
    match query.filter(|q| !q.is_empty()) {
        Some(q) => format!("http://{host}:{port}/hls/{channel_id}/{segment}?{q}"),
        None => format!("http://{host}:{port}/hls/{channel_id}/{segment}"),
    }
}

/// パストラバーサル等を防ぐため、id / segment に許容しない文字が
/// 混ざっていないかチェックする。`..` や `/`, `\` を含む値は上流の
/// URL 解釈次第で別パスに逃げる可能性があるので拒否。
fn is_safe_path_component(s: &str) -> bool {
    !s.is_empty()
        && !s.contains("..")
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains('\0')
        && s.chars().all(|c| !c.is_control())
}

async fn upstream_host_port(state: &AppState) -> (String, u16) {
    let cfg = state.cfg.read().await;
    (cfg.peercast.host.clone(), cfg.peercast.port)
}

async fn upstream_auth(state: &AppState) -> (Option<String>, Option<String>) {
    let cfg = state.cfg.read().await;
    (
        cfg.peercast.auth_user.clone(),
        cfg.peercast.auth_pass.clone(),
    )
}

/// 上流レスポンスを axum レスポンスに変換。stream で本体を透過する。
/// セグメント転送は低速 LAN だと数秒かかることがあるため、total
/// timeout 付きの CLIENT ではなく STREAM_CLIENT (connect + チャンク間
/// timeout のみ) を使う。
async fn proxy_get(url: String, headers: HeaderMap, state: &AppState) -> ApiResult<Response> {
    let mut req = STREAM_CLIENT.get(&url);
    // 受け取った Range / If-Modified-Since はそのまま上流に転送 (再生開始
    // 直後の再開や、playlist の差分取得で有用)。
    for h in [RANGE, IF_MODIFIED_SINCE] {
        if let Some(v) = headers.get(&h) {
            req = req.header(h, v);
        }
    }
    let (auth_user, auth_pass) = upstream_auth(state).await;
    if let (Some(u), Some(p)) = (auth_user.as_deref(), auth_pass.as_deref()) {
        if !u.is_empty() {
            req = req.basic_auth(u, Some(p));
        }
    }

    let resp = req.send().await.map_err(|e| ApiError {
        status: StatusCode::BAD_GATEWAY,
        code: "peercast_unreachable",
        message: format!("HLS 上流に接続できません: {e}"),
    })?;

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    // ヘッダの一部だけを引き継ぐ。Hop-by-hop ヘッダ等はパススルー
    // しないほうが安全。
    let mut out = Response::builder().status(status);
    if let Some(headers_mut) = out.headers_mut() {
        for h in [CONTENT_TYPE, CONTENT_LENGTH, CACHE_CONTROL, LAST_MODIFIED] {
            if let Some(v) = resp.headers().get(&h) {
                headers_mut.insert(h, v.clone());
            }
        }
        // Accept-Ranges / Content-Range は HLS シーク中に必要。
        for name in ["accept-ranges", "content-range"] {
            if let Some(v) = resp.headers().get(name) {
                if let (Ok(hn), Ok(hv)) = (
                    HeaderName::from_bytes(name.as_bytes()),
                    HeaderValue::from_bytes(v.as_bytes()),
                ) {
                    headers_mut.insert(hn, hv);
                }
            }
        }
    }

    let stream = resp.bytes_stream();
    let body = Body::from_stream(stream);
    out.body(body).map_err(|e| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "decode",
        message: format!("response build failed: {e}"),
    })
}

/// playlist 取得用クライアント。リダイレクトを**追わない** (PCS は
/// `?session=` 付き URL へ誘導し、以降の再取得はその session で継続する
/// 必要がある。プロキシ内部で追ってしまうと hls.js が毎回ベース URL を
/// 叩いて session が作り直され、セグメントが確定しないまま空回りする:
/// 実機 QA)。リダイレクトは Location をローカルパスに書き換えて
/// クライアントへ返す。
fn playlist_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build playlist HTTP client")
    })
}

/// 上流の絶対 URL (`http://host:port/...`) を本サーバのパスへ書き換える。
/// m3u8 の中身にだけ適用する (ブラウザが上流へ直接行くと CORS で死ぬ)。
fn rewrite_playlist_body(body: &str, host: &str, port: u16) -> String {
    let origin = format!("http://{host}:{port}");
    body.replace(&origin, "")
}

/// `GET /hls/{id}` → 上流 `/hls/{id}` (クエリ透過)。m3u8 は本文を読み、
/// 上流絶対 URL を本サーバのパスへ書き換えて返す。
pub async fn playlist(
    State(s): State<AppState>,
    Path(id): Path<String>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
    headers: HeaderMap,
) -> ApiResult<Response> {
    // `.m3u8` 拡張子付きで来た場合に剥がす (`open()` 側で固定で付けている)。
    let id = id.strip_suffix(".m3u8").unwrap_or(&id).to_string();
    if !is_safe_path_component(&id) {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_channel_id",
            message: format!("不正な channel_id: {id}"),
        });
    }
    let url = upstream_playlist_url(&s, &id, query.as_deref()).await;
    let _ = headers; // playlist は条件付き GET を上流に流さない (常に最新を返す)
    let (auth_user, auth_pass) = upstream_auth(&s).await;
    let mut req = playlist_client().get(&url);
    if let (Some(u), Some(p)) = (auth_user.as_deref(), auth_pass.as_deref()) {
        if !u.is_empty() {
            req = req.basic_auth(u, Some(p));
        }
    }
    let resp = req.send().await.map_err(|e| ApiError {
        status: StatusCode::BAD_GATEWAY,
        code: "peercast_unreachable",
        message: format!("HLS 上流に接続できません: {e}"),
    })?;
    let (host, port) = upstream_host_port(&s).await;
    let origin = format!("http://{host}:{port}");
    if resp.status().is_redirection() {
        // ?session= 付き URL への誘導。ローカルパスに書き換えて返し、
        // 以降のクライアントの再取得を session 継続にする。
        let loc = resp
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .replace(&origin, "");
        return Response::builder()
            .status(StatusCode::FOUND)
            .header(axum::http::header::LOCATION, loc)
            .body(Body::empty())
            .map_err(|e| ApiError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "decode",
                message: format!("redirect build failed: {e}"),
            });
    }
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = resp.headers().get(CONTENT_TYPE).cloned();
    let cache_control = resp.headers().get(CACHE_CONTROL).cloned();
    let is_m3u8 = content_type
        .as_ref()
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("mpegurl") || v.contains("m3u8") || v.starts_with("text/"))
        .unwrap_or(true);
    let bytes = resp.bytes().await.map_err(|e| ApiError {
        status: StatusCode::BAD_GATEWAY,
        code: "decode",
        message: format!("playlist read failed: {e}"),
    })?;
    let body = if status.is_success() && is_m3u8 {
        let text = String::from_utf8_lossy(&bytes);
        Body::from(rewrite_playlist_body(&text, &host, port))
    } else {
        Body::from(bytes)
    };
    let mut out = Response::builder().status(status);
    if let Some(h) = out.headers_mut() {
        if let Some(v) = content_type {
            h.insert(CONTENT_TYPE, v);
        }
        if let Some(v) = cache_control {
            h.insert(CACHE_CONTROL, v);
        }
    }
    out.body(body).map_err(|e| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "decode",
        message: format!("response build failed: {e}"),
    })
}

/// `GET /hls/{id}/{segment}` → 上流 `/hls/{id}/{segment}` (クエリ透過)
pub async fn segment(
    State(s): State<AppState>,
    Path((id, segment)): Path<(String, String)>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if !is_safe_path_component(&id) {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_channel_id",
            message: format!("不正な channel_id: {id}"),
        });
    }
    if !is_safe_path_component(&segment) {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_segment",
            message: format!("不正な segment 名: {segment}"),
        });
    }
    let url = upstream_segment_url(&s, &id, &segment, query.as_deref()).await;
    proxy_get(url, headers, &s).await
}

#[cfg(test)]
mod tests {
    use super::{is_safe_path_component, rewrite_playlist_body};

    #[test]
    fn rewrites_absolute_upstream_urls_to_local_paths() {
        let body = "#EXTM3U\nhttp://192.168.0.5:7144/hls/abc?session=X\nseg-1.ts\n";
        let out = rewrite_playlist_body(body, "192.168.0.5", 7144);
        assert_eq!(out, "#EXTM3U\n/hls/abc?session=X\nseg-1.ts\n");
    }

    #[test]
    fn accepts_normal_ids_and_segments() {
        assert!(is_safe_path_component("0123456789abcdef0123456789abcdef"));
        assert!(is_safe_path_component("seg-1.ts"));
        assert!(is_safe_path_component("index.m3u8"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_safe_path_component(""));
        assert!(!is_safe_path_component(".."));
        assert!(!is_safe_path_component("../etc/passwd"));
        assert!(!is_safe_path_component("a/b"));
        assert!(!is_safe_path_component("a\\b"));
        assert!(!is_safe_path_component("a\0b"));
        assert!(!is_safe_path_component("a\nb"));
        assert!(!is_safe_path_component("a\rb"));
    }
}

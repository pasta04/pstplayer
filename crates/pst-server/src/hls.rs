//! PeerCastStation の HLS 出力を中継するプロキシ。
//!
//! PeerCastStation は配信プレイヤー設定で HLS を選択でき、その場合
//! `/hls/{channel_id}/index.m3u8` のような URL で playlist と TS
//! セグメントを返す (バージョンによってパスが異なるため、`/hls/*` の
//! 任意パスを上流に転送する形にしておく)。
//!
//! 本サーバは:
//!   `GET /hls/{id}.m3u8`          → 上流 `/hls/{id}/index.m3u8`
//!   `GET /hls/{id}/{segment}.ts`  → 上流 `/hls/{id}/{segment}.ts`
//!
//! いずれもレスポンスを stream で透過。Range / If-Modified-Since は
//! axum が握ってくれる範囲だけ転送 (TS セグメントは通常 Range なし)。

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use pst_core::util::http::CLIENT;
use reqwest::header::{
    HeaderName, HeaderValue, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, IF_MODIFIED_SINCE,
    LAST_MODIFIED, RANGE,
};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// 上流 PeerCastStation の HLS playlist URL を組み立てる。
async fn upstream_playlist_url(state: &AppState, channel_id: &str) -> String {
    let (host, port) = upstream_host_port(state).await;
    format!("http://{host}:{port}/hls/{channel_id}/index.m3u8")
}

/// 上流 HLS の TS セグメント URL を組み立てる。
async fn upstream_segment_url(state: &AppState, channel_id: &str, segment: &str) -> String {
    let (host, port) = upstream_host_port(state).await;
    format!("http://{host}:{port}/hls/{channel_id}/{segment}")
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
async fn proxy_get(url: String, headers: HeaderMap, state: &AppState) -> ApiResult<Response> {
    let mut req = CLIENT.get(&url);
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

/// `GET /hls/{id}.m3u8` → 上流 `/hls/{id}/index.m3u8`
pub async fn playlist(
    State(s): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    // `.m3u8` 拡張子付きで来た場合に剥がす (`open()` 側で固定で付けている)。
    let id = id.strip_suffix(".m3u8").unwrap_or(&id).to_string();
    let url = upstream_playlist_url(&s, &id).await;
    proxy_get(url, headers, &s).await
}

/// `GET /hls/{id}/{segment}` → 上流 `/hls/{id}/{segment}` (透過)
pub async fn segment(
    State(s): State<AppState>,
    Path((id, segment)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let url = upstream_segment_url(&s, &id, &segment).await;
    proxy_get(url, headers, &s).await
}

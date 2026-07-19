//! FLV 生ストリームのパススループロキシ。
//!
//! ブラウザ視聴のリアルタイム再生用 (mpegts.js が MSE で直接再生する)。
//! HLS はセグメント (約8秒×3本) ぶん構造的に遅延するため、MSE が
//! 使える環境では FLV 直結を優先する。join は録画と同じ経路
//! (`/pls/{id}?tip=` を解決してから stream URL へ接続) を通る。
//! `/stream/{id}.{ext}` の直叩きは PeerCast 実装によっては join が
//! 始まらず無応答になる (recording.rs と同じ知見)。

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use pst_core::util::http::STREAM_CLIENT;
use reqwest::header::{HeaderValue, CACHE_CONTROL, CONTENT_TYPE};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /stream/{id}[.flv][?tip=host:port]` → 上流 FLV ストリーム透過。
pub async fn flv(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let id = id.strip_suffix(".flv").unwrap_or(&id).to_string();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_channel_id",
            message: format!("不正な channel_id: {id}"),
        });
    }
    let tip = params.get("tip").map(|v| v.as_str());
    let (host, port, auth_user, auth_pass) = {
        let cfg = s.cfg.read().await;
        (
            cfg.peercast.host.clone(),
            cfg.peercast.port,
            cfg.peercast.auth_user.clone(),
            cfg.peercast.auth_pass.clone(),
        )
    };
    let pls_url = pst_core::peercast::url::build_pls_url(&host, port, &id, tip);
    let upstream = pst_core::peercast::client::resolve_stream_url(&pls_url)
        .await
        .map_err(|e| ApiError {
            status: StatusCode::BAD_GATEWAY,
            code: "peercast_unreachable",
            message: format!("stream URL を解決できません: {e}"),
        })?;
    let mut req = STREAM_CLIENT.get(&upstream);
    if let (Some(u), Some(p)) = (auth_user.as_deref(), auth_pass.as_deref()) {
        if !u.is_empty() {
            req = req.basic_auth(u, Some(p));
        }
    }
    let resp = tokio::time::timeout(std::time::Duration::from_secs(45), req.send())
        .await
        .map_err(|_| ApiError {
            status: StatusCode::GATEWAY_TIMEOUT,
            code: "peercast_timeout",
            message: "上流ストリームへの接続がタイムアウトしました (45s)".into(),
        })?
        .map_err(|e| ApiError {
            status: StatusCode::BAD_GATEWAY,
            code: "peercast_unreachable",
            message: format!("上流ストリームに接続できません: {e}"),
        })?;
    if !resp.status().is_success() {
        return Err(ApiError {
            status: StatusCode::BAD_GATEWAY,
            code: "peercast_error",
            message: format!("上流ストリームが {} を返しました", resp.status()),
        });
    }
    let body = Body::from_stream(resp.bytes_stream());
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, HeaderValue::from_static("video/x-flv"))
        .header(CACHE_CONTROL, HeaderValue::from_static("no-store"))
        .body(body)
        .map_err(|e| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "decode",
            message: format!("response build failed: {e}"),
        })
}

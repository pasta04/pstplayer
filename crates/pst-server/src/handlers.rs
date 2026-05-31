//! HTTP ハンドラ。pst-core の API を叩いて JSON で返すだけの薄いラッパー。

use axum::extract::{Path, Query, State};
use axum::Json;
use pst_core::bbs::{
    router::classify,
    types::{BoardKind, PostRequest},
};
use pst_core::peercast::{client, types::ChannelRecord, yp, yp::YpEntry};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// ── PeerCast ────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ChannelsResp {
    pub channels: Vec<ChannelRecord>,
}

pub async fn channels(State(s): State<AppState>) -> ApiResult<Json<ChannelsResp>> {
    let ep = s.endpoint();
    let channels = pst_core::peercast::jsonrpc::get_channels(&ep).await?;
    Ok(Json(ChannelsResp { channels }))
}

pub async fn channel_info(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<pst_core::peercast::types::ChannelInfo>> {
    let ep = s.endpoint();
    let info = client::fetch_info(&ep, &id).await?;
    Ok(Json(info))
}

pub async fn channel_status(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<pst_core::peercast::types::ChannelStatus>> {
    let ep = s.endpoint();
    let status = client::fetch_status(&ep, &id).await?;
    Ok(Json(status))
}

#[derive(Serialize)]
pub struct Empty {}

pub async fn channel_bump(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Empty>> {
    let ep = s.endpoint();
    client::bump(&ep, &id).await?;
    Ok(Json(Empty {}))
}

pub async fn channel_stop(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Empty>> {
    let ep = s.endpoint();
    client::stop(&ep, &id).await?;
    Ok(Json(Empty {}))
}

// ── YP ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct YpQuery {
    /// 未指定 → ない時は空配列を返す (server 設定で YP URL を持つかは
    /// 今は未対応。クライアントから明示的に URL を渡す形)。
    pub url: Option<String>,
}

#[derive(Serialize)]
pub struct YpResp {
    pub entries: Vec<YpEntry>,
}

pub async fn yp_index(Query(q): Query<YpQuery>) -> ApiResult<Json<YpResp>> {
    let Some(url) = q.url else {
        return Ok(Json(YpResp { entries: vec![] }));
    };
    let entries = yp::fetch_index(&url).await?;
    Ok(Json(YpResp { entries }))
}

// ── BBS ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BoardQuery {
    pub url: String,
}

#[derive(Serialize)]
pub struct ThreadsResp {
    pub kind: Option<BoardKind>,
    pub threads: Vec<pst_core::bbs::parse::SubjectEntry>,
}

pub async fn board_threads(Query(q): Query<BoardQuery>) -> ApiResult<Json<ThreadsResp>> {
    let kind = classify(&q.url);
    let threads = match kind {
        Some(BoardKind::Shitaraba) => {
            pst_core::bbs::shitaraba::ShitarabaClient::new()
                .list_threads(&q.url)
                .await?
        }
        Some(BoardKind::Ch2Compat) => {
            pst_core::bbs::ch2::Ch2Client::new().list_threads(&q.url).await?
        }
        None => {
            return Err(ApiError {
                status: axum::http::StatusCode::BAD_REQUEST,
                code: "invalid_url",
                message: format!("not a supported BBS URL: {}", q.url),
            })
        }
    };
    Ok(Json(ThreadsResp { kind, threads }))
}

#[derive(Deserialize)]
pub struct ThreadQuery {
    pub url: String,
    pub last_count: Option<u32>,
    pub last_byte: Option<u64>,
    pub last_modified: Option<String>,
}

#[derive(Serialize)]
pub struct ThreadResp {
    pub kind: Option<BoardKind>,
    pub posts: Vec<pst_core::bbs::types::Post>,
    pub state: pst_core::bbs::types::FetchState,
}

pub async fn thread_fetch(Query(q): Query<ThreadQuery>) -> ApiResult<Json<ThreadResp>> {
    let kind = classify(&q.url);
    let prev = if q.last_count.is_some() || q.last_byte.is_some() || q.last_modified.is_some() {
        Some(pst_core::bbs::types::FetchState {
            last_count: q.last_count.unwrap_or(0),
            last_byte: q.last_byte.unwrap_or(0),
            last_modified: q.last_modified.clone(),
        })
    } else {
        None
    };
    let (posts, state) = match kind {
        Some(BoardKind::Shitaraba) => {
            pst_core::bbs::shitaraba::ShitarabaClient::new()
                .fetch_thread(&q.url, prev.as_ref())
                .await?
        }
        Some(BoardKind::Ch2Compat) => {
            pst_core::bbs::ch2::Ch2Client::new()
                .fetch_thread(&q.url, prev.as_ref())
                .await?
        }
        None => {
            return Err(ApiError {
                status: axum::http::StatusCode::BAD_REQUEST,
                code: "invalid_url",
                message: format!("not a supported BBS URL: {}", q.url),
            })
        }
    };
    Ok(Json(ThreadResp { kind, posts, state }))
}

#[derive(Deserialize)]
pub struct PostBody {
    pub url: String,
    pub name: String,
    pub mail: String,
    pub body: String,
}

pub async fn thread_post(Json(b): Json<PostBody>) -> ApiResult<Json<Empty>> {
    let kind = classify(&b.url);
    let req = PostRequest {
        name: b.name,
        mail: b.mail,
        body: b.body,
    };
    match kind {
        Some(BoardKind::Shitaraba) => {
            pst_core::bbs::shitaraba::ShitarabaClient::new()
                .post(&b.url, &req)
                .await?
        }
        Some(BoardKind::Ch2Compat) => {
            pst_core::bbs::ch2::Ch2Client::new().post(&b.url, &req).await?
        }
        None => {
            return Err(ApiError {
                status: axum::http::StatusCode::BAD_REQUEST,
                code: "invalid_url",
                message: format!("not a supported BBS URL: {}", b.url),
            })
        }
    };
    Ok(Json(Empty {}))
}

// ── Index ──────────────────────────────────────────────────────

/// MVP の最小ランディング。Svelte の Web ビルドを後で /pkg にマウントする
/// 方針なので、ここは placeholder。
pub async fn index() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!doctype html>
<html lang="ja"><head><meta charset="utf-8"><title>PSTPlayer Server</title></head>
<body style="font-family: sans-serif; padding: 2rem; line-height: 1.7;">
<h1>PSTPlayer Server</h1>
<p>HTTP API is up. Endpoints:</p>
<ul>
  <li><code>GET /api/channels</code></li>
  <li><code>GET /api/channel/{id}/info</code></li>
  <li><code>GET /api/channel/{id}/status</code></li>
  <li><code>POST /api/channel/{id}/bump</code></li>
  <li><code>POST /api/channel/{id}/stop</code></li>
  <li><code>GET /api/yp?url=...</code></li>
  <li><code>GET /api/board?url=...</code></li>
  <li><code>GET /api/thread?url=...&amp;last_count=&amp;last_byte=&amp;last_modified=</code></li>
  <li><code>POST /api/thread/post</code></li>
</ul>
<p>Browser frontend (Svelte PWA) will be mounted here in a future release.</p>
</body></html>"#,
    )
}

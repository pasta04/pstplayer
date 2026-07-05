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
    let ep = s.endpoint().await;
    let channels = pst_core::peercast::jsonrpc::get_channels(&ep).await?;
    Ok(Json(ChannelsResp { channels }))
}

pub async fn channel_info(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<pst_core::peercast::types::ChannelInfo>> {
    let ep = s.endpoint().await;
    let info = client::fetch_info(&ep, &id).await?;
    Ok(Json(info))
}

pub async fn channel_status(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<pst_core::peercast::types::ChannelStatus>> {
    let ep = s.endpoint().await;
    let status = client::fetch_status(&ep, &id).await?;
    Ok(Json(status))
}

#[derive(Serialize)]
pub struct Empty {}

pub async fn channel_bump(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Empty>> {
    let ep = s.endpoint().await;
    client::bump(&ep, &id).await?;
    Ok(Json(Empty {}))
}

pub async fn channel_stop(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Empty>> {
    let ep = s.endpoint().await;
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

/// 設定 (`[yp].sources`) の全 YP を並行 fetch して集約した一覧を返す。
/// pst-server が「YP 一覧表示」を担うためのサーバ側集約。個別 YP の
/// 取得失敗は他に影響させず `failures` に積んで継続する (pst-core の
/// `fetch_indexes` をそのまま再利用)。
pub async fn yp_all(State(s): State<AppState>) -> Json<yp::MultiFetchOutcome> {
    // ロックは clone まで。fetch は guard を手放してから行う。
    let sources = s.cfg.read().await.yp.sources.clone();
    Json(yp::fetch_indexes(&sources).await)
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
            pst_core::bbs::ch2::Ch2Client::new()
                .list_threads(&q.url)
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
            full_reload: false,
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
            pst_core::bbs::ch2::Ch2Client::new()
                .post(&b.url, &req)
                .await?
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

// ── Recording (複数本並行) ────────────────────────────────────

#[derive(Deserialize)]
pub struct RecordStart {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// 配信元ヒント (host:port)。未リレーのチャンネルの録画開始に必要。
    #[serde(default)]
    pub tip: Option<String>,
}

pub async fn record_start(
    State(s): State<AppState>,
    Json(b): Json<RecordStart>,
) -> ApiResult<Json<crate::recording::RecordingEntry>> {
    let entry = s.recording.start(&s, b.id, b.name, b.tip).await?;
    Ok(Json(entry))
}

#[derive(Deserialize)]
pub struct RecordStop {
    /// `id` 省略時は全停止。
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Serialize)]
pub struct RecordStopResp {
    pub stopped: usize,
}

pub async fn record_stop(
    State(s): State<AppState>,
    Json(b): Json<RecordStop>,
) -> Json<RecordStopResp> {
    let stopped = match b.id {
        Some(id) => {
            if s.recording.stop(&id).await {
                1
            } else {
                0
            }
        }
        None => s.recording.stop_all().await,
    };
    Json(RecordStopResp { stopped })
}

pub async fn record_list(State(s): State<AppState>) -> Json<crate::recording::RecordingList> {
    Json(s.recording.list().await)
}

// ── Favorites ─────────────────────────────────────────────────

pub async fn favorites_list(
    State(s): State<AppState>,
) -> Json<pst_core::favorites::FavoritesConfig> {
    Json(s.cfg.read().await.favorites.clone())
}

// ── Config (Web UI から読み書き) ──────────────────────────────

pub async fn get_config(State(s): State<AppState>) -> Json<crate::config::Config> {
    Json(s.cfg.read().await.clone())
}

pub async fn put_config(
    State(s): State<AppState>,
    Json(new): Json<crate::config::Config>,
) -> ApiResult<Json<crate::config::Config>> {
    // 先にディスクに書き出す。失敗したらメモリは触らない。
    crate::config::save_to(&s.config_path, &new).map_err(|e| ApiError {
        status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        code: "config_save_failed",
        message: e.to_string(),
    })?;
    *s.cfg.write().await = new.clone();
    Ok(Json(new))
}

/// 設定ファイルのパスを返す (UI に「保存先 ファイル」を出すため)。
pub async fn config_path(State(s): State<AppState>) -> Json<ConfigPathResp> {
    Json(ConfigPathResp {
        path: s.config_path.to_string_lossy().into_owned(),
    })
}

#[derive(Serialize)]
pub struct ConfigPathResp {
    pub path: String,
}

/// 任意の (スレ or 板) URL → 板トップ URL (Tauri command `board_url_of`
/// の REST 版)。
pub async fn board_url_of(
    axum::extract::Query(q): axum::extract::Query<BoardQuery>,
) -> ApiResult<Json<ThreadUrlResp>> {
    let url = pst_core::bbs::url::build_board_url(&q.url).map_err(|e| ApiError {
        status: axum::http::StatusCode::BAD_REQUEST,
        code: "invalid_url",
        message: e.to_string(),
    })?;
    Ok(Json(ThreadUrlResp { url }))
}

/// 板の SETTING (最大レス数等)。満レス判定 (最新スレへのリダイレクト)
/// に使う (Tauri command `fetch_board_setting` の REST 版)。
pub async fn board_setting(
    axum::extract::Query(q): axum::extract::Query<BoardQuery>,
) -> ApiResult<Json<pst_core::bbs::types::BoardSetting>> {
    let setting = match classify(&q.url) {
        Some(BoardKind::Shitaraba) => {
            pst_core::bbs::shitaraba::ShitarabaClient::new()
                .fetch_setting(&q.url)
                .await?
        }
        Some(BoardKind::Ch2Compat) => {
            pst_core::bbs::ch2::Ch2Client::new()
                .fetch_setting(&q.url)
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
    Ok(Json(setting))
}

#[derive(Deserialize)]
pub struct ThreadUrlQuery {
    pub url: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct ThreadUrlResp {
    pub url: String,
}

/// 板 URL + key → canonical なスレッド URL (Tauri command `thread_url_of`
/// の REST 版。ブラウザの視聴ページが「コンタクト = 板 URL」のとき最新
/// スレを開くのに使う)。
pub async fn thread_url_of(
    axum::extract::Query(q): axum::extract::Query<ThreadUrlQuery>,
) -> ApiResult<Json<ThreadUrlResp>> {
    let url = pst_core::bbs::url::build_thread_url(&q.url, &q.key).map_err(|e| ApiError {
        status: axum::http::StatusCode::BAD_REQUEST,
        code: "invalid_url",
        message: e.to_string(),
    })?;
    Ok(Json(ThreadUrlResp { url }))
}

// ── Redirects ──────────────────────────────────────────────────

/// `/` はハブ (YP 一覧) へ。ブラウザでユーザーが最初に見たいのは YP。
pub async fn root_redirect() -> axum::response::Redirect {
    axum::response::Redirect::temporary("/hub")
}

/// 旧 `/watch` → `/player`。`?id=...` 等のクエリを引き継ぐ。
pub async fn watch_redirect(
    axum::extract::RawQuery(q): axum::extract::RawQuery,
) -> axum::response::Redirect {
    match q.as_deref() {
        Some(q) if !q.is_empty() => axum::response::Redirect::temporary(&format!("/player?{q}")),
        _ => axum::response::Redirect::temporary("/player"),
    }
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
  <li><code>GET /api/yp/all</code> (設定の全 YP を集約)</li>
  <li><code>GET /api/board?url=...</code></li>
  <li><code>GET /api/thread?url=...&amp;last_count=&amp;last_byte=&amp;last_modified=</code></li>
  <li><code>POST /api/thread/post</code></li>
</ul>
<p>Browser frontend (Svelte PWA) will be mounted here in a future release.</p>
</body></html>"#,
    )
}

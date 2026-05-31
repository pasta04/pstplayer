//! PSTPlayer リレーサーバ。設計は [`docs/decisions/0005-workspace-and-server.md`]
//! を参照。
//!
//! MVP のスコープ:
//! * 紐付け先 PeerCastStation の JSON-RPC をプロキシして browser に返す
//! * したらば / 2ch 互換 BBS の取得 / 投稿をプロキシ
//! * 静的フロント (Svelte の Web ビルド) と HLS 配信は次フェーズ

pub mod config;
pub mod error;
pub mod handlers;
pub mod hls;
pub mod recording;
pub mod state;

use std::path::PathBuf;

use axum::{routing, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::state::AppState;

/// Router を組み立てる。
///
/// `web_dir` には静的フロント (PWA + Vanilla JS) のディレクトリを渡す。
/// 通常は `crates/pst-server/web/` を指す。`None` ならフォールバックの
/// 暫定 HTML だけを `/` で返す。
pub fn build_router(state: AppState, web_dir: Option<PathBuf>) -> Router {
    // CORS は当面 LAN 用途想定で全開。公開時は origin を絞ること。
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // axum 0.7 系のパスパラメータ syntax は `:id` (axum 0.8 から `{id}`)。
    // 上げると tower-http 等の周辺もメジャー追従が要るので当面 0.7 で。
    let mut router = Router::new()
        // PeerCast
        .route("/api/channels", routing::get(handlers::channels))
        .route(
            "/api/channel/:id/info",
            routing::get(handlers::channel_info),
        )
        .route(
            "/api/channel/:id/status",
            routing::get(handlers::channel_status),
        )
        .route(
            "/api/channel/:id/bump",
            routing::post(handlers::channel_bump),
        )
        .route(
            "/api/channel/:id/stop",
            routing::post(handlers::channel_stop),
        )
        // YP
        .route("/api/yp", routing::get(handlers::yp_index))
        // BBS
        .route("/api/board", routing::get(handlers::board_threads))
        .route("/api/thread", routing::get(handlers::thread_fetch))
        .route("/api/thread/post", routing::post(handlers::thread_post))
        // HLS proxy (上流 PeerCastStation の /hls/{id} を透過)
        .route("/hls/:id", routing::get(hls::playlist))
        .route("/hls/:id/:segment", routing::get(hls::segment))
        // Recording (既定 OFF、config の [recording] enabled = true 必要)。
        // 同時録画は `[recording] max_concurrent` (既定 8) まで。
        .route("/api/record/start", routing::post(handlers::record_start))
        .route("/api/record/stop", routing::post(handlers::record_stop))
        .route("/api/record/list", routing::get(handlers::record_list))
        // Favorites (frontend に config の rules を流すだけの read-only)
        .route("/api/favorites", routing::get(handlers::favorites_list));

    if let Some(dir) = web_dir.filter(|p| p.is_dir()) {
        // `/` 以下はすべて静的フロント (ServeDir)。API ルートが既に上で
        // 定義されているので、衝突せず static のみが裏でフォールバック。
        router = router.fallback_service(ServeDir::new(dir).append_index_html_on_directories(true));
    } else {
        router = router.route("/", routing::get(handlers::index));
    }

    router.layer(cors).with_state(state)
}

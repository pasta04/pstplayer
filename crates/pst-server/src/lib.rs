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
pub mod state;

use axum::{routing, Router};
use tower_http::cors::{Any, CorsLayer};

use crate::state::AppState;

/// Router を組み立てる。テストで axum::Server を立てずに `tower::ServiceExt`
/// 経由でリクエストを流し込めるよう、生の `Router` を返す。
pub fn build_router(state: AppState) -> Router {
    // CORS は当面 LAN 用途想定で全開。公開時は origin を絞ること。
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", routing::get(handlers::index))
        // PeerCast
        .route("/api/channels", routing::get(handlers::channels))
        .route(
            "/api/channel/{id}/info",
            routing::get(handlers::channel_info),
        )
        .route(
            "/api/channel/{id}/status",
            routing::get(handlers::channel_status),
        )
        .route(
            "/api/channel/{id}/bump",
            routing::post(handlers::channel_bump),
        )
        .route(
            "/api/channel/{id}/stop",
            routing::post(handlers::channel_stop),
        )
        // YP
        .route("/api/yp", routing::get(handlers::yp_index))
        // BBS
        .route("/api/board", routing::get(handlers::board_threads))
        .route("/api/thread", routing::get(handlers::thread_fetch))
        .route("/api/thread/post", routing::post(handlers::thread_post))
        .layer(cors)
        .with_state(state)
}

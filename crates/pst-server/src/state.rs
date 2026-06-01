//! axum 各ハンドラに inject される共有 state。
//!
//! `cfg` を `Arc<RwLock<Config>>` にしておくのは Web UI から
//! `PUT /api/config` で実行時に書き換える運用 (PeerCastStation の
//! Web UI に似た方針) を取るため。各ハンドラは短いクリティカル
//! セクション内でロックを取り、必要な値を clone してから使う。

use std::path::PathBuf;
use std::sync::Arc;

use pst_core::peercast::types::{BasicAuth, PeerCastEndpoint};
use tokio::sync::RwLock;

use crate::config::Config;
use crate::recording::RecordingState;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<RwLock<Config>>,
    pub config_path: Arc<PathBuf>,
    pub recording: Arc<RecordingState>,
}

impl AppState {
    pub fn new(cfg: Config, config_path: PathBuf) -> Self {
        Self {
            cfg: Arc::new(RwLock::new(cfg)),
            config_path: Arc::new(config_path),
            recording: RecordingState::new(),
        }
    }

    /// pst-core の API に渡せる形に変換する。短時間だけロックを取り
    /// 必要なフィールドを clone してから endpoint を組み立てる。
    pub async fn endpoint(&self) -> PeerCastEndpoint {
        let cfg = self.cfg.read().await;
        let auth = match (
            cfg.peercast.auth_user.as_deref(),
            cfg.peercast.auth_pass.as_deref(),
        ) {
            (Some(u), Some(p)) if !u.is_empty() => Some(BasicAuth {
                user: u.to_string(),
                pass: p.to_string(),
            }),
            _ => None,
        };
        PeerCastEndpoint {
            host: cfg.peercast.host.clone(),
            port: cfg.peercast.port,
            auth,
        }
    }
}

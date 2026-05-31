//! axum 各ハンドラに inject される共有 state。

use std::sync::Arc;

use pst_core::peercast::types::{BasicAuth, PeerCastEndpoint};

use crate::config::Config;
use crate::recording::RecordingState;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub recording: Arc<RecordingState>,
}

impl AppState {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg: Arc::new(cfg),
            recording: RecordingState::new(),
        }
    }

    /// pst-core の API に渡せる形に変換する。Basic 認証は config に
    /// あれば付ける。
    pub fn endpoint(&self) -> PeerCastEndpoint {
        let auth = match (
            self.cfg.peercast.auth_user.as_deref(),
            self.cfg.peercast.auth_pass.as_deref(),
        ) {
            (Some(u), Some(p)) if !u.is_empty() => Some(BasicAuth {
                user: u.to_string(),
                pass: p.to_string(),
            }),
            _ => None,
        };
        PeerCastEndpoint {
            host: self.cfg.peercast.host.clone(),
            port: self.cfg.peercast.port,
            auth,
        }
    }
}

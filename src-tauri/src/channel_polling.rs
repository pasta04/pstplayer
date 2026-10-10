//! バックエンド側でチャンネル状態をポーリングし、Tauri event として
//! フロントに配るための小さなランタイム。
//!
//! 目的は機能ではなく整理: フロントの setInterval を 1 箇所にまとめ、
//! 詳細モーダル / 将来の追加ウィンドウ等が同じ event を listen できる
//! 形にする。外側のトラフィック (PeerCast への HTTP) は従来通り。

use std::sync::Mutex;
use std::time::Duration;

use pst_core::peercast::{client, types::PeerCastEndpoint};
use tauri::{AppHandle, Emitter, Runtime};

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// 1 件だけ走るポーラータスクのハンドル。再 start 時は古いタスクを
/// abort してから差し替える。
#[derive(Default)]
pub struct ChannelPolling {
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl ChannelPolling {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start<R: Runtime>(
        &self,
        app: AppHandle<R>,
        endpoint: PeerCastEndpoint,
        channel_id: String,
    ) {
        self.stop();
        let task = tokio::spawn(async move {
            let mut tick = tokio::time::interval(POLL_INTERVAL);
            // 即時に 1 回打つ + その後 5 秒ごと。
            loop {
                tick.tick().await;
                if let Ok(status) = client::fetch_status(&endpoint, &channel_id).await {
                    let _ = app.emit("channel:status", &status);
                }
            }
        });
        *self.handle.lock().expect("polling lock") = Some(task);
    }

    pub fn stop(&self) {
        if let Some(handle) = self.handle.lock().expect("polling lock").take() {
            handle.abort();
        }
    }
}

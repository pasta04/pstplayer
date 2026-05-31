//! Recording task — 上流 PeerCastStation の生 stream (`/stream/{id}.flv`)
//! を tokio task でファイルに書き出す。
//!
//! 設計メモ:
//! * 同時録画は **1 本だけ**。すでに走っているタスクがあれば 409。
//! * ファイル名は `YYYYMMDD_HHmmss_<channel_name>.<ext>` (pst-core の
//!   `snapshot::make_filename` と同じ規則)。
//! * ディスク書き込みは録画タスクが直接 `tokio::fs::File::write_all` で
//!   行う。HLS プロキシのような透過 stream ではなく、ユーザーの意図で
//!   開始した「書き出し」なので SD カード保護方針と矛盾しない。
//! * stop は `JoinHandle::abort()` で即時切る。書き出し中のチャンク
//!   は最後まで flush できないが、FLV/MKV は途中で切れても多くの
//!   プレイヤーで再生可能。

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use pst_core::snapshot::make_filename;
use pst_core::util::http::CLIENT;
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::error::ApiError;
use crate::state::AppState;

/// AppState に Arc<Mutex<>> で持たせる録画タスクの状態。
#[derive(Default)]
pub struct RecordingState {
    /// 録画中なら Some。stop / 完了時に None に戻す。
    inner: Mutex<Option<RecordingTask>>,
}

pub struct RecordingTask {
    pub path: PathBuf,
    pub channel_id: String,
    pub channel_name: String,
    /// abort 用ハンドル。録画タスクが正常終了したらこれを参照しない。
    pub handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RecordingStatus {
    pub recording: bool,
    pub path: Option<String>,
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
}

impl RecordingState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn status(&self) -> RecordingStatus {
        let guard = self.inner.lock().await;
        match guard.as_ref() {
            Some(t) => RecordingStatus {
                recording: true,
                path: Some(t.path.to_string_lossy().into_owned()),
                channel_id: Some(t.channel_id.clone()),
                channel_name: Some(t.channel_name.clone()),
            },
            None => RecordingStatus {
                recording: false,
                path: None,
                channel_id: None,
                channel_name: None,
            },
        }
    }

    /// 録画開始。すでに進行中なら `Conflict` を返す。
    pub async fn start(
        &self,
        state: &AppState,
        channel_id: String,
        channel_name: String,
    ) -> Result<RecordingStatus, ApiError> {
        let cfg = &state.cfg.recording;
        if !cfg.enabled {
            return Err(ApiError {
                status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
                code: "recording_disabled",
                message: "[recording] enabled = false なので録画機能は無効です".into(),
            });
        }
        let dir = cfg.dir.trim();
        if dir.is_empty() {
            return Err(ApiError {
                status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
                code: "recording_disabled",
                message: "[recording] dir が空なので録画機能は無効です".into(),
            });
        }
        let dir = PathBuf::from(dir);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ApiError {
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                code: "recording_dir_create_failed",
                message: format!("録画ディレクトリを作れません ({}): {e}", dir.display()),
            })?;
        let raw_ext = cfg.ext.trim().trim_start_matches('.');
        let ext = if raw_ext.is_empty() { "flv" } else { raw_ext };
        let filename = make_filename(&channel_name, ext);
        let path = dir.join(&filename);

        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return Err(ApiError {
                status: axum::http::StatusCode::CONFLICT,
                code: "recording_busy",
                message: "別の録画が進行中です。先に /api/record/stop を呼んでください".into(),
            });
        }

        let upstream = format!(
            "http://{}:{}/stream/{}.{}",
            state.cfg.peercast.host, state.cfg.peercast.port, channel_id, ext
        );
        let auth_user = state.cfg.peercast.auth_user.clone();
        let auth_pass = state.cfg.peercast.auth_pass.clone();
        let path_for_task = path.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = run_recording(upstream, path_for_task, auth_user, auth_pass).await {
                eprintln!("recording task failed: {e}");
            }
        });

        let task = RecordingTask {
            path: path.clone(),
            channel_id: channel_id.clone(),
            channel_name: channel_name.clone(),
            handle,
        };
        *guard = Some(task);
        Ok(RecordingStatus {
            recording: true,
            path: Some(path.to_string_lossy().into_owned()),
            channel_id: Some(channel_id),
            channel_name: Some(channel_name),
        })
    }

    /// 録画停止。進行中でなければ no-op。
    pub async fn stop(&self) -> RecordingStatus {
        let mut guard = self.inner.lock().await;
        if let Some(task) = guard.take() {
            task.handle.abort();
        }
        RecordingStatus {
            recording: false,
            path: None,
            channel_id: None,
            channel_name: None,
        }
    }
}

/// PeerCast の生 stream を 1 本受けて指定パスに書き出す。エラーは
/// 呼び出し側で `eprintln!` される (録画タスクは abort/終了するだけ)。
async fn run_recording(
    upstream: String,
    path: PathBuf,
    auth_user: Option<String>,
    auth_pass: Option<String>,
) -> Result<(), String> {
    let mut req = CLIENT.get(&upstream);
    if let (Some(u), Some(p)) = (auth_user.as_deref(), auth_pass.as_deref()) {
        if !u.is_empty() {
            req = req.basic_auth(u, Some(p));
        }
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("upstream returned {}", resp.status()));
    }
    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| e.to_string())?;
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

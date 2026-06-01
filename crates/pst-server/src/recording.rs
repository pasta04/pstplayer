//! Recording task — 上流 PeerCastStation の生 stream を tokio task
//! でファイルに書き出す。同時に複数本録画可能。
//!
//! 設計メモ:
//! * 同時録画は config の `recording.max_concurrent` で上限指定可。
//!   既定 8 本。1 ホスト 1 チャンネル 1 本まで (同一 channel_id を
//!   二重開始すると 409)。
//! * ファイル名は `YYYYMMDD_HHmmss_<channel_name>.<ext>` (pst-core の
//!   `snapshot::make_filename` と同じ規則)。
//! * ディスク書き込みは録画タスクが直接 `tokio::fs::File::write_all` で
//!   行う。ユーザー意図の write なので SD カード保護方針と矛盾しない。
//! * stop は `JoinHandle::abort()` で即時切る。FLV/MKV は途中で切れても
//!   多くのプレイヤーで再生可能。

use std::collections::HashMap;
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

const DEFAULT_MAX_CONCURRENT: u32 = 8;

/// AppState に Arc で持たせる録画タスク群の状態。
#[derive(Default)]
pub struct RecordingState {
    /// channel_id をキーにした録画タスクの集合。
    inner: Mutex<HashMap<String, RecordingTask>>,
}

pub struct RecordingTask {
    pub path: PathBuf,
    pub channel_name: String,
    /// graceful 停止用フラグ。stop() で true にセットすると、録画ループは
    /// 次のチャンク境界で抜けて file.flush() してから終了する。
    pub stop_flag: Arc<std::sync::atomic::AtomicBool>,
    /// 監視 / fallback abort 用ハンドル。
    pub handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RecordingEntry {
    pub channel_id: String,
    pub channel_name: String,
    pub path: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct RecordingList {
    pub recordings: Vec<RecordingEntry>,
}

impl RecordingState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn list(&self) -> RecordingList {
        let guard = self.inner.lock().await;
        let mut recordings: Vec<RecordingEntry> = guard
            .iter()
            .map(|(id, t)| RecordingEntry {
                channel_id: id.clone(),
                channel_name: t.channel_name.clone(),
                path: t.path.to_string_lossy().into_owned(),
            })
            .collect();
        recordings.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        RecordingList { recordings }
    }

    /// 指定 channel_id の録画が進行中か。自動録画タスクが「既に録画中
    /// なら start を投げない」判定に使う。
    pub async fn is_recording(&self, channel_id: &str) -> bool {
        self.inner.lock().await.contains_key(channel_id)
    }

    /// 録画開始。同 channel_id が既に進行中なら 409。最大本数 (config
    /// `max_concurrent`) を超えていたら 429。
    pub async fn start(
        &self,
        state: &AppState,
        channel_id: String,
        channel_name: String,
    ) -> Result<RecordingEntry, ApiError> {
        // クリティカルセクションを短く保つため、必要な設定だけ clone
        // してから RwLock を解放する。
        let (rec_cfg, peercast_host, peercast_port, auth_user, auth_pass) = {
            let cfg = state.cfg.read().await;
            (
                cfg.recording.clone(),
                cfg.peercast.host.clone(),
                cfg.peercast.port,
                cfg.peercast.auth_user.clone(),
                cfg.peercast.auth_pass.clone(),
            )
        };
        if !rec_cfg.enabled {
            return Err(ApiError {
                status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
                code: "recording_disabled",
                message: "[recording] enabled = false なので録画機能は無効です".into(),
            });
        }
        let dir = rec_cfg.dir.trim();
        if dir.is_empty() {
            return Err(ApiError {
                status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
                code: "recording_disabled",
                message: "[recording] dir が空なので録画機能は無効です".into(),
            });
        }
        let max = if rec_cfg.max_concurrent == 0 {
            DEFAULT_MAX_CONCURRENT
        } else {
            rec_cfg.max_concurrent
        };
        let dir = PathBuf::from(dir);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ApiError {
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                code: "recording_dir_create_failed",
                message: format!("録画ディレクトリを作れません ({}): {e}", dir.display()),
            })?;
        let raw_ext = rec_cfg.ext.trim().trim_start_matches('.');
        let ext = if raw_ext.is_empty() { "flv" } else { raw_ext };
        let filename = make_filename(&channel_name, ext);
        let path = dir.join(&filename);

        let mut guard = self.inner.lock().await;
        if guard.contains_key(&channel_id) {
            return Err(ApiError {
                status: axum::http::StatusCode::CONFLICT,
                code: "recording_busy",
                message: format!("チャンネル {channel_id} は既に録画中です"),
            });
        }
        if guard.len() as u32 >= max {
            return Err(ApiError {
                status: axum::http::StatusCode::TOO_MANY_REQUESTS,
                code: "recording_limit",
                message: format!(
                    "同時録画の上限 ({max}) に達しています。停止してから再試行してください"
                ),
            });
        }

        let upstream = format!("http://{peercast_host}:{peercast_port}/stream/{channel_id}.{ext}");
        let path_for_task = path.clone();
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag_task = stop_flag.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) =
                run_recording(upstream, path_for_task, auth_user, auth_pass, stop_flag_task).await
            {
                eprintln!("recording task failed: {e}");
            }
        });

        let task = RecordingTask {
            path: path.clone(),
            channel_name: channel_name.clone(),
            stop_flag,
            handle,
        };
        guard.insert(channel_id.clone(), task);
        Ok(RecordingEntry {
            channel_id,
            channel_name,
            path: path.to_string_lossy().into_owned(),
        })
    }

    /// 特定 channel_id の録画を停止。進行中でなければ no-op。
    ///
    /// graceful shutdown: stop_flag をセットして次チャンク受信境界で
    /// 録画ループが抜けて `file.flush()` してから終了する。fallback
    /// として `abort()` も呼ぶが、これは upstream stream がハングした
    /// 場合の保険 (graceful 抜けが効くケースではほぼ flush 後 abort)。
    pub async fn stop(&self, channel_id: &str) -> bool {
        let mut guard = self.inner.lock().await;
        if let Some(task) = guard.remove(channel_id) {
            task.stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            // 一定時間 graceful 完了を待つ余裕を与えてから abort
            // (上流が stuck していた場合のみ abort が実効する)。
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                task.handle.abort();
            });
            true
        } else {
            false
        }
    }

    /// 全録画を停止。
    pub async fn stop_all(&self) -> usize {
        let mut guard = self.inner.lock().await;
        let n = guard.len();
        for (_, task) in guard.drain() {
            task.stop_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                task.handle.abort();
            });
        }
        n
    }
}

/// PeerCast の生 stream を 1 本受けて指定パスに書き出す。エラーは
/// 呼び出し側で `eprintln!` される (録画タスクは abort/終了するだけ)。
///
/// `stop_flag` が外部から true にされたら、次のチャンク境界で抜けて
/// `file.flush()` してから戻る (graceful shutdown)。これで FLV/MKV
/// の writer buffer が確実に flush され、途中切断によるファイル末尾
/// 欠損を最小化する。
async fn run_recording(
    upstream: String,
    path: PathBuf,
    auth_user: Option<String>,
    auth_pass: Option<String>,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
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
        if stop_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let bytes = chunk.map_err(|e| e.to_string())?;
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

//! 自動配信録画タスク。
//!
//! 設計 (docs/decisions/0006-auto-record-and-multiview.md):
//! * `getChannels` を `recording.auto_poll_interval_sec` (既定 60) 秒
//!   間隔で polling
//! * 各チャンネルを `favorites.rules` で評価し、最初にマッチしたルール
//!   が `auto_record = true` なら録画を開始する
//! * 既に録画中のチャンネルは再開しない (RecordingState 側で 409 で
//!   弾かれるが、こちらでも early-return することで余計なログを出さない)
//! * 1 周期で見えていた配信が次周期で消えたら、`recording.auto_stop_grace_sec`
//!   (既定 30) 秒の grace を入れて停止する。短時間の瞬断で録画が分割
//!   されないようにするため
//! * `[recording] enabled = false` の時は polling を回さない (CPU 浪費
//!   防止)。runtime に true へ変えられた場合は次回の起動まで反映されない
//!   が、許容範囲 (Web UI から自動録画 ON / OFF を頻繁に切り替える運用
//!   は想定していない)
//!
//! 過剰な防衛は入れない:
//! * `get_channels` が失敗したら ログだけ吐いて次周期を待つ
//! * grace 中に再度見えたら停止予定をキャンセル (HashMap から消す)

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use pst_core::favorites::first_match;
use pst_core::peercast::jsonrpc::get_channels;

use crate::state::AppState;

const DEFAULT_POLL_INTERVAL_SEC: u64 = 60;
const DEFAULT_STOP_GRACE_SEC: u64 = 30;

/// `pst-server` 起動時に 1 度だけ呼ぶ。tokio task を生やして JoinHandle を
/// 返す (in-process 埋め込み時に host から abort できるように)。返り値を
/// 無視すれば従来どおり detach されてタスクは生き続ける。
pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run(state).await;
    })
}

async fn run(state: AppState) {
    // channel_id -> 「消えてから止めるまでの deadline」。grace 中の
    // チャンネルだけが入る。再度現れたら remove して停止をキャンセル。
    let mut pending_stops: HashMap<String, Instant> = HashMap::new();

    loop {
        // 各周期で設定を読み直す (PUT /api/config で書き換えられた時に
        // 次周期から反映される)。
        let (enabled, interval, grace, rules) = {
            let cfg = state.cfg.read().await;
            (
                cfg.recording.enabled,
                interval_or_default(cfg.recording.auto_poll_interval_sec),
                grace_or_default(cfg.recording.auto_stop_grace_sec),
                cfg.favorites.rules.clone(),
            )
        };

        if !enabled {
            // 録画機能 OFF のうちは polling を回す意味がない。
            tokio::time::sleep(interval).await;
            continue;
        }

        let has_auto_rule = rules.iter().any(|r| r.auto_record);
        if !has_auto_rule {
            // auto_record=true のルールが 1 つも無いなら polling 不要。
            tokio::time::sleep(interval).await;
            continue;
        }

        let endpoint = state.endpoint().await;
        match get_channels(&endpoint).await {
            Ok(channels) => {
                let mut seen: HashSet<String> = HashSet::new();
                for ch in &channels {
                    if ch.channel_id.is_empty() {
                        continue;
                    }
                    seen.insert(ch.channel_id.clone());

                    // grace 中だったチャンネルが復活したら停止予定を取消。
                    pending_stops.remove(&ch.channel_id);

                    // auto_record ルールにマッチするか。
                    let Some(rule) = first_match(&rules, &ch.info) else {
                        continue;
                    };
                    // 最初にマッチしたルールが Block / Ignore なら録画
                    // しない。auto_record=true な別ルールが下にあっても
                    // 優先順位上位のものを尊重する。
                    if rule.action != pst_core::favorites::FavoriteAction::Show {
                        continue;
                    }
                    if !rule.auto_record {
                        continue;
                    }

                    // 録画開始。既に進行中なら recording_busy になるので
                    // ログを抑制 (毎周期 409 が出るのを避ける)。
                    if state.recording.is_recording(&ch.channel_id).await {
                        continue;
                    }
                    let name = if ch.info.name.is_empty() {
                        ch.channel_id.clone()
                    } else {
                        ch.info.name.clone()
                    };
                    match state
                        .recording
                        // 自動録画の対象はローカル PeerCast が既にリレー中の
                        // チャンネルなので tip は不要。
                        .start(&state, ch.channel_id.clone(), name.clone(), None)
                        .await
                    {
                        Ok(entry) => {
                            tracing::info!(
                                "auto-record started: id={} name={} path={}",
                                entry.channel_id,
                                entry.channel_name,
                                entry.path
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "auto-record start failed: id={} name={} code={} msg={}",
                                ch.channel_id,
                                name,
                                e.code,
                                e.message
                            );
                        }
                    }
                }

                // 録画中だが今周期に見えなかったチャンネルに停止予定を
                // 立てる。既に予定済みのものはそのまま放置。
                let recordings = state.recording.list().await;
                let now = Instant::now();
                for entry in &recordings.recordings {
                    if seen.contains(&entry.channel_id) {
                        continue;
                    }
                    pending_stops
                        .entry(entry.channel_id.clone())
                        .or_insert_with(|| now + grace);
                }

                // grace 切れの停止を実行。
                let mut expired: Vec<String> = Vec::new();
                for (id, deadline) in pending_stops.iter() {
                    if now >= *deadline {
                        expired.push(id.clone());
                    }
                }
                for id in expired {
                    pending_stops.remove(&id);
                    if state.recording.stop(&id).await {
                        tracing::info!("auto-record stopped (gone): id={id}");
                    }
                }
            }
            Err(e) => {
                tracing::warn!("auto-record: get_channels failed: {e}");
            }
        }

        tokio::time::sleep(interval).await;
    }
}

fn interval_or_default(v: u64) -> Duration {
    if v == 0 {
        Duration::from_secs(DEFAULT_POLL_INTERVAL_SEC)
    } else {
        Duration::from_secs(v)
    }
}

fn grace_or_default(v: u64) -> Duration {
    if v == 0 {
        Duration::from_secs(DEFAULT_STOP_GRACE_SEC)
    } else {
        Duration::from_secs(v)
    }
}

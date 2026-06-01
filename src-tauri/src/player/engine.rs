//! libmpv FFI wrapper + 自動再接続イベントループ。
//!
//! 公開 API はチャンネル load / stop / pause / property 等の同期メソッド。
//! 内部で 1 本のイベントループスレッドが `mpv_wait_event` を回しており、
//! end_file (= 再生終了) を受信したら設定 + 状態を見て自動再接続するか
//! どうかを判定する。詳細な再接続ポリシーは [`decide_reconnect`] を参照。
//!
//! 再接続判定は **純関数** `decide_reconnect` に分離してあり、libmpv
//! 無しでユニットテスト可能。

use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use libmpv2::{GetData, Mpv};
use pst_core::util::errors::{AppError, AppResult};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

// ── 再接続ポリシー (定数) ─────────────────────────────────────

/// 最大リトライ回数。これを超えたら諦めてステータス帯に通知。
pub const MAX_ATTEMPTS: u32 = 10;
/// 再接続シーケンス全体の総タイムアウト。
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(300);
/// 「再接続後にすぐ EndFile が来た = 即切断」と判定する閾値。
pub const IMMEDIATE_DISCONNECT_THRESHOLD: Duration = Duration::from_secs(3);
/// 即切断がこの回数連続したら「配信元が本当に終わった」と推定して打ち切る。
pub const MAX_IMMEDIATE_DISCONNECTS: u32 = 3;

// ── libmpv end_file の reason 値 (libmpv2_sys を経由) ─────────

const REASON_EOF: u32 = libmpv2_sys::mpv_end_file_reason_MPV_END_FILE_REASON_EOF;
const REASON_STOP: u32 = libmpv2_sys::mpv_end_file_reason_MPV_END_FILE_REASON_STOP;
const REASON_QUIT: u32 = libmpv2_sys::mpv_end_file_reason_MPV_END_FILE_REASON_QUIT;
const REASON_ERROR: u32 = libmpv2_sys::mpv_end_file_reason_MPV_END_FILE_REASON_ERROR;
const REASON_REDIRECT: u32 = libmpv2_sys::mpv_end_file_reason_MPV_END_FILE_REASON_REDIRECT;

const EVENT_NONE: u32 = libmpv2_sys::mpv_event_id_MPV_EVENT_NONE;
const EVENT_SHUTDOWN: u32 = libmpv2_sys::mpv_event_id_MPV_EVENT_SHUTDOWN;
const EVENT_END_FILE: u32 = libmpv2_sys::mpv_event_id_MPV_EVENT_END_FILE;

/// libmpv が EndFile で返す reason の人間可読化。Tauri event payload に
/// 含めて、ステータス帯 / 開発者ログで表示する。
pub fn reason_label(reason: u32) -> &'static str {
    match reason {
        REASON_EOF => "eof",
        REASON_STOP => "stop",
        REASON_QUIT => "quit",
        REASON_ERROR => "error",
        REASON_REDIRECT => "redirect",
        _ => "unknown",
    }
}

// ── 状態 ──────────────────────────────────────────────────────

/// PlayerEngine の再接続状態。Mutex で守られる。`decide_reconnect` には
/// この値の参照を渡して判定する (純関数化)。
#[derive(Debug, Default, Clone)]
pub struct EngineState {
    /// 直近 load() した URL。再接続時に reload する対象。
    pub last_url: Option<String>,
    /// 現在の再接続シーケンスでの試行回数 (0 = リトライしてない / 新規)。
    pub attempts: u32,
    /// 現在の再接続シーケンスを開始した時刻 (総タイムアウト判定用)。
    pub series_started_at: Option<Instant>,
    /// 直近の load() / reload() を発行した時刻 (即切断判定用)。
    pub last_load_at: Option<Instant>,
    /// 即切断 (3 秒以内の EndFile) が連続した回数。
    pub immediate_disconnects: u32,
    /// 直前のユーザ操作が stop だった場合に true。次の EndFile を無視する。
    pub user_stop: bool,
    /// config.player.auto_reconnect の現在値 (キャッシュ)。
    pub enabled: bool,
}

// ── 判定結果 ──────────────────────────────────────────────────

/// 再接続を試みない理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// ユーザが手動で stop した直後の EndFile。
    UserStop,
    /// reason が STOP / QUIT で、再接続対象外。
    NotReconnectable,
    /// config で auto_reconnect が無効 (= 観察モード)。
    Disabled,
    /// 直近に load() した URL を持っていない。
    NoUrl,
    /// 最大リトライ回数に達した。
    MaxAttempts,
    /// 総タイムアウトに達した。
    TotalTimeout,
    /// 即切断が連続した = 配信元が本当に終わったと推定。
    BroadcastLikelyEnded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectDecision {
    Skip { reason: SkipReason },
    Retry { attempt: u32, delay: Duration },
}

// ── 純関数: 判定ロジック ──────────────────────────────────────

/// 与えられた state と end_file の reason、現在時刻から、再接続するか
/// どうかを判定する。state は変更しない。
///
/// 判定の優先順位は次の通り:
/// 1. ユーザが直前に stop した → 無視
/// 2. reason が STOP / QUIT → 無視
/// 3. config disabled → 無視 (= 観察モード)
/// 4. last_url 不在 → 無視
/// 5. 即切断連続 3 回 → 配信終了とみなす
/// 6. 最大試行回数 / 総タイムアウト → 諦め
/// 7. 上記すべてクリア → バックオフ計算して Retry
pub fn decide_reconnect(s: &EngineState, end_file_reason: u32, now: Instant) -> ReconnectDecision {
    if s.user_stop {
        return ReconnectDecision::Skip { reason: SkipReason::UserStop };
    }
    if end_file_reason == REASON_STOP || end_file_reason == REASON_QUIT {
        return ReconnectDecision::Skip { reason: SkipReason::NotReconnectable };
    }
    if !s.enabled {
        return ReconnectDecision::Skip { reason: SkipReason::Disabled };
    }
    if s.last_url.is_none() {
        return ReconnectDecision::Skip { reason: SkipReason::NoUrl };
    }

    // 直近の load() からの経過時間で即切断判定。3 秒以下なら「すぐ
    // 切れた」とみなして即切断カウンタを +1 する想定の試算をする
    // (実際の +1 は呼び出し側が apply 時に行う)。
    let immediate = s
        .last_load_at
        .map(|l| now.duration_since(l) <= IMMEDIATE_DISCONNECT_THRESHOLD)
        .unwrap_or(false);
    if immediate && s.immediate_disconnects + 1 >= MAX_IMMEDIATE_DISCONNECTS {
        return ReconnectDecision::Skip { reason: SkipReason::BroadcastLikelyEnded };
    }

    let next_attempt = s.attempts + 1;
    if next_attempt > MAX_ATTEMPTS {
        return ReconnectDecision::Skip { reason: SkipReason::MaxAttempts };
    }
    if let Some(started) = s.series_started_at {
        if now.duration_since(started) > TOTAL_TIMEOUT {
            return ReconnectDecision::Skip { reason: SkipReason::TotalTimeout };
        }
    }

    ReconnectDecision::Retry { attempt: next_attempt, delay: backoff_delay(next_attempt) }
}

/// 試行回数 → 待機時間。1, 2, 4, 8, 16, 30, 30, 30, 30, 30 秒。
pub fn backoff_delay(attempt: u32) -> Duration {
    let secs = match attempt {
        1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        5 => 16,
        _ => 30,
    };
    Duration::from_secs(secs)
}

// ── Tauri event payload 型 ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct EndFileObservedPayload {
    pub reason: String,
    pub auto_reconnect_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconnectingPayload {
    pub attempt: u32,
    pub max: u32,
    pub delay_sec: u64,
    pub end_file_reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconnectStoppedPayload {
    pub reason: SkipReason,
    pub end_file_reason: String,
}

// ── Mpv ハンドルを別スレッドへ送るための Send wrapper ────────

/// libmpv の生ハンドル。`mpv_wait_event` を別スレッドで呼ぶために
/// `Send` を実装する。libmpv は内部で同期しており、`mpv_wait_event`
/// は 1 スレッドからのみ呼ぶ前提で、その制約は本モジュールが
/// 「event_loop スレッドだけが wait_event する」ことで満たす。
#[derive(Copy, Clone)]
struct MpvCtx(NonNull<libmpv2_sys::mpv_handle>);
// SAFETY: libmpv のクライアント API はスレッドセーフ (mpv の C
// ドキュメントに明記)。
unsafe impl Send for MpvCtx {}

// ── PlayerEngine 本体 ─────────────────────────────────────────

#[derive(Clone)]
pub struct PlayerEngine {
    mpv: Arc<Mpv>,
    state: Arc<Mutex<EngineState>>,
}

impl PlayerEngine {
    /// libmpv を初期化する。`force-window=no` / `idle=yes` で「ウィンドウは
    /// 後で attach する」モードになる。
    pub fn new() -> AppResult<Self> {
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("force-window", "no").ok();
            init.set_option("idle", "yes").ok();
            init.set_option("keepaspect", "yes").ok();
            init.set_option("video-unscaled", "no").ok();
            init.set_option("terminal", "no").ok();
            init.set_option("msg-level", "all=warn").ok();
            init.set_option("cache", "yes").ok();
            init.set_option("demuxer-max-bytes", "10MiB").ok();
            init.set_option("demuxer-max-back-bytes", "5MiB").ok();
            Ok(())
        })
        .map_err(map_err)?;
        Ok(Self { mpv: Arc::new(mpv), state: Arc::new(Mutex::new(EngineState::default())) })
    }

    /// Load `url` and start playback (`replace` the current entry).
    /// state の再接続シーケンスをリセットする。
    pub fn load(&self, url: &str) -> AppResult<()> {
        {
            let mut s = self.state.lock().expect("engine state lock");
            s.last_url = Some(url.to_string());
            s.attempts = 0;
            s.series_started_at = None;
            s.last_load_at = Some(Instant::now());
            s.immediate_disconnects = 0;
            s.user_stop = false;
        }
        self.mpv.command("loadfile", &[url, "replace"]).map_err(map_err)
    }

    /// Stop playback. `user_stop` フラグを立てて、続いて来る EndFile を
    /// 無視させる。
    pub fn stop(&self) -> AppResult<()> {
        {
            let mut s = self.state.lock().expect("engine state lock");
            s.user_stop = true;
            s.last_url = None;
            s.attempts = 0;
            s.series_started_at = None;
            s.immediate_disconnects = 0;
        }
        self.mpv.command("stop", &[]).map_err(map_err)
    }

    /// config.player.auto_reconnect の値を反映する。フロントの設定
    /// ダイアログから即時更新するため Tauri command として公開する。
    pub fn set_auto_reconnect(&self, enabled: bool) {
        self.state.lock().expect("engine state lock").enabled = enabled;
    }

    pub fn set_pause(&self, pause: bool) -> AppResult<()> {
        self.mpv.set_property("pause", pause).map_err(map_err)
    }

    pub fn set_volume(&self, percent: u8) -> AppResult<()> {
        let v = percent.min(200) as i64;
        self.mpv.set_property("volume", v).map_err(map_err)
    }

    pub fn set_mute(&self, mute: bool) -> AppResult<()> {
        self.mpv.set_property("mute", mute).map_err(map_err)
    }

    pub fn get_property<T: GetData>(&self, name: &str) -> Option<T> {
        self.mpv.get_property::<T>(name).ok()
    }

    pub fn screenshot_to_file(&self, path: &str, flag: &str) -> AppResult<()> {
        self.mpv.command("screenshot-to-file", &[path, flag]).map_err(map_err)
    }

    /// 録画開始。空文字列なら録画停止と同じ意味。
    pub fn start_record(&self, path: &str) -> AppResult<()> {
        self.mpv.set_property("stream-record", path).map_err(map_err)
    }

    pub fn stop_record(&self) -> AppResult<()> {
        self.mpv.set_property("stream-record", "").map_err(map_err)
    }

    pub fn record_path(&self) -> Option<String> {
        self.mpv.get_property::<String>("stream-record").ok().filter(|s| !s.is_empty())
    }

    pub fn set_aspect(&self, ratio: f64) -> AppResult<()> {
        let v = format!("{ratio}");
        self.mpv.set_property("video-aspect-override", v.as_str()).map_err(map_err)
    }

    pub fn handle(&self) -> Arc<Mpv> {
        Arc::clone(&self.mpv)
    }

    /// 自動再接続用のイベントループを別スレッドで起動する。Tauri
    /// AppHandle を引数に取り、状態遷移を Tauri event でフロントに伝える。
    /// 多重 attach は no-op (= 1 度だけ呼ぶ想定。冪等にしたい場合は要追加)。
    pub fn attach_event_loop<R: Runtime + 'static>(&self, app: AppHandle<R>) {
        let ctx = MpvCtx(self.mpv.ctx);
        let mpv = Arc::clone(&self.mpv);
        let state = Arc::clone(&self.state);
        thread::Builder::new()
            .name("pst-mpv-events".into())
            .spawn(move || event_loop(ctx, mpv, state, app))
            .expect("spawn mpv event loop");
    }
}

fn map_err(e: libmpv2::Error) -> AppError {
    AppError::Network(format!("mpv: {e}"))
}

// ── イベントループ ────────────────────────────────────────────

fn event_loop<R: Runtime + 'static>(
    ctx: MpvCtx,
    mpv: Arc<Mpv>,
    state: Arc<Mutex<EngineState>>,
    app: AppHandle<R>,
) {
    loop {
        // -1.0 = 無期限に blocking で待つ。libmpv2 の wait_event は
        // &mut self を要求するが、内部で行うのは ctx ポインタの
        // mpv_wait_event 呼び出しだけなので sys API を直接叩く。
        let event_ptr = unsafe { libmpv2_sys::mpv_wait_event(ctx.0.as_ptr(), -1.0) };
        if event_ptr.is_null() {
            continue;
        }
        let event = unsafe { *event_ptr };
        match event.event_id {
            id if id == EVENT_NONE => continue,
            id if id == EVENT_SHUTDOWN => break,
            id if id == EVENT_END_FILE => {
                let end_file = unsafe { *(event.data as *const libmpv2_sys::mpv_event_end_file) };
                handle_end_file(end_file.reason as u32, &state, &mpv, &app);
            }
            _ => continue,
        }
    }
}

fn handle_end_file<R: Runtime + 'static>(
    reason: u32,
    state: &Mutex<EngineState>,
    mpv: &Arc<Mpv>,
    app: &AppHandle<R>,
) {
    let now = Instant::now();
    let snapshot = state.lock().expect("engine state lock").clone();
    let decision = decide_reconnect(&snapshot, reason, now);

    // 観察モードでも reason は常に通知 (config OFF / 配信終了などのケース
    // でユーザが「何が起きたか」を把握できるように)。
    let _ = app.emit(
        "player:end_file_observed",
        EndFileObservedPayload {
            reason: reason_label(reason).to_string(),
            auto_reconnect_enabled: snapshot.enabled,
        },
    );
    eprintln!(
        "player end_file: reason={} auto_reconnect={} decision={:?}",
        reason_label(reason),
        snapshot.enabled,
        decision
    );

    match decision {
        ReconnectDecision::Retry { attempt, delay } => {
            // state を mutate。即切断カウンタは「今が即切断だったか」を
            // 計算し直す (snapshot 取得後にユーザ操作で last_load_at が
            // 動いている可能性は低いが安全側で取り直す)。
            let url = {
                let mut s = state.lock().expect("engine state lock");
                // user_stop / disabled / no_url は Skip 経由なのでここに来ない。
                let is_immediate = s
                    .last_load_at
                    .map(|l| now.duration_since(l) <= IMMEDIATE_DISCONNECT_THRESHOLD)
                    .unwrap_or(false);
                if is_immediate {
                    s.immediate_disconnects += 1;
                } else {
                    // 「ちゃんと再生してたあとの切断」なのでシリーズを
                    // リセット (= 新たな再接続シーケンスの起点)。
                    s.immediate_disconnects = 0;
                    s.series_started_at = None;
                    s.attempts = 0;
                }
                if s.series_started_at.is_none() {
                    s.series_started_at = Some(now);
                }
                s.attempts = attempt;
                s.last_url.clone()
            };
            let Some(url) = url else {
                return;
            };

            let _ = app.emit(
                "player:reconnecting",
                ReconnectingPayload {
                    attempt,
                    max: MAX_ATTEMPTS,
                    delay_sec: delay.as_secs(),
                    end_file_reason: reason_label(reason).to_string(),
                },
            );

            thread::sleep(delay);

            // 待機中にユーザが stop した / config が OFF になった場合は
            // 再接続をキャンセル。
            {
                let s = state.lock().expect("engine state lock");
                if !s.enabled || s.user_stop || s.last_url.as_deref() != Some(url.as_str()) {
                    return;
                }
            }
            state.lock().expect("engine state lock").last_load_at = Some(Instant::now());
            if let Err(e) = mpv.command("loadfile", &[url.as_str(), "replace"]) {
                eprintln!("auto-reconnect loadfile failed: {e:?}");
            }
        }
        ReconnectDecision::Skip { reason: skip_reason } => {
            // user_stop は 1 回だけ消費。次のセッションのために false に戻す。
            {
                let mut s = state.lock().expect("engine state lock");
                s.user_stop = false;
                // 諦め系の Skip ではシリーズを完全リセット。
                if matches!(
                    skip_reason,
                    SkipReason::MaxAttempts
                        | SkipReason::TotalTimeout
                        | SkipReason::BroadcastLikelyEnded
                ) {
                    s.attempts = 0;
                    s.series_started_at = None;
                    s.immediate_disconnects = 0;
                }
            }
            let _ = app.emit(
                "player:reconnect_stopped",
                ReconnectStoppedPayload {
                    reason: skip_reason,
                    end_file_reason: reason_label(reason).to_string(),
                },
            );
        }
    }
}

// ── テスト ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn base_state() -> EngineState {
        EngineState {
            last_url: Some("http://x/pls/abc".into()),
            attempts: 0,
            series_started_at: None,
            last_load_at: Some(Instant::now()),
            immediate_disconnects: 0,
            user_stop: false,
            enabled: true,
        }
    }

    #[test]
    fn user_stop_always_skips() {
        let s = EngineState { user_stop: true, ..base_state() };
        for reason in [REASON_EOF, REASON_ERROR, REASON_REDIRECT, REASON_STOP, REASON_QUIT] {
            assert_eq!(
                decide_reconnect(&s, reason, t0()),
                ReconnectDecision::Skip { reason: SkipReason::UserStop },
                "reason={reason}"
            );
        }
    }

    #[test]
    fn stop_and_quit_reasons_skip() {
        let s = base_state();
        for reason in [REASON_STOP, REASON_QUIT] {
            assert_eq!(
                decide_reconnect(&s, reason, t0()),
                ReconnectDecision::Skip { reason: SkipReason::NotReconnectable }
            );
        }
    }

    #[test]
    fn observation_mode_skips_all_reasons() {
        let s = EngineState { enabled: false, ..base_state() };
        for reason in [REASON_EOF, REASON_ERROR, REASON_REDIRECT] {
            assert_eq!(
                decide_reconnect(&s, reason, t0()),
                ReconnectDecision::Skip { reason: SkipReason::Disabled }
            );
        }
    }

    #[test]
    fn missing_url_skips() {
        let s = EngineState { last_url: None, ..base_state() };
        assert_eq!(
            decide_reconnect(&s, REASON_EOF, t0()),
            ReconnectDecision::Skip { reason: SkipReason::NoUrl }
        );
    }

    #[test]
    fn eof_with_enabled_retries() {
        let s = base_state();
        // last_load_at は base_state で「ちょうど今」なので即切断扱い。
        // ただし最初の即切断なので閾値以下: immediate_disconnects + 1 = 1 < 3
        // → Retry が返るはず。
        match decide_reconnect(&s, REASON_EOF, Instant::now()) {
            ReconnectDecision::Retry { attempt, delay } => {
                assert_eq!(attempt, 1);
                assert_eq!(delay, Duration::from_secs(1));
            }
            d => panic!("expected Retry, got {d:?}"),
        }
    }

    #[test]
    fn error_reason_retries_same_as_eof() {
        let s = base_state();
        let d = decide_reconnect(&s, REASON_ERROR, Instant::now());
        assert!(matches!(d, ReconnectDecision::Retry { .. }));
    }

    #[test]
    fn redirect_retries() {
        let s = base_state();
        assert!(matches!(
            decide_reconnect(&s, REASON_REDIRECT, Instant::now()),
            ReconnectDecision::Retry { .. }
        ));
    }

    #[test]
    fn backoff_progression() {
        assert_eq!(backoff_delay(1), Duration::from_secs(1));
        assert_eq!(backoff_delay(2), Duration::from_secs(2));
        assert_eq!(backoff_delay(3), Duration::from_secs(4));
        assert_eq!(backoff_delay(4), Duration::from_secs(8));
        assert_eq!(backoff_delay(5), Duration::from_secs(16));
        assert_eq!(backoff_delay(6), Duration::from_secs(30));
        assert_eq!(backoff_delay(10), Duration::from_secs(30));
    }

    #[test]
    fn max_attempts_skips() {
        let s = EngineState { attempts: MAX_ATTEMPTS, ..base_state() };
        assert_eq!(
            decide_reconnect(&s, REASON_EOF, Instant::now()),
            ReconnectDecision::Skip { reason: SkipReason::MaxAttempts }
        );
    }

    #[test]
    fn total_timeout_skips() {
        let started = Instant::now() - TOTAL_TIMEOUT - Duration::from_secs(1);
        let s = EngineState {
            attempts: 3,
            series_started_at: Some(started),
            // last_load_at を昔にして即切断判定を回避。
            last_load_at: Some(Instant::now() - Duration::from_secs(60)),
            ..base_state()
        };
        assert_eq!(
            decide_reconnect(&s, REASON_EOF, Instant::now()),
            ReconnectDecision::Skip { reason: SkipReason::TotalTimeout }
        );
    }

    #[test]
    fn three_immediate_disconnects_marks_broadcast_ended() {
        // 即切断 2 回まではセーフ。3 回目で BroadcastLikelyEnded。
        let s = EngineState {
            immediate_disconnects: MAX_IMMEDIATE_DISCONNECTS - 1,
            last_load_at: Some(Instant::now()),
            ..base_state()
        };
        assert_eq!(
            decide_reconnect(&s, REASON_EOF, Instant::now()),
            ReconnectDecision::Skip { reason: SkipReason::BroadcastLikelyEnded }
        );
    }

    #[test]
    fn long_playback_then_disconnect_is_not_immediate() {
        // 5 分以上再生してから切断 → 即切断判定にならず Retry に進む。
        let long_ago = Instant::now() - Duration::from_secs(300);
        let s = EngineState {
            immediate_disconnects: 2, // 直前のシーケンスでの累積
            last_load_at: Some(long_ago),
            ..base_state()
        };
        assert!(matches!(
            decide_reconnect(&s, REASON_EOF, Instant::now()),
            ReconnectDecision::Retry { attempt: 1, .. }
        ));
    }

    /// libmpv 自体の初期化テスト。CI 環境で libmpv が無い場合は soft skip。
    #[test]
    fn engine_initialises() {
        match PlayerEngine::new() {
            Ok(engine) => {
                engine.set_volume(80).expect("set_volume");
                engine.set_mute(false).expect("set_mute");
            }
            Err(e) => {
                eprintln!("libmpv not available, skipping: {e}");
            }
        }
    }
}

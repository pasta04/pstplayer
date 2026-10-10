use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(30);

pub fn user_agent() -> String {
    format!("PSTPlayer/{}", env!("CARGO_PKG_VERSION"))
}

/// Shared HTTP client for general use (no cookies, gzip on).
///
/// **注意**: `timeout()` は接続開始〜レスポンスボディ読み終わりの
/// 「総時間」に効く。短い API / YP / playlist 取得向け。録画や HLS の
/// ように body をストリームし続ける用途で使うと 5 秒で切断されるので、
/// そちらは [`STREAM_CLIENT`] を使うこと。
pub static CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(user_agent())
        .timeout(DEFAULT_TIMEOUT)
        .build()
        .expect("failed to build default HTTP client")
});

/// 長時間ストリーミング用クライアント (録画 / HLS プロキシ)。
///
/// total timeout は付けない (付けると録画がその時間で必ず切れる)。
/// 代わりに:
/// - `connect_timeout`: 接続確立までの上限
/// - `read_timeout`: チャンク間の無通信上限。上流が固まった時に
///   いつまでも待ち続けない (録画の graceful stop は次チャンク境界で
///   しか効かないため、この read_timeout が実質の hung-stream 保険)
pub static STREAM_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(user_agent())
        .connect_timeout(STREAM_CONNECT_TIMEOUT)
        .read_timeout(STREAM_READ_TIMEOUT)
        .build()
        .expect("failed to build streaming HTTP client")
});

/// PeerCast 本体への API リクエストに使うタイムアウト (秒)。config の
/// `peercast.timeout_sec` を反映する。
///
/// [`CLIENT`] は `Lazy` なプロセス共有インスタンスで後から作り直せないため、
/// クライアント自体ではなく値だけをここに持ち、PeerCast 向けリクエストごとに
/// `RequestBuilder::timeout()` で上書きする (per-request 指定はクライアント
/// 既定より優先される)。BBS / YP など別サーバ向けの通信はこの設定の対象外。
static PEERCAST_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(DEFAULT_TIMEOUT_SECS);

/// 設定値を反映する。範囲外 (0 や極端に大きい値) は既定に倒す。設定 UI の
/// 入力範囲は 1〜60 秒だが、TOML 直接編集もあり得るので広めに許容する。
pub fn set_peercast_timeout_secs(secs: u64) {
    let v = if (1..=600).contains(&secs) {
        secs
    } else {
        DEFAULT_TIMEOUT_SECS
    };
    PEERCAST_TIMEOUT_SECS.store(v, Ordering::Relaxed);
}

/// PeerCast 向けリクエストに適用するタイムアウト。
pub fn peercast_timeout() -> Duration {
    Duration::from_secs(PEERCAST_TIMEOUT_SECS.load(Ordering::Relaxed))
}

/// Build a fresh client with cookie support; used by BBS modules that need
/// to retain Set-Cookie state across requests.
pub fn cookie_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("Monazilla/1.00 {}", user_agent()))
        .timeout(DEFAULT_TIMEOUT)
        .cookie_store(true)
        .build()
        .expect("failed to build cookie HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 設定値がそのまま反映され、範囲外は既定に倒れること。
    /// (この値は PeerCast 向けリクエストの `.timeout()` に使われる)
    #[test]
    fn peercast_timeout_follows_config_and_clamps() {
        set_peercast_timeout_secs(30);
        assert_eq!(peercast_timeout(), Duration::from_secs(30));

        // 0 / 極端に大きい値は既定へ。
        set_peercast_timeout_secs(0);
        assert_eq!(peercast_timeout(), DEFAULT_TIMEOUT);
        set_peercast_timeout_secs(100_000);
        assert_eq!(peercast_timeout(), DEFAULT_TIMEOUT);

        // 設定 UI の範囲 (1..=60) の両端。
        set_peercast_timeout_secs(1);
        assert_eq!(peercast_timeout(), Duration::from_secs(1));
        set_peercast_timeout_secs(60);
        assert_eq!(peercast_timeout(), Duration::from_secs(60));

        set_peercast_timeout_secs(DEFAULT_TIMEOUT_SECS);
    }
}

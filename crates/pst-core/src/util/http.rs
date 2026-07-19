use once_cell::sync::Lazy;
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
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

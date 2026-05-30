use once_cell::sync::Lazy;
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn user_agent() -> String {
    format!("PSTPlayer/{}", env!("CARGO_PKG_VERSION"))
}

/// Shared HTTP client for general use (no cookies, gzip on).
pub static CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .user_agent(user_agent())
        .timeout(DEFAULT_TIMEOUT)
        .build()
        .expect("failed to build default HTTP client")
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

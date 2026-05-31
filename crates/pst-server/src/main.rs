//! `pst-server` 実行バイナリ。設定を読み、axum サーバを起動する。
//!
//! 使い方:
//!
//! ```bash
//! pst-server                       # デフォルト config パスを使用
//! pst-server --config ./foo.toml   # 明示パス
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use pst_server::{
    config::{self, ConfigError},
    state::AppState,
};

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let explicit_config = parse_config_arg(&args);

    let cfg = match config::load(explicit_config.as_deref()) {
        Ok(c) => c,
        Err(ConfigError::Io { path, source }) => {
            tracing::error!("config read error: {} ({})", source, path.display());
            return ExitCode::from(2);
        }
        Err(ConfigError::Parse { path, source }) => {
            tracing::error!("config parse error: {} ({})", source, path.display());
            return ExitCode::from(2);
        }
    };
    tracing::info!(
        "starting pst-server: bind={} upstream={}:{}",
        cfg.server.bind,
        cfg.peercast.host,
        cfg.peercast.port
    );

    let bind = cfg.server.bind;
    let state = AppState::new(cfg);
    let app = pst_server::build_router(state);

    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("bind {bind} failed: {e}");
            return ExitCode::from(3);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("server error: {e}");
        return ExitCode::from(4);
    }
    ExitCode::SUCCESS
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("pst_server=info,axum=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// 簡素な `--config <path>` パーサ。MVP では他フラグは持たない。
fn parse_config_arg(args: &[String]) -> Option<PathBuf> {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--config" {
            return it.next().map(PathBuf::from);
        }
        if let Some(rest) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(rest));
        }
    }
    None
}

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
    config::{self, Config, ConfigError},
    state::AppState,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let explicit_config = parse_config_arg(&args);

    let cfg = match config::load(explicit_config.as_deref()) {
        Ok(c) => c,
        Err(ConfigError::Io { path, source }) => {
            // 起動前のクリティカルなエラーは stderr に 1 行だけ出す。
            eprintln!("config read error: {} ({})", source, path.display());
            return ExitCode::from(2);
        }
        Err(ConfigError::Parse { path, source }) => {
            eprintln!("config parse error: {} ({})", source, path.display());
            return ExitCode::from(2);
        }
    };

    // ログは設定で `[log] debug = true, dir = "..."` の時のみ有効。
    // 既定は完全無効 (SD カード / tmpfs を保護するため)。返り値の
    // _guard が drop されるとファイルへの flush が走るので main の
    // スコープで保持する。
    let _log_guard = init_tracing(&cfg);

    tracing::info!(
        "starting pst-server: bind={} upstream={}:{}",
        cfg.server.bind,
        cfg.peercast.host,
        cfg.peercast.port
    );

    let bind = cfg.server.bind;
    let state = AppState::new(cfg);
    // 静的フロントの場所: ① CLI 引数 `--web <dir>` ② 環境変数
    // `PST_SERVER_WEB_DIR` ③ exe 隣の `web/` ④ ソースツリーの
    // `crates/pst-server/web/` (開発時)。
    let web_dir = resolve_web_dir(&args);
    let app = pst_server::build_router(state, web_dir);

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

/// `[log] debug = true` && `dir` 指定時のみ tracing を有効化する。
/// 返り値の `WorkerGuard` を main の lifetime 中保持することで、
/// プロセス終了時にバッファをファイルに flush できる。
///
/// SD カード保護の方針 (docs/usage/server.md):
///   * 既定: 一切ログを書かない
///   * 有効: 指定ディレクトリに日次ローテーションで書き込む。
///     ディレクトリは tmpfs / RAM disk を推奨。
fn init_tracing(cfg: &Config) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    if !cfg.log.debug {
        return None;
    }
    let dir = cfg.log.dir.trim();
    if dir.is_empty() {
        return None;
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("log dir create failed ({dir}): {e}");
        return None;
    }
    let appender = tracing_appender::rolling::daily(dir, "pst-server.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("pst_server=debug,axum=info,tower_http=info")
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();
    Some(guard)
}

/// 簡素な `--config <path>` パーサ。MVP では他フラグは持たない。
fn parse_config_arg(args: &[String]) -> Option<PathBuf> {
    parse_flag(args, "--config")
}

fn parse_flag(args: &[String], name: &str) -> Option<PathBuf> {
    let mut it = args.iter();
    let prefix = format!("{name}=");
    while let Some(arg) = it.next() {
        if arg == name {
            return it.next().map(PathBuf::from);
        }
        if let Some(rest) = arg.strip_prefix(&prefix) {
            return Some(PathBuf::from(rest));
        }
    }
    None
}

/// 静的フロントのディレクトリを解決する。順序: CLI 引数 `--web` →
/// 環境変数 `PST_SERVER_WEB_DIR` → exe 隣の `web/` → ソースツリーの
/// `crates/pst-server/web/`。どれも見つからなければ None (暫定 HTML
/// にフォールバック)。
fn resolve_web_dir(args: &[String]) -> Option<PathBuf> {
    if let Some(p) = parse_flag(args, "--web") {
        return Some(p);
    }
    if let Ok(s) = std::env::var("PST_SERVER_WEB_DIR") {
        if !s.is_empty() {
            return Some(PathBuf::from(s));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("web");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    // 開発時 (cargo run): manifest ディレクトリ隣の web/
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

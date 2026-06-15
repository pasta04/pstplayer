//! サーバ (axum) の起動ロジック。
//!
//! 単体の `pst-server` バイナリ (`src/main.rs`) と、デスクトップ版の
//! 単一バイナリ (`pstplayer --server`) の **両方から** 同じ関数を呼べる
//! よう、起動処理を main から分離してここに集約する。これにより
//! 「1 バイナリ 2 モード (アプリ / 常駐サーバ)」を実現する。

use std::path::PathBuf;

use crate::config::{self, Config, ConfigError};
use crate::state::AppState;

/// 設定を読み込み、axum サーバを起動して終了までブロックする。
///
/// 返り値はプロセス終了コード相当 (`0` = 正常終了、`2` = 設定エラー、
/// `3` = bind 失敗、`4` = serve エラー)。`args` は実行ファイル名を除いた
/// 引数列で、`--config <path>` / `--web <dir>` を解釈する。`--server` など
/// の未知フラグは無視するので、呼び出し側はそのまま引数を渡してよい。
pub async fn run(args: Vec<String>) -> u8 {
    let explicit_config = parse_config_arg(&args);

    let (cfg, cfg_path) = match config::load(explicit_config.as_deref()) {
        Ok(pair) => pair,
        Err(ConfigError::Io { path, source }) => {
            // 起動前のクリティカルなエラーは stderr に 1 行だけ出す。
            eprintln!("config read error: {} ({})", source, path.display());
            return 2;
        }
        Err(ConfigError::Parse { path, source }) => {
            eprintln!("config parse error: {} ({})", source, path.display());
            return 2;
        }
        Err(ConfigError::Serialize { path, source }) => {
            eprintln!("config serialise error: {} ({})", source, path.display());
            return 2;
        }
    };

    // ログは設定で `[log] debug = true, dir = "..."` の時のみ有効。
    // 既定は完全無効 (SD カード / tmpfs を保護するため)。返り値の
    // _guard が drop されるとファイルへの flush が走るので run の
    // スコープで保持する。
    let _log_guard = init_tracing(&cfg);

    tracing::info!(
        "starting pst-server: bind={} upstream={}:{} config={}",
        cfg.server.bind,
        cfg.peercast.host,
        cfg.peercast.port,
        cfg_path.display()
    );

    let bind = cfg.server.bind;
    let state = AppState::new(cfg, cfg_path);

    // 自動配信録画タスク。recording.enabled + favorites.rules に
    // auto_record=true のルールがあれば polling して録画開始する。
    // task 内で都度設定を読み直すので、ここで条件分岐はしない。
    crate::auto_record::spawn(state.clone());

    // 静的フロントの場所: ① CLI 引数 `--web <dir>` ② 環境変数
    // `PST_SERVER_WEB_DIR` ③ exe 隣の `web/` ④ ソースツリーの
    // `crates/pst-server/web/` (開発時)。
    let web_dir = resolve_web_dir(&args);
    let app = crate::build_router(state, web_dir);

    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("bind {bind} failed: {e}");
            return 3;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("server error: {e}");
        return 4;
    }
    0
}

/// `[log] debug = true` && `dir` 指定時のみ tracing を有効化する。
/// 返り値の `WorkerGuard` を呼び出し側の lifetime 中保持することで、
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

/// 簡素な `--config <path>` パーサ。
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

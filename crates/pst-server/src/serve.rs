//! サーバ (axum) の起動ロジック。
//!
//! 単体の `pst-server` バイナリ (`src/main.rs`) と、デスクトップ版の
//! 単一バイナリ (`pstplayer --server`) の **両方から** 同じ関数を呼べる
//! よう、起動処理を main から分離してここに集約する。これにより
//! 「1 バイナリ 2 モード (アプリ / 常駐サーバ)」を実現する。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;

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

    // 待受アドレスは config の [server] bind が基本。CLI で上書きできる:
    //   --port <n>        … ポートだけ差し替え (host は config/既定のまま)
    //   --bind <ip:port>  … 全体を差し替え (--port より優先)
    let bind = resolve_bind(&args, cfg.server.bind);

    tracing::info!(
        "starting pst-server: bind={} upstream={}:{} config={}",
        bind,
        cfg.peercast.host,
        cfg.peercast.port,
        cfg_path.display()
    );

    // 静的フロントの場所: ① CLI 引数 `--web <dir>` ② 環境変数
    // `PST_SERVER_WEB_DIR` ③ exe 隣の `web/` ④ ソースツリーの
    // `crates/pst-server/web/` (開発時)。
    let web_dir = resolve_web_dir(&args);

    // AppState + 自動録画タスク + router を共通ロジックで組み立てる
    // (auto_record は task 内で都度設定を読み直すので条件分岐はしない)。
    let (_state, app, _auto) = build(cfg, web_dir, cfg_path);

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

/// AppState + Router を組み立て、自動録画タスクを起動する共通部分。
/// `run()` (CLI ブロッキング) と `spawn_embedded()` (in-process) で共有し、
/// bind/serve/auto_record のロジックを重複させない。返り値の AppState は
/// `cfg` (Arc<RwLock>) のホットスワップに使える。
fn build(
    cfg: Config,
    web_dir: Option<PathBuf>,
    config_path: PathBuf,
) -> (AppState, axum::Router, JoinHandle<()>) {
    let state = AppState::new(cfg, config_path);
    let auto_handle = crate::auto_record::spawn(state.clone());
    let app = crate::build_router(state.clone(), web_dir);
    (state, app, auto_handle)
}

/// in-process 埋め込みサーバのハンドル。デスクトップアプリが録画サーバを
/// 自前プロセス内でバックグラウンド起動するために使う。
pub struct EmbeddedServer {
    pub addr: SocketAddr,
    cfg: Arc<RwLock<Config>>,
    serve_handle: JoinHandle<()>,
    auto_handle: JoinHandle<()>,
}

impl EmbeddedServer {
    /// `http://<addr>/` 形式のベース URL (ハブの pst_server_url に注入する)。
    pub fn base_url(&self) -> String {
        format!("http://{}/", self.addr)
    }

    /// 設定の Arc ハンドルを clone して返す。ホットスワップは
    /// `*handle.write().await = new_cfg` で行う (呼び出し側が std Mutex を
    /// await 跨ぎで保持しないで済むよう、メソッドではなくハンドルを渡す)。
    /// これで稼働中サーバの設定 (お気に入り / YP / 録画先 / peercast) を
    /// 次の polling・録画から反映できる。
    pub fn config_handle(&self) -> Arc<RwLock<Config>> {
        self.cfg.clone()
    }

    /// serve / auto_record タスクを停止する。
    pub fn shutdown(&self) {
        self.serve_handle.abort();
        self.auto_handle.abort();
    }
}

/// axum サーバをバックグラウンド (tokio task) で起動し、ブロックせずに
/// ハンドルを返す。`cfg.server.bind` に bind し、実際の `local_addr()` を
/// `addr` に入れる (port=0 の OS 割当にも対応)。
pub async fn spawn_embedded(
    cfg: Config,
    web_dir: Option<PathBuf>,
    config_path: PathBuf,
) -> std::io::Result<EmbeddedServer> {
    let bind = cfg.server.bind;
    let (state, app, auto_handle) = build(cfg, web_dir, config_path);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let addr = listener.local_addr()?;
    let serve_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(EmbeddedServer {
        addr,
        cfg: state.cfg.clone(),
        serve_handle,
        auto_handle,
    })
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
    parse_value(args, name).map(PathBuf::from)
}

/// `--name value` または `--name=value` の値を取り出す汎用パーサ。
fn parse_value(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    let prefix = format!("{name}=");
    while let Some(arg) = it.next() {
        if arg == name {
            return it.next().cloned();
        }
        if let Some(rest) = arg.strip_prefix(&prefix) {
            return Some(rest.to_string());
        }
    }
    None
}

/// 待受アドレスを CLI 引数で上書きする。`--bind <ip:port>` は全体を、
/// `--port <n>` はポートのみを差し替える (両方あれば `--bind` 優先)。
/// 不正値は警告して `base` (config / 既定) を維持する。
fn resolve_bind(args: &[String], base: SocketAddr) -> SocketAddr {
    let mut bind = base;
    if let Some(p) = parse_value(args, "--port") {
        match p.parse::<u16>() {
            Ok(port) => bind.set_port(port),
            Err(_) => eprintln!("warning: --port の値が不正です: {p} (config/既定値を使用)"),
        }
    }
    if let Some(b) = parse_value(args, "--bind") {
        match b.parse::<SocketAddr>() {
            Ok(addr) => bind = addr,
            Err(_) => eprintln!("warning: --bind の値が不正です: {b} (例: 0.0.0.0:9000)"),
        }
    }
    bind
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

#[cfg(test)]
mod tests {
    use super::resolve_bind;
    use std::net::SocketAddr;

    fn base() -> SocketAddr {
        "0.0.0.0:8080".parse().unwrap()
    }

    #[test]
    fn bind_unchanged_without_flags() {
        assert_eq!(resolve_bind(&[], base()), base());
    }

    #[test]
    fn port_flag_overrides_port_only() {
        let args = vec!["--port".to_string(), "9001".to_string()];
        assert_eq!(resolve_bind(&args, base()).port(), 9001);
        assert_eq!(resolve_bind(&args, base()).ip(), base().ip());
    }

    #[test]
    fn bind_flag_overrides_whole_addr() {
        let args = vec!["--bind".to_string(), "127.0.0.1:9123".to_string()];
        assert_eq!(
            resolve_bind(&args, base()),
            "127.0.0.1:9123".parse().unwrap()
        );
    }

    #[test]
    fn bind_flag_wins_over_port() {
        let args = vec![
            "--port".to_string(),
            "9001".to_string(),
            "--bind".to_string(),
            "127.0.0.1:9123".to_string(),
        ];
        assert_eq!(
            resolve_bind(&args, base()),
            "127.0.0.1:9123".parse().unwrap()
        );
    }

    #[test]
    fn invalid_values_keep_base() {
        assert_eq!(
            resolve_bind(&["--port".into(), "abc".into()], base()),
            base()
        );
        assert_eq!(
            resolve_bind(&["--bind".into(), "nope".into()], base()),
            base()
        );
    }
}

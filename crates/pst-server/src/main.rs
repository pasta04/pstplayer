//! `pst-server` 実行バイナリ。設定を読み、axum サーバを起動する。
//!
//! 使い方:
//!
//! ```bash
//! pst-server                       # デフォルト config パスを使用
//! pst-server --config ./foo.toml   # 明示パス
//! ```
//!
//! 起動ロジックは [`pst_server::serve::run`] に集約してあり、デスクトップ版
//! の単一バイナリ (`pstplayer --server`) からも同じ関数を呼ぶ。

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(pst_server::serve::run(args).await)
}

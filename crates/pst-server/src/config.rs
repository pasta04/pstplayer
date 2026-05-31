//! `pst-server.toml` の設定スキーマと読み込み。
//!
//! デスクトップ版 `PSTPlayer` の config (`pst-core::config`) とは別物。
//! サーバはモバイル端末から複数アクセスされる前提なので、リレー先 PeerCast
//! の指定と bind アドレスだけが必要。

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub peercast: PeerCastUpstream,
    #[serde(default)]
    pub server: ServerBinding,
    #[serde(default)]
    pub log: LogConfig,
}

/// ログ出力の制御。Raspberry Pi 等の SD カード環境を想定し、
/// **既定では一切ログを書き出さない**。`debug = true` かつ `dir`
/// が指定された時のみ、そのディレクトリに日次ローテーションで吐く。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogConfig {
    /// `false` (既定) なら全ログを破棄。`true` で `dir` への書き出し
    /// 有効化。
    #[serde(default)]
    pub debug: bool,
    /// `debug = true` の時の出力先ディレクトリ。空なら `debug = true`
    /// でも no-op (= 出力しない)。tmpfs / RAM disk を推奨。
    #[serde(default)]
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCastUpstream {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub auth_user: Option<String>,
    #[serde(default)]
    pub auth_pass: Option<String>,
}

impl Default for PeerCastUpstream {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 7144,
            auth_user: None,
            auth_pass: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerBinding {
    /// `0.0.0.0:8080` 等。LAN 公開なら 0.0.0.0、ローカルテストは
    /// 127.0.0.1。
    pub bind: SocketAddr,
    /// 逆プロキシ越しの URL。PWA manifest や absolute URL 生成用。
    #[serde(default)]
    pub public_url: String,
}

impl Default for ServerBinding {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".parse().expect("default bind addr"),
            public_url: String::new(),
        }
    }
}

/// `${XDG_CONFIG_HOME:-~/.config}/PSTPlayer/pst-server.toml` 等の標準
/// 場所を返す。デスクトップ版と同じ ProjectDirs を使うので 1 ホスト
/// で両方動かしても干渉しない (ファイル名が違うだけ)。
pub fn default_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("io.github", "pasta04", "PSTPlayer")
        .map(|d| d.config_dir().join("pst-server.toml"))
}

/// 指定パス (なければデフォルト) から TOML を読む。存在しない場合は
/// `Config::default()` を返す。
pub fn load(explicit: Option<&Path>) -> Result<Config, ConfigError> {
    let path = explicit.map(Path::to_path_buf).or_else(default_config_path);
    let Some(path) = path else {
        return Ok(Config::default());
    };
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| ConfigError::Io {
        path: path.clone(),
        source: e,
    })?;
    toml::from_str(&raw).map_err(|e| ConfigError::Parse {
        path,
        source: Box::new(e),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config read failed: {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("config parse failed: {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_toml() {
        let s = r#"
[peercast]
host = "192.0.2.10"
port = 7144

[server]
bind = "127.0.0.1:9000"
public_url = "https://pst.example.lan/"
"#;
        let cfg: Config = toml::from_str(s).unwrap();
        assert_eq!(cfg.peercast.host, "192.0.2.10");
        assert_eq!(cfg.peercast.port, 7144);
        assert_eq!(cfg.server.bind.port(), 9000);
        assert_eq!(cfg.server.public_url, "https://pst.example.lan/");
    }

    #[test]
    fn defaults_are_sane() {
        let cfg = Config::default();
        assert_eq!(cfg.peercast.host, "localhost");
        assert_eq!(cfg.peercast.port, 7144);
        assert_eq!(cfg.server.bind.port(), 8080);
    }
}

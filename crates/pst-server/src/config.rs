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
    #[serde(default)]
    pub recording: RecordingConfig,
    #[serde(default)]
    pub favorites: pst_core::favorites::FavoritesConfig,
}

/// 録画機能の設定。SD カード保護方針に従い既定 OFF。`enabled = true`
/// + `dir` 指定時のみ `/api/record/*` が動く。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// `false` (既定) なら API が `503` を返して録画機能を拒否。
    #[serde(default)]
    pub enabled: bool,
    /// 出力先ディレクトリ (絶対パス推奨)。空なら enabled でも無効化。
    #[serde(default)]
    pub dir: String,
    /// ファイル名拡張子。空なら `flv`。
    #[serde(default)]
    pub ext: String,
    /// 同時に録画できる本数。0 (既定) は 8 として扱う。1 に固定したい
    /// なら 1。Pi 等のリソース制約と SD カード書き込み帯域を踏まえて
    /// 抑えるための上限。
    #[serde(default)]
    pub max_concurrent: u32,
    /// 自動録画タスクの polling 間隔 (秒)。0 (既定) は 60 として扱う。
    /// `favorites.rules` のうち `auto_record = true` のルールにマッチ
    /// する配信が `getChannels` に現れた瞬間、録画を自動で開始する。
    #[serde(default)]
    pub auto_poll_interval_sec: u64,
    /// 自動録画した配信が `getChannels` から消えてから停止するまでの
    /// 猶予 (秒)。0 (既定) は 30 として扱う。短時間の瞬断で録画が
    /// プツプツ切れるのを防ぐためのバッファ。
    #[serde(default)]
    pub auto_stop_grace_sec: u64,
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
/// `Config::default()` を返す。返り値の `PathBuf` は実際に読み書き
/// 対象とするファイルパス (将来 PUT /api/config で同じ場所に書き戻す)。
pub fn load(explicit: Option<&Path>) -> Result<(Config, PathBuf), ConfigError> {
    let path = explicit
        .map(Path::to_path_buf)
        .or_else(default_config_path)
        .unwrap_or_else(|| PathBuf::from("pst-server.toml"));
    if !path.exists() {
        return Ok((Config::default(), path));
    }
    let raw = fs::read_to_string(&path).map_err(|e| ConfigError::Io {
        path: path.clone(),
        source: e,
    })?;
    let cfg = toml::from_str(&raw).map_err(|e| ConfigError::Parse {
        path: path.clone(),
        source: Box::new(e),
    })?;
    Ok((cfg, path))
}

/// `Config` を TOML として `path` に書き出す。親ディレクトリが
/// 無ければ作る。
pub fn save_to(path: &Path, cfg: &Config) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ConfigError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let body = toml::to_string_pretty(cfg).map_err(|e| ConfigError::Serialize {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    fs::write(path, body).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config IO failed: {path}: {source}")]
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
    #[error("config serialise failed: {path}: {source}")]
    Serialize {
        path: PathBuf,
        #[source]
        source: Box<toml::ser::Error>,
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

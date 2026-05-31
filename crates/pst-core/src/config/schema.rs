//! Persisted configuration schema (`config.toml`).
//!
//! See `docs/architecture.md` and `docs/features.md` §2.2 / §5.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub peercast: PeerCastConfig,
    #[serde(default)]
    pub bbs: BbsConfig,
    #[serde(default)]
    pub player: PlayerConfig,
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    /// ユーザーがカスタマイズしたホットキー (action_id → 文字列形式
    /// e.g. `Ctrl+Shift+R`)。未指定の action はフロント側のデフォルト
    /// が使われる。空文字列を入れると「割当無し」として無効化。
    #[serde(default)]
    pub hotkeys: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryConfig {
    /// Recently opened channel URLs (newest first). Capped at MAX_HISTORY.
    #[serde(default)]
    pub recent: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub channel_name: String,
    /// UNIX seconds when last opened.
    pub last_opened_at: u64,
}

pub const MAX_HISTORY: usize = 30;
pub const MAX_RECENT_HOSTS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCastConfig {
    pub host: String,
    pub port: u16,
    pub auth_user: Option<String>,
    pub auth_pass: Option<String>,
    pub timeout_sec: u64,
    #[serde(default)]
    pub recent_hosts: Vec<String>,
    /// YP `index.txt` の URL。空文字列なら YP 機能を無効化。
    /// 例: `http://yp.example.invalid/index.txt`
    #[serde(default)]
    pub yp_url: String,
}

impl Default for PeerCastConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 7144,
            auth_user: None,
            auth_pass: None,
            timeout_sec: 5,
            recent_hosts: Vec::new(),
            yp_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BbsConfig {
    #[serde(default)]
    pub default_name: String,
    #[serde(default)]
    pub default_mail: String,
    #[serde(default = "default_refresh_sec")]
    pub auto_refresh_sec: u32,
    /// "plain" (default) or "html". HTML mode pipes post bodies
    /// through the ammonia whitelist sanitiser.
    #[serde(default = "default_display_mode")]
    pub display_mode: String,
    /// "ctrl_enter" (default; Ctrl/Cmd+Enter で送信) or "shift_enter"
    /// (Shift+Enter で送信). Plain Enter は常に改行。
    #[serde(default = "default_submit_key")]
    pub submit_key: String,
    /// 新着レス到着時に OS 通知を出すか。レス頻度が高いと鬱陶しいので
    /// 既定 false。設定 UI には出さず、TOML 直接編集で有効化する隠し
    /// オプション。
    #[serde(default)]
    pub notify_on_new_post: bool,
    /// 新着レスが追加された時、ユーザーが末尾近くにいたら自動的に
    /// 末尾までスクロールするか。手動スクロール中 (末尾から離れている)
    /// は追従しない。既定 ON。
    #[serde(default = "default_autoscroll")]
    pub autoscroll: bool,
}

fn default_refresh_sec() -> u32 {
    5
}

fn default_display_mode() -> String {
    "plain".to_string()
}

fn default_submit_key() -> String {
    "ctrl_enter".to_string()
}

fn default_autoscroll() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConfig {
    pub volume: u8,
    pub aspect_mode: String,
    /// Empty means "exe directory / snapshot/". See features.md §1.1.
    pub snapshot_dir: String,
    pub snapshot_format: String,
    pub snapshot_jpeg_quality: u8,
    /// 録画の保存先ディレクトリ。空なら exe 直下 `recordings/` を
    /// 使う (snapshot と同じ解決ルール)。
    #[serde(default)]
    pub recording_dir: String,
    /// 録画ファイルの拡張子。`""` (既定) なら libmpv の `stream-record`
    /// が入力フォーマットから推測 (FLV → .flv 等)。明示指定したい時は
    /// "mkv" / "mp4" 等を指定。
    #[serde(default)]
    pub recording_ext: String,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            volume: 80,
            aspect_mode: "auto".into(),
            snapshot_dir: String::new(),
            snapshot_format: "png".into(),
            snapshot_jpeg_quality: 95,
            recording_dir: String::new(),
            recording_ext: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowConfig {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bbs_pane_ratio: Option<f32>,
    pub bbs_pane_position: Option<String>,
    pub always_on_top: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub show_titlebar: bool,
    pub show_frame: bool,
    pub show_statusbar: bool,
    pub show_bbs_pane: bool,
    pub show_bbs_writebox: bool,
    pub titlebar_size_indicator: bool,
    pub titlebar_source_size: bool,
    pub status_fps: bool,
    pub status_size: bool,
    pub status_uptime_dynamic: bool,
    pub status_prefer_play_info: bool,
    pub bbs_refresh_counter: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            show_titlebar: true,
            show_frame: true,
            show_statusbar: true,
            show_bbs_pane: true,
            show_bbs_writebox: true,
            titlebar_size_indicator: true,
            titlebar_source_size: false,
            status_fps: true,
            status_size: false,
            status_uptime_dynamic: true,
            status_prefer_play_info: true,
            bbs_refresh_counter: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_through_toml() {
        let cfg = Config::default();
        let serialised = toml::to_string(&cfg).expect("serialise");
        let parsed: Config = toml::from_str(&serialised).expect("parse");
        assert_eq!(cfg.peercast.host, parsed.peercast.host);
        assert_eq!(cfg.player.volume, parsed.player.volume);
        assert!(parsed.display.show_titlebar);
    }
}

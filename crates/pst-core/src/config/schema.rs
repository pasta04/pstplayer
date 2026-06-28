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
    /// YP / チャンネルリストに対するお気に入り (フィルタ) ルール。
    /// 詳細は [`crate::favorites`]。
    #[serde(default)]
    pub favorites: crate::favorites::FavoritesConfig,
    /// 複数 YP ソース設定。PeerCastStation には「YP X に登録されている
    /// 全チャンネル」を返す API が無いため、各 YP の `index.txt` を直接
    /// HTTP GET する。詳細は [docs/design/pstplayer-hub-settings.md]。
    #[serde(default)]
    pub yp: YpConfig,
    /// ハブ画面の表示設定。
    #[serde(default)]
    pub hub: HubConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    /// YP の自動再 fetch 間隔 (秒)。0 で自動更新無効。既定 60。
    #[serde(default = "default_hub_refresh_sec")]
    pub refresh_sec: u32,
    /// 「視聴中」リストのポーリング間隔 (秒)。0 で無効。既定 5。
    #[serde(default = "default_hub_watching_poll_sec")]
    pub watching_poll_sec: u32,
    /// ダブルクリック時の動作。既定 `Watch` (視聴)。
    #[serde(default)]
    pub double_click: HubClickAction,
    /// ミドルクリック時の動作。既定 `OpenBbs`。
    #[serde(default = "default_middle_click")]
    pub middle_click: HubClickAction,
    /// 「🌐 pst-server」ボタンが開く URL。空なら `http://localhost:8080/`。
    /// 同居運用 (localhost) と別マシン (192.168.x.y:8080) の切替用。
    #[serde(default)]
    pub pst_server_url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubClickAction {
    /// 何もしない (= 行選択のみ)。
    None,
    /// 視聴ウィンドウを別プロセスで開く。
    #[default]
    Watch,
    /// 視聴 + 録画開始。
    WatchAndRecord,
    /// BBS としてコンタクト URL を開く。
    OpenBbs,
    /// コンタクト URL をブラウザで開く。
    OpenContact,
}

fn default_middle_click() -> HubClickAction {
    HubClickAction::OpenBbs
}

fn default_hub_refresh_sec() -> u32 {
    60
}

fn default_hub_watching_poll_sec() -> u32 {
    5
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            refresh_sec: default_hub_refresh_sec(),
            watching_poll_sec: default_hub_watching_poll_sec(),
            double_click: HubClickAction::default(),
            middle_click: default_middle_click(),
            pst_server_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YpConfig {
    /// 登録 YP の一覧。表示順 (タブ並び) はこの順。重複 channel_id は
    /// 上にある YP のものを採用 (= 上ほど優先)。
    #[serde(default)]
    pub sources: Vec<YpSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YpSource {
    /// UI 表示用の短い名前 (例: "SP", "EP")。
    pub name: String,
    /// `index.txt` の URL。
    pub url: String,
    /// 名前空間。PeCaRecorder 相当。空欄可。
    #[serde(default)]
    pub namespace: String,
    /// ハブのタブバーにこの YP のタブを出すか。
    #[serde(default = "default_true")]
    pub show_tab: bool,
    /// 「すべて」タブに含めるか。
    #[serde(default = "default_true")]
    pub show_in_all: bool,
    /// この YP 由来のチャンネル行のデフォルト文字色 (CSS color)。空なら
    /// テーマ既定。お気に入りルールの `text_color` が当たればそちらが優先。
    #[serde(default)]
    pub text_color: String,
    /// 背景色。同上。
    #[serde(default)]
    pub background: String,
}

fn default_true() -> bool {
    true
}

impl Config {
    /// マイグレーション込みで「実際に YP として fetch するソース一覧」を
    /// 返す。`yp.sources` に明示があればそれを使う。空のときは旧
    /// `peercast.yp_url` を 1 件の YpSource に変換 (移行期の互換性)。
    pub fn effective_yp_sources(&self) -> Vec<YpSource> {
        if !self.yp.sources.is_empty() {
            return self.yp.sources.clone();
        }
        let legacy = self.peercast.yp_url.trim();
        if legacy.is_empty() {
            return Vec::new();
        }
        vec![YpSource {
            name: "YP".to_string(),
            url: legacy.to_string(),
            namespace: String::new(),
            show_tab: true,
            show_in_all: true,
            text_color: String::new(),
            background: String::new(),
        }]
    }
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

// フロント (api.ts / 設定 UI / viewer) は bbs 設定を camelCase で読み書き
// するので、ここで rename_all = camelCase に揃える。これが無いと
// display_mode / submit_key / auto_refresh_sec 等が JS 側で undefined になり
// 設定が round-trip しない (実機で判明した潜在バグ)。
//
// Default は derive せず手書きする。`#[serde(default = "...")]` は TOML を
// パースする時 (= キー欠落時) しか効かず、config.toml が無い / [bbs] セクション
// 欠落時に使われる `BbsConfig::default()` には反映されない。derive(Default) だと
// bool=false / String="" / u32=0 になり、autoscroll が既定で OFF・display_mode
// が空…と意図しない既定になってしまう (実機で発覚)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// 新着レスへ自動スクロールする際の速度 (px/秒)。スムーズに流して
    /// 読めるようにするための値。大きいほど速い。既定 600。
    #[serde(default = "default_autoscroll_speed")]
    pub autoscroll_speed: u32,
}

fn default_autoscroll_speed() -> u32 {
    600
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

impl Default for BbsConfig {
    fn default() -> Self {
        Self {
            default_name: String::new(),
            default_mail: String::new(),
            auto_refresh_sec: default_refresh_sec(),
            display_mode: default_display_mode(),
            submit_key: default_submit_key(),
            notify_on_new_post: false,
            autoscroll: default_autoscroll(),
            autoscroll_speed: default_autoscroll_speed(),
        }
    }
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
    /// 配信が予期せず切断された時に自動再接続を試みるかどうか。
    /// 既定 false (= 観察モード: end_file の reason をステータス帯と
    /// stderr に出すだけで実際の再接続はしない)。実機で reason 値の
    /// 挙動を確認してから true に切り替える運用想定。
    ///
    /// 有効時の挙動 (詳細は src-tauri/src/player/engine.rs):
    /// - EOF / ERROR / REDIRECT で再接続を試みる (STOP / QUIT は無視)
    /// - 指数バックオフ: 1, 2, 4, 8, 16, 30, 30, 30, 30, 30 秒
    /// - 総タイムアウト 5 分 / 最大 10 試行
    /// - 再接続後 3 秒以内に切れる「即切断」が 3 回連続したら配信終了と
    ///   推定して打ち切り (リレー網が完全に死亡 = origin が止まったケース)
    /// - ユーザ手動 stop / 別チャンネル load でリセット
    #[serde(default)]
    pub auto_reconnect: bool,
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
            auto_reconnect: false,
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
    /// true なら最小化時にウィンドウをタスクトレイへ格納する (タスクバーからも
    /// 消す)。既定 false = 通常のタスクバー最小化。トレイアイコンから復帰する。
    /// 変更は次回起動から反映 (トレイ生成が起動時のため)。
    #[serde(default)]
    pub minimize_to_tray: bool,
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

    #[test]
    fn legacy_yp_url_migrates_to_effective_sources() {
        // 旧 config: peercast.yp_url のみ指定。yp.sources は空。
        let toml_str = r#"
            [peercast]
            host = "localhost"
            port = 7144
            timeout_sec = 5
            yp_url = "http://yp.example/index.txt"
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        let sources = cfg.effective_yp_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].url, "http://yp.example/index.txt");
        assert_eq!(sources[0].name, "YP");
        assert!(sources[0].show_tab);
        assert!(sources[0].show_in_all);
    }

    #[test]
    fn explicit_yp_sources_wins_over_legacy_yp_url() {
        let toml_str = r#"
            [peercast]
            host = "localhost"
            port = 7144
            timeout_sec = 5
            yp_url = "http://legacy.example/index.txt"

            [[yp.sources]]
            name = "SP"
            url = "http://sp.example/index.txt"
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        let sources = cfg.effective_yp_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "SP");
        assert_eq!(sources[0].url, "http://sp.example/index.txt");
    }

    #[test]
    fn no_yp_returns_empty() {
        let cfg = Config::default();
        let sources = cfg.effective_yp_sources();
        assert!(sources.is_empty());
    }
}

//! デスクトップアプリ内蔵の録画サーバ (pst-server を in-process 起動)。
//!
//! 「アプリをダウンロード → ダブルクリック → YP 一覧 → 右クリック / フィルタ
//! 自動でバックグラウンド録画」を、別途サーバを起動せずゼロ設定で成立させる。
//! 録画は HTTP で配信を受信しファイル保存するだけ (ウィンドウ無し・再生無し)。
//!
//! ハブ起動 (URL 引数なし) のときだけ起動し、ユーザーが外部 pst-server を
//! 設定している場合 (`hub.pst_server_url` 非空) はそちらを尊重して起動しない。
//! bind は常に loopback、ポートは OS 任せ (0) で衝突を避け、実ポートは
//! `local_addr()` から取得してハブの `pst_server_url` に注入する
//! (`commands::config::get_config`)。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pst_server::serve::EmbeddedServer;
use tauri::{AppHandle, Manager, Runtime};

/// 内蔵録画サーバは常に loopback へ bind する (他デバイスからは見えない。
/// LAN / モバイル視聴はスタンドアロン pst-server の役目)。
const EMBEDDED_HOST: [u8; 4] = [127, 0, 0, 1];

/// 内蔵録画サーバのハンドル (起動成功時 Some)。Tauri State として manage する。
/// viewer プロセスや bind 失敗時は None のまま。
#[derive(Default)]
pub struct EmbeddedServerState(pub Mutex<Option<EmbeddedServer>>);

/// 実行ファイルの隣ディレクトリ (録画先の既定解決に使う)。
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
}

/// デスクトップ config (`pst_core`) を内蔵録画サーバ用の
/// `pst_server::config::Config` に写像する。録画を有効化し、録画先は
/// `player.recording_dir` が空でも `<exe>/recordings` に解決する (既存の
/// libmpv 録画と同じ `resolve_record_dir` を再利用)。`port` は bind 用で、
/// 起動後 (ホットスワップ時) は使われない。
pub fn map_config(
    desktop: &pst_core::config::Config,
    exe_dir: Option<&Path>,
    port: u16,
) -> pst_server::config::Config {
    let resolved = pst_core::snapshot::resolve_record_dir(&desktop.player, exe_dir);
    pst_server::config::Config {
        peercast: pst_server::config::PeerCastUpstream {
            host: desktop.peercast.host.clone(),
            port: desktop.peercast.port,
            auth_user: desktop.peercast.auth_user.clone(),
            auth_pass: desktop.peercast.auth_pass.clone(),
        },
        server: pst_server::config::ServerBinding {
            bind: std::net::SocketAddr::from((EMBEDDED_HOST, port)),
            public_url: String::new(),
        },
        log: pst_server::config::LogConfig::default(),
        bbs: pst_server::config::BbsDefaults {
            default_name: desktop.bbs.default_name.clone(),
            default_mail: desktop.bbs.default_mail.clone(),
        },
        recording: pst_server::config::RecordingConfig {
            enabled: true,
            dir: resolved.dir.to_string_lossy().into_owned(),
            ext: desktop.player.recording_ext.clone(),
            max_concurrent: 0,         // 0 = 既定 (8)
            auto_poll_interval_sec: 0, // 0 = 既定 (60)
            auto_stop_grace_sec: 0,    // 0 = 既定 (30)
        },
        favorites: desktop.favorites.clone(),
        yp: desktop.yp.clone(),
    }
}

/// ハブ起動時に内蔵録画サーバを立ち上げる。外部 URL 設定時は何もしない。
/// bind 失敗は致命的でなく、録画導線は「pst-server が必要」エラーへフォール
/// バックする (= 従来 UX)。
pub fn start_if_enabled<R: Runtime>(app: AppHandle<R>) {
    let cfg = match pst_core::config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("embedded server: config load failed ({e}); not starting");
            return;
        }
    };
    if !cfg.hub.pst_server_url.trim().is_empty() {
        // 外部 pst-server を明示設定済み → 内蔵は起動しない。
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mapped = map_config(&cfg, exe_dir().as_deref(), 0);
        let config_path = pst_server::config::default_config_path()
            .unwrap_or_else(|| PathBuf::from("pst-server.toml"));
        match pst_server::serve::spawn_embedded(mapped, None, config_path).await {
            Ok(server) => {
                let url = server.base_url();
                if let Some(state) = app.try_state::<EmbeddedServerState>() {
                    if let Ok(mut guard) = state.0.lock() {
                        *guard = Some(server);
                    }
                }
                eprintln!("embedded recording server started: {url}");
            }
            Err(e) => {
                eprintln!(
                    "embedded recording server: bind failed ({e}); recording needs an external pst-server"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::map_config;
    use std::path::Path;

    #[test]
    fn enables_recording_and_resolves_dir() {
        let mut desktop = pst_core::config::Config::default();
        desktop.peercast.host = "192.0.2.5".into();
        desktop.peercast.port = 7144;
        // recording_dir が空でも dir は既定 (<exe>/recordings) に解決される。
        desktop.player.recording_dir = String::new();
        let mapped = map_config(&desktop, Some(Path::new("/tmp/app")), 0);
        assert!(mapped.recording.enabled);
        assert!(
            !mapped.recording.dir.is_empty(),
            "recording dir should resolve to a non-empty default"
        );
        assert_eq!(mapped.peercast.host, "192.0.2.5");
        assert_eq!(mapped.peercast.port, 7144);
        assert!(mapped.server.bind.ip().is_loopback());
    }

    #[test]
    fn carries_favorites_and_yp_and_port() {
        let mut desktop = pst_core::config::Config::default();
        desktop.yp.sources.push(pst_core::config::schema::YpSource {
            name: "SP".into(),
            url: "http://example/index.txt".into(),
            namespace: String::new(),
            show_tab: true,
            show_in_all: true,
            text_color: String::new(),
            background: String::new(),
        });
        let mapped = map_config(&desktop, None, 8080);
        assert_eq!(mapped.yp.sources.len(), 1);
        assert_eq!(mapped.yp.sources[0].name, "SP");
        assert_eq!(mapped.server.bind.port(), 8080);
    }
}

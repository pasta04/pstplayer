//! `spawn_embedded` の起動テスト。デスクトップアプリが in-process で録画
//! サーバを立ち上げる経路を検証する。127.0.0.1:0 (OS 割当ポート) に bind し、
//! `base_url()` 経由で `/api/record/list` が応答する (= 録画 API が稼働) ことを
//! 確認する。

use pst_server::config::Config;

#[tokio::test]
async fn embedded_server_boots_and_serves_record_api() {
    let mut cfg = Config::default();
    cfg.server.bind = "127.0.0.1:0".parse().unwrap();
    cfg.recording.enabled = true;

    let server =
        pst_server::serve::spawn_embedded(cfg, None, std::path::PathBuf::from("unused.toml"))
            .await
            .expect("spawn_embedded should bind on 127.0.0.1:0");

    // base_url は実 local_addr 由来 (loopback + OS 割当ポート)。
    assert!(server.addr.ip().is_loopback());
    assert_ne!(server.addr.port(), 0, "OS should assign a real port");

    let url = format!("{}api/record/list", server.base_url());
    let resp = reqwest::get(&url).await.expect("GET /api/record/list");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.expect("json body");
    let recordings = body
        .get("recordings")
        .and_then(|r| r.as_array())
        .expect("recordings array");
    assert!(recordings.is_empty(), "no recordings expected at boot");

    server.shutdown();
}

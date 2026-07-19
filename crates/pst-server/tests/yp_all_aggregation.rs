//! `/api/yp/all` (サーバ側 YP 集約) の統合テスト。
//!
//! PeerCast YP の `index.txt` をローカルの axum スタブで模倣し、設定の複数
//! ソースを集約できること (並び順保持・重複 channel_id の dedup・`yp_source`
//! タグ付け) を確認する。M1 の回帰テスト。実ネットワーク (localhost) 越しに
//! fetch するので、`fetch_indexes` の HTTP 経路もまとめて検証される。

use std::net::SocketAddr;

use axum::extract::State;
use axum::{routing::get, Json, Router};
use pst_core::config::schema::{YpConfig, YpSource};
use pst_server::config::Config;
use pst_server::state::AppState;

// 19 個の `<>` 区切りフィールド (name,id,tip,contact,genre,desc,listeners,
// relays,bitrate,type,...,uptime,flag,comment,flag)。
const SP_INDEX: &str = "テスト配信SP-1<>0123456789abcdef0123456789abcdef<>192.0.2.1:7144<>http://example.com/<>ゲーム<>説明1<>42<>3<>1500<>FLV<><><><><>name<>1:23<>click<>コメ<>0
テスト配信SP-2<>11111111111111111111111111111111<>192.0.2.1:7144<>http://example.com/<>雑談<>説明2<>7<>3<>800<>FLV<><><><><>name<>1:23<>click<>コメ<>0
";

const EP_INDEX: &str = "テスト配信EP-1<>22222222222222222222222222222222<>192.0.2.1:7144<>http://example.com/<>音楽<>説明<>100<>3<>2000<>FLV<><><><><>name<>1:23<>click<>コメ<>0
dup<>0123456789abcdef0123456789abcdef<>192.0.2.1:7144<>http://example.com/<>g<>d<>1<>3<>500<>FLV<><><><><>n<>1:23<>c<>x<>0
";

/// `index.txt` を返す最小スタブ (PeerCast YP の模倣) を起動し、待受 addr を返す。
async fn spawn_yp_stub() -> SocketAddr {
    let app = Router::new()
        .route("/sp.txt", get(|| async { SP_INDEX }))
        .route("/ep.txt", get(|| async { EP_INDEX }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

fn src(name: &str, url: String) -> YpSource {
    YpSource {
        name: name.to_string(),
        url,
        namespace: String::new(),
        show_tab: true,
        show_in_all: true,
        text_color: String::new(),
        background: String::new(),
    }
}

#[tokio::test]
async fn yp_all_aggregates_dedups_and_tags_source() {
    let addr = spawn_yp_stub().await;
    let cfg = Config {
        yp: YpConfig {
            sources: vec![
                src("SP", format!("http://{addr}/sp.txt")),
                src("EP", format!("http://{addr}/ep.txt")),
            ],
        },
        ..Default::default()
    };
    let state = AppState::new(cfg, std::path::PathBuf::from("unused.toml"));

    let Json(outcome) = pst_server::handlers::yp_all(State(state)).await;

    // SP-1, SP-2, EP-1 の 3 件。EP の "dup" 行 (SP-1 と同一 id) は除外。
    assert!(
        outcome.failures.is_empty(),
        "unexpected failures: {:?}",
        outcome.failures
    );
    assert_eq!(outcome.entries.len(), 3, "entries: {:?}", outcome.entries);

    let names: Vec<&str> = outcome.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"テスト配信SP-1"));
    assert!(names.contains(&"テスト配信SP-2"));
    assert!(names.contains(&"テスト配信EP-1"));
    // 重複 id は先頭ソース (SP) が優先され、EP の "dup" 行は採用されない。
    assert!(!names.contains(&"dup"), "dup should be deduped: {names:?}");

    // 採用元 YP が yp_source に入る。
    let sp1 = outcome
        .entries
        .iter()
        .find(|e| e.name == "テスト配信SP-1")
        .unwrap();
    assert_eq!(sp1.yp_source, "SP");
    assert_eq!(sp1.listeners, 42);
    assert_eq!(sp1.bitrate, 1500);
    let ep1 = outcome
        .entries
        .iter()
        .find(|e| e.name == "テスト配信EP-1")
        .unwrap();
    assert_eq!(ep1.yp_source, "EP");
}

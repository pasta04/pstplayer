//! Live write-test against the two threads specified by the user
//! (`docs`/PR comment) to verify the BBS post pipeline end-to-end.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p pst-core --example write_test
//! ```
//!
//! Behaviour:
//! - Posts a single explicitly-marked "test post" to each target thread.
//! - Never retries on its own (rejection / regulation is reported only).
//! - Sleeps 5 seconds between the two posts to avoid back-to-back rate
//!   limiting.
//!
//! The body identifies the post as a non-conversational connectivity probe
//! so the thread owners can recognise it and (if desired) request deletion.

use std::time::Duration;

use pst_core::bbs::{ch2::Ch2Client, shitaraba::ShitarabaClient, types::PostRequest};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let body = concat!(
        "PSTPlayer (Tauri + Rust 製クロスプラットフォーム PeerCast 視聴ソフトの開発中) ",
        "の BBS クライアント疎通確認の自動テスト投稿です。\n",
        "実装の動作確認のみで会話を意図したものではありません。お騒がせします。\n",
        "リポジトリ: https://github.com/pasta04/pstplayer (PR #1)",
    )
    .to_string();

    let req = PostRequest {
        name: "PSTPlayer test".into(),
        mail: "sage".into(),
        body,
    };

    println!("── shitaraba 投稿テスト ─────────────────────────────");
    let shi_url = "https://jbbs.shitaraba.net/bbs/read.cgi/game/51638/1587841373/";
    println!("URL : {shi_url}");
    match ShitarabaClient::new().post(shi_url, &req).await {
        Ok(()) => println!("結果: ✅ success"),
        Err(e) => println!("結果: ❌ {e:?}"),
    }

    println!();
    println!("(rate limit 回避のため 5 秒待機)");
    tokio::time::sleep(Duration::from_secs(5)).await;

    println!();
    println!("── 2ch 互換 (jpnkn) 投稿テスト ──────────────────────");
    let ch2_url = "https://bbs.jpnkn.com/test/read.cgi/carbonara/1558097910/";
    println!("URL : {ch2_url}");
    match Ch2Client::new().post(ch2_url, &req).await {
        Ok(()) => println!("結果: ✅ success"),
        Err(e) => println!("結果: ❌ {e:?}"),
    }
}

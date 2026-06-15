//! pst-server (ハブ常駐サーバ) の録画 API への薄いプロキシ。
//!
//! Desktop の webview は CSP (`connect-src 'self'`) で任意ホストへ fetch
//! できないため、録画の開始 / 停止 / 一覧は Rust 側 (reqwest) から pst-server
//! を叩く。録画は pst-server が「再生せず HTTP ストリームを直接ファイルに
//! 保存」する方式なので、視聴ウィンドウを開かず (= 音を出さず) に録画できる。
//! これが「YP/ハブ側で再生せず録画」の実体 (実機 QA フィードバック対応)。

use pst_core::util::errors::{AppError, IpcError};
use serde_json::{json, Value};

fn endpoint(server_url: &str, path: &str) -> String {
    format!("{}/{}", server_url.trim_end_matches('/'), path.trim_start_matches('/'))
}

/// 指定チャンネルを pst-server で録画開始する (視聴ウィンドウ無し・再生無し)。
/// 戻り値は pst-server の `RecordingEntry` JSON。
#[tauri::command]
pub async fn server_record_start(
    server_url: String,
    id: String,
    name: String,
) -> Result<Value, IpcError> {
    let url = endpoint(&server_url, "api/record/start");
    let v = reqwest::Client::new()
        .post(&url)
        .json(&json!({ "id": id, "name": name }))
        .send()
        .await
        .map_err(AppError::from)?
        .error_for_status()
        .map_err(AppError::from)?
        .json::<Value>()
        .await
        .map_err(AppError::from)?;
    Ok(v)
}

/// pst-server の録画を停止する (`id` 省略時は全停止)。
#[tauri::command]
pub async fn server_record_stop(server_url: String, id: Option<String>) -> Result<Value, IpcError> {
    let url = endpoint(&server_url, "api/record/stop");
    let v = reqwest::Client::new()
        .post(&url)
        .json(&json!({ "id": id }))
        .send()
        .await
        .map_err(AppError::from)?
        .error_for_status()
        .map_err(AppError::from)?
        .json::<Value>()
        .await
        .map_err(AppError::from)?;
    Ok(v)
}

/// pst-server で録画中のチャンネル一覧 (`{ recordings: [{channel_id, ...}] }`)。
#[tauri::command]
pub async fn server_record_list(server_url: String) -> Result<Value, IpcError> {
    let url = endpoint(&server_url, "api/record/list");
    let v = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(AppError::from)?
        .error_for_status()
        .map_err(AppError::from)?
        .json::<Value>()
        .await
        .map_err(AppError::from)?;
    Ok(v)
}

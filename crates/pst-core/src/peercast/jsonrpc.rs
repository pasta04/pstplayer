//! Minimal JSON-RPC 2.0 client for PeerCastStation `/api/1`.
//!
//! See `docs/protocols/peercast.md` §3.1 for the request format.

use crate::util::{
    errors::{AppError, AppResult},
    http::CLIENT,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use super::types::{ChannelInfo, ChannelRecord, ChannelStatus, PeerCastEndpoint, Track, VersionInfo};

/// `getChannelInfo` のレスポンス形状。PeerCastStation は
/// `{ info: {...}, track: {...}, yellowPages: [...] }` のネスト構造を
/// 返すため、フラットな [`ChannelInfo`] に直接デシリアライズすると
/// 全フィールドが空になる (= name が "(unnamed)"、url 空でスレッド非表示)。
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ChannelInfoResult {
    info: ChannelInfo,
    track: Track,
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u32,
    method: &'a str,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<Value>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Build a request and invoke it against the given endpoint.
/// Returns the raw JSON `result` field.
pub async fn call(endpoint: &PeerCastEndpoint, method: &str, params: Value) -> AppResult<Value> {
    let url = format!("{}/api/1", endpoint.base_url());
    let req = JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method,
        params,
    };

    let mut builder = CLIENT
        .post(&url)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Content-Type", "application/json");

    if let Some(auth) = &endpoint.auth {
        builder = builder.basic_auth(&auth.user, Some(&auth.pass));
    }

    let resp = builder.json(&req).send().await?;
    let status = resp.status();
    let body: JsonRpcResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Decode(format!("non-JSON response (status {status}): {e}")))?;

    if let Some(err) = body.error {
        return Err(AppError::Network(format!(
            "JSON-RPC error {}: {}",
            err.code, err.message
        )));
    }
    body.result
        .ok_or_else(|| AppError::Decode("JSON-RPC response had neither result nor error".into()))
}

/// Deserialise the JSON result into a concrete type.
async fn call_typed<T: DeserializeOwned>(
    endpoint: &PeerCastEndpoint,
    method: &str,
    params: Value,
) -> AppResult<T> {
    let raw = call(endpoint, method, params).await?;
    serde_json::from_value(raw).map_err(|e| AppError::Decode(e.to_string()))
}

// ── Convenience wrappers ────────────────────────────────────────────

pub async fn get_version_info(endpoint: &PeerCastEndpoint) -> AppResult<VersionInfo> {
    call_typed(endpoint, "getVersionInfo", json!([])).await
}

pub async fn get_channels(endpoint: &PeerCastEndpoint) -> AppResult<Vec<ChannelRecord>> {
    call_typed(endpoint, "getChannels", json!([])).await
}

pub async fn get_channel_info(
    endpoint: &PeerCastEndpoint,
    channel_id: &str,
) -> AppResult<ChannelInfo> {
    let result: ChannelInfoResult =
        call_typed(endpoint, "getChannelInfo", json!([channel_id])).await?;
    let mut info = result.info;
    // PeerCast の慣習では配信のコンタクト URL は info.url に入るが、
    // 実装によっては track.url 側にしか入らないことがあるので、info.url
    // が空なら track.url で補完する (BBS スレッド解決に使う)。
    if info.url.trim().is_empty() && !result.track.url.trim().is_empty() {
        info.url = result.track.url;
    }
    Ok(info)
}

pub async fn get_channel_status(
    endpoint: &PeerCastEndpoint,
    channel_id: &str,
) -> AppResult<ChannelStatus> {
    call_typed(endpoint, "getChannelStatus", json!([channel_id])).await
}

pub async fn bump_channel(endpoint: &PeerCastEndpoint, channel_id: &str) -> AppResult<()> {
    call(endpoint, "bumpChannel", json!([channel_id])).await?;
    Ok(())
}

pub async fn stop_channel(endpoint: &PeerCastEndpoint, channel_id: &str) -> AppResult<()> {
    call(endpoint, "stopChannel", json!([channel_id])).await?;
    Ok(())
}

/// Probe whether the endpoint speaks JSON-RPC `/api/1`.
pub async fn supports_jsonrpc(endpoint: &PeerCastEndpoint) -> bool {
    get_version_info(endpoint).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_info_deserialises_camel_case() {
        let json = serde_json::json!({
            "name": "Test",
            "genre": "Music",
            "bitrate": 320,
            "contentType": "FLV"
        });
        let info: ChannelInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.name, "Test");
        assert_eq!(info.content_type, "FLV");
        assert_eq!(info.bitrate, 320);
    }

    #[test]
    fn channel_status_defaults_for_missing_fields() {
        let json = serde_json::json!({ "status": "Receiving" });
        let st: ChannelStatus = serde_json::from_value(json).unwrap();
        assert_eq!(st.status, "Receiving");
        assert_eq!(st.uptime, 0);
        assert!(!st.is_broadcasting);
    }

    #[test]
    fn channel_info_result_extracts_nested_info() {
        // PeerCastStation の getChannelInfo が返す実際のネスト形状。
        let json = serde_json::json!({
            "info": {
                "name": "テスト配信",
                "url": "http://jbbs.example/bbs/read.cgi/game/12345/",
                "genre": "Game",
                "bitrate": 2000,
                "contentType": "FLV"
            },
            "track": { "name": "song", "url": "http://track.example/" },
            "yellowPages": []
        });
        let result: ChannelInfoResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.info.name, "テスト配信");
        assert_eq!(result.info.url, "http://jbbs.example/bbs/read.cgi/game/12345/");
        assert_eq!(result.info.bitrate, 2000);
    }

    #[test]
    fn channel_info_result_falls_back_to_track_url() {
        // info.url が空で track.url のみ埋まっているケースの補完ロジックは
        // get_channel_info 側にあるが、構造体としては両方読めることを確認。
        let json = serde_json::json!({
            "info": { "name": "X", "url": "" },
            "track": { "url": "http://contact.example/thread/" }
        });
        let result: ChannelInfoResult = serde_json::from_value(json).unwrap();
        assert!(result.info.url.is_empty());
        assert_eq!(result.track.url, "http://contact.example/thread/");
    }

    #[test]
    fn channel_record_round_trip() {
        let json = serde_json::json!({
            "channelId": "ABC",
            "info": { "name": "X" },
            "track": { "name": "Y" },
            "status": { "status": "Idle", "uptime": 12 }
        });
        let rec: ChannelRecord = serde_json::from_value(json).unwrap();
        assert_eq!(rec.channel_id, "ABC");
        assert_eq!(rec.info.name, "X");
        assert_eq!(rec.track.name, "Y");
        assert_eq!(rec.status.uptime, 12);
    }
}

//! Minimal JSON-RPC 2.0 client for PeerCastStation `/api/1`.
//!
//! See `docs/protocols/peercast.md` §3.1 for the request format.

use crate::util::{
    errors::{AppError, AppResult},
    http::CLIENT,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::PeerCastEndpoint;

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
pub async fn call(endpoint: &PeerCastEndpoint, method: &str, params: Value) -> AppResult<Value> {
    let url = format!("{}/api/1", endpoint.base_url());
    let req = JsonRpcRequest { jsonrpc: "2.0", id: 1, method, params };

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
        return Err(AppError::Network(format!("JSON-RPC error {}: {}", err.code, err.message)));
    }
    body.result
        .ok_or_else(|| AppError::Decode("JSON-RPC response had neither result nor error".into()))
}

/// Probe whether the endpoint speaks JSON-RPC `/api/1`.
pub async fn supports_jsonrpc(endpoint: &PeerCastEndpoint) -> bool {
    call(endpoint, "getVersionInfo", Value::Array(vec![])).await.is_ok()
}

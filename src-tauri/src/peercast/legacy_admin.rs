//! Fallback client for the old HTML/XML admin API
//! (`http://host:port/admin?cmd=...`).
//!
//! See `docs/protocols/peercast.md` §3.2.
//! Implementation intentionally stubbed for now — JSON-RPC is preferred
//! and the strategy in §3.3 only reaches this code path when `/api/1`
//! does not respond.

use crate::util::errors::{AppError, AppResult};

use super::types::PeerCastEndpoint;

pub async fn stop(_endpoint: &PeerCastEndpoint, _channel_id: &str) -> AppResult<()> {
    Err(AppError::NotImplemented("legacy_admin::stop"))
}

pub async fn bump(_endpoint: &PeerCastEndpoint, _channel_id: &str) -> AppResult<()> {
    Err(AppError::NotImplemented("legacy_admin::bump"))
}

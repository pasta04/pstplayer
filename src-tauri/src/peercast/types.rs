use serde::{Deserialize, Serialize};

/// Channel identifier (32-char lowercase hex).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(String);

impl ChannelId {
    /// Construct a normalized (lowercase) channel id, validating the hex length.
    pub fn parse(s: &str) -> Result<Self, &'static str> {
        if s.len() != 32 {
            return Err("channel id must be 32 hex chars");
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("channel id must contain only hex digits");
        }
        Ok(ChannelId(s.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub contact: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub name: String,
    pub id: String,
    pub bitrate: u32,
    pub content_type: String,
    pub genre: String,
    pub desc: String,
    pub url: String,
    pub uptime: u64,
    pub comment: String,
    pub track: Track,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub status: String,
    pub uptime: u64,
    pub local_relays: u32,
    pub local_directs: u32,
    pub total_relays: u32,
    pub total_directs: u32,
    pub is_broadcasting: bool,
    pub is_firewalled: Option<bool>,
}

/// Endpoint of a PeerCast host we connect to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerCastEndpoint {
    pub host: String,
    pub port: u16,
    pub auth: Option<BasicAuth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicAuth {
    pub user: String,
    pub pass: String,
}

impl Default for PeerCastEndpoint {
    fn default() -> Self {
        Self { host: "localhost".to_string(), port: 7144, auth: None }
    }
}

impl PeerCastEndpoint {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

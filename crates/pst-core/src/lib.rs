//! PSTPlayer core: UI-agnostic PeerCast / BBS / config building blocks.
//!
//! Shared between the Tauri desktop app (`pstplayer`) and the future
//! HTTP relay server. See `docs/decisions/0005-workspace-and-server.md`.

pub mod bbs;
pub mod cli;
pub mod config;
pub mod peercast;
pub mod single_instance;
pub mod util;

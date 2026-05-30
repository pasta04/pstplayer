//! Multiple-launch policy.
//!
//! See `docs/features.md` §2.1. Stub for now — wiring to Tauri's
//! `single_instance` plugin happens during 1.1.x.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchPolicy {
    #[default]
    NewWindow,
    Replace,
    Single,
}

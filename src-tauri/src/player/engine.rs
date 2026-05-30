//! libmpv FFI wrapper.
//!
//! Stub: real binding selection (libmpv2 vs. raw) is part of
//! roadmap §1.3. For now we expose the API surface so the IPC layer
//! can compile.

use crate::util::errors::{AppError, AppResult};

pub struct PlayerEngine;

impl PlayerEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn load(&self, _url: &str) -> AppResult<()> {
        Err(AppError::NotImplemented("player::load"))
    }

    pub fn stop(&self) -> AppResult<()> {
        Err(AppError::NotImplemented("player::stop"))
    }

    pub fn set_volume(&self, _percent: u8) -> AppResult<()> {
        Err(AppError::NotImplemented("player::set_volume"))
    }
}

impl Default for PlayerEngine {
    fn default() -> Self {
        Self::new()
    }
}

//! libmpv FFI wrapper.
//!
//! Thin shell around `libmpv2::Mpv`. Window embedding (passing the
//! Tauri window's native handle into mpv via the `wid` property) is
//! implemented separately in `window.rs` per OS.

use std::sync::Arc;

use libmpv2::{GetData, Mpv};
use pst_core::util::errors::{AppError, AppResult};

#[derive(Clone)]
pub struct PlayerEngine {
    mpv: Arc<Mpv>,
}

impl PlayerEngine {
    /// Initialise libmpv with sensible defaults for an embedded player.
    pub fn new() -> AppResult<Self> {
        let mpv = Mpv::with_initializer(|init| {
            // We will attach mpv to our Tauri window later; until then
            // run "off-screen" rather than spawn a separate window.
            //
            // `force-window=no` keeps mpv from opening its own window
            // when we have not yet handed it a `wid`. The window code
            // in `window.rs` flips this to `yes` once it has the
            // native handle.
            init.set_option("force-window", "no").ok();
            init.set_option("idle", "yes").ok();
            // Keep the aspect ratio while filling the player area
            // (matches docs/ui-design.md §配信画面の動画スケーリング).
            init.set_option("keepaspect", "yes").ok();
            init.set_option("video-unscaled", "no").ok();
            // Be quiet during development.
            init.set_option("terminal", "no").ok();
            init.set_option("msg-level", "all=warn").ok();
            // Live-friendly defaults.
            init.set_option("cache", "yes").ok();
            init.set_option("demuxer-max-bytes", "10MiB").ok();
            init.set_option("demuxer-max-back-bytes", "5MiB").ok();
            Ok(())
        })
        .map_err(map_err)?;
        Ok(Self { mpv: Arc::new(mpv) })
    }

    /// Load `url` and start playback (`replace` the current entry).
    pub fn load(&self, url: &str) -> AppResult<()> {
        self.mpv.command("loadfile", &[url, "replace"]).map_err(map_err)
    }

    /// Stop playback and unload the current file.
    pub fn stop(&self) -> AppResult<()> {
        self.mpv.command("stop", &[]).map_err(map_err)
    }

    /// Toggle pause state.
    pub fn set_pause(&self, pause: bool) -> AppResult<()> {
        self.mpv.set_property("pause", pause).map_err(map_err)
    }

    /// Set volume in percent (0–100). libmpv treats 100 as "no change".
    pub fn set_volume(&self, percent: u8) -> AppResult<()> {
        let v = percent.min(200) as i64;
        self.mpv.set_property("volume", v).map_err(map_err)
    }

    /// Set the mute state.
    pub fn set_mute(&self, mute: bool) -> AppResult<()> {
        self.mpv.set_property("mute", mute).map_err(map_err)
    }

    /// Generic property accessor for status-bar consumers
    /// (e.g. `estimated-vf-fps`, `video-params/w`, `video-params/h`).
    pub fn get_property<T: GetData>(&self, name: &str) -> Option<T> {
        self.mpv.get_property::<T>(name).ok()
    }

    /// Borrow the underlying handle for OS-specific window attachment
    /// (see `window.rs`). The returned Arc keeps the same instance.
    pub fn handle(&self) -> Arc<Mpv> {
        Arc::clone(&self.mpv)
    }
}

fn map_err(e: libmpv2::Error) -> AppError {
    AppError::Network(format!("mpv: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Initialising libmpv requires libmpv.so to be present at link
    /// time and to load at run time. We treat init failures as a
    /// soft skip so that environments without libmpv installed (some
    /// minimal CI containers) don't break the rest of the suite.
    #[test]
    fn engine_initialises() {
        match PlayerEngine::new() {
            Ok(engine) => {
                // Setting volume on a fresh engine without a loaded
                // file is allowed and just stores the value.
                engine.set_volume(80).expect("set_volume");
                engine.set_mute(false).expect("set_mute");
            }
            Err(e) => {
                eprintln!("libmpv not available, skipping: {e}");
            }
        }
    }
}

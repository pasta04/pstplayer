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

    /// Capture the current video frame to `path`. The file extension
    /// determines the format (libmpv supports png/jpg/jpeg/webp).
    /// The `flag` argument is the libmpv "include" flag —
    /// "subtitles" (default), "video" (no overlay), or "window".
    pub fn screenshot_to_file(&self, path: &str, flag: &str) -> AppResult<()> {
        self.mpv.command("screenshot-to-file", &[path, flag]).map_err(map_err)
    }

    /// 録画を開始する。`path` には拡張子付きの保存先絶対パスを渡す。
    /// libmpv は現在のストリームを **再エンコードせずそのまま** 書き
    /// 出す。空文字列を渡すと録画停止と同じ意味になる。
    pub fn start_record(&self, path: &str) -> AppResult<()> {
        self.mpv.set_property("stream-record", path).map_err(map_err)
    }

    /// 録画を停止する。
    pub fn stop_record(&self) -> AppResult<()> {
        self.mpv.set_property("stream-record", "").map_err(map_err)
    }

    /// 録画中なら現在の保存先パスを返す。
    pub fn record_path(&self) -> Option<String> {
        self.mpv.get_property::<String>("stream-record").ok().filter(|s| !s.is_empty())
    }

    /// Override the displayed aspect ratio.
    ///   0.0 → "no override" (use the video's own DAR)
    ///  -1.0 → stretch to fill the window
    /// positive → use that ratio (e.g. 16.0/9.0)
    pub fn set_aspect(&self, ratio: f64) -> AppResult<()> {
        // Use a string here because libmpv2 doesn't expose a Double
        // setter for free-form floats and "0" / "-1" are accepted.
        let v = format!("{ratio}");
        self.mpv.set_property("video-aspect-override", v.as_str()).map_err(map_err)
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

    /// Initialising libmpv requires libmpv to be present at link
    /// time and to load at run time. We treat init failures as a
    /// soft skip so that environments without libmpv installed (some
    /// minimal CI containers) don't break the rest of the suite.
    ///
    /// Historical note: Windows CI used to abort with
    /// `STATUS_DLL_NOT_FOUND` because the GitHub runner couldn't
    /// resolve libmpv-2.dll's delay-loaded API set imports. That was
    /// worked around by copying the DLL into System32 in the workflow
    /// and switching from `cargo build --release` to
    /// `tauri build --no-bundle`; full `cargo test --workspace` now
    /// runs on Windows too.
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

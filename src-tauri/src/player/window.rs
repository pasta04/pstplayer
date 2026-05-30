//! Native-window embedding for libmpv.
//!
//! libmpv supports two strategies:
//!
//! 1. **`wid` property** — hand mpv a native window handle
//!    (`HWND` on Windows, `NSView*` on macOS, `Window` (XID) on
//!    Linux/X11). mpv draws directly into that handle.
//!
//! 2. **Render API** — use `mpv_render_context_create` to receive
//!    decoded frames into our own OpenGL / D3D11 / Metal context.
//!    More flexible (we can composite under web UI), but needs much
//!    more glue per OS.
//!
//! For MVP we go with strategy 1. The Tauri webview is positioned
//! *above* the mpv-drawn area via CSS `pointer-events` and
//! background transparency, so click handling on the surrounding UI
//! still goes to the webview.
//!
//! Implementation per OS lives in the modules below. They are stubs
//! for now; each will be filled in during the libmpv embedding PoC
//! (roadmap §1.3).

use libmpv2::Mpv;
use pst_core::util::errors::{AppError, AppResult};

/// Attach the player to the given Tauri window. Sets the `wid`
/// property on libmpv to the OS-native handle and flips
/// `force-window` to `yes` so mpv starts rendering immediately.
pub fn attach<R: tauri::Runtime>(_mpv: &Mpv, _window: &tauri::Window<R>) -> AppResult<()> {
    // TODO(roadmap §1.3): per-OS wid lookup using
    //   window.raw_window_handle() and mpv.set_property("wid", ...)
    Err(AppError::NotImplemented("player::window::attach"))
}

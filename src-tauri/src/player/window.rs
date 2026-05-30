//! Per-OS window embedding for libmpv (`wid` property strategy).
//!
//! See `docs/protocols/peercast.md` & ADR-0001 for the choice of
//! strategy. We hand mpv a native window handle and let it draw into
//! that surface; the Tauri WebView sits on top and provides the BBS /
//! control UI overlay.

use libmpv2::Mpv;
use pst_core::util::errors::{AppError, AppResult};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Attach the player to the given Tauri WebView window.
///
/// Sets the `wid` property on libmpv to the OS-native handle:
///
/// | OS      | Handle source       | Type passed to mpv |
/// | ------- | ------------------- | ------------------ |
/// | Windows | `HWND`              | `isize` (pointer)  |
/// | macOS   | `NSView*`           | `isize` (pointer)  |
/// | Linux   | X11 `Window` (XID)  | `i64`              |
///
/// Wayland is not supported through `wid` — mpv would need its own
/// Wayland surface, which Tauri does not expose. On Wayland systems
/// the user should run their session under XWayland, or we'd have to
/// switch to the render API (out of scope for the MVP).
pub fn attach<R: tauri::Runtime>(mpv: &Mpv, window: &tauri::WebviewWindow<R>) -> AppResult<()> {
    let handle =
        window.window_handle().map_err(|e| AppError::Network(format!("window_handle: {e}")))?;
    let raw = handle.as_raw();
    let wid = wid_from(&raw)?;

    mpv.set_property("wid", wid)
        .map_err(|e| AppError::Network(format!("mpv set wid={wid}: {e}")))?;
    // Once a window handle is attached, allow mpv to render even when
    // no file is loaded yet.
    mpv.set_property("force-window", "yes")
        .map_err(|e| AppError::Network(format!("mpv force-window: {e}")))?;
    Ok(())
}

fn wid_from(raw: &RawWindowHandle) -> AppResult<i64> {
    match raw {
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(h) => Ok(h.hwnd.get() as i64),

        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(h) => Ok(h.ns_view.as_ptr() as i64),

        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        RawWindowHandle::Xlib(h) => Ok(h.window as i64),

        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        RawWindowHandle::Xcb(h) => Ok(h.window.get() as i64),

        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ))]
        RawWindowHandle::Wayland(_) => Err(AppError::Network(
            "Wayland surfaces are not supported via libmpv `wid` — \
             run the app under XWayland or switch to the render API"
                .into(),
        )),

        other => Err(AppError::Network(format!("unsupported window handle variant: {other:?}"))),
    }
}

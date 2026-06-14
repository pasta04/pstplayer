//! libmpv 描画用の子ウィンドウ管理 (Windows 専用)。
//!
//! `wid` にメインウィンドウの HWND を直接渡すと、libmpv は WebView2 と
//! 同じ面に全域描画してしまい、BBS ペインや各バーまで覆ってしまう
//! (実機 QA で確認)。そこで「プレイヤー領域だけを占める子ウィンドウ」を
//! 1 枚作り、その HWND を `wid` に渡す。フロントが `.player-canvas` の
//! 物理ピクセル矩形を測って [`set_rect`] を呼ぶことでリサイズ追従する。
//!
//! 子ウィンドウは Win32 定義済みクラス `"STATIC"` を使う (独自 WNDPROC を
//! 登録しなくて済む)。非 Windows ではこのモジュールは空 (各 OS は従来
//! 通りメインウィンドウへ直接 attach する)。

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::Mutex;

    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, SetWindowPos, HMENU, HWND_TOP, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        WINDOW_EX_STYLE, WS_CHILD, WS_VISIBLE,
    };

    /// 作成済みの子ウィンドウ HWND を保持する。Tauri の managed state として
    /// 1 インスタンスだけ持つ。
    #[derive(Default)]
    pub struct VideoEmbed {
        child: Mutex<Option<isize>>,
    }

    impl VideoEmbed {
        pub fn new() -> Self {
            Self::default()
        }

        /// 既に子ウィンドウを作っていればその HWND を、無ければ
        /// `parent_hwnd` の子として新規作成して返す (= mpv に渡す wid)。
        /// `parent_hwnd` はメインウィンドウの HWND (isize 表現)。
        pub fn ensure_child(&self, parent_hwnd: isize) -> Result<isize, String> {
            let mut guard = self.child.lock().expect("VideoEmbed lock");
            if let Some(h) = *guard {
                return Ok(h);
            }
            // SAFETY: parent_hwnd はメインウィンドウから取得した有効な HWND。
            // STATIC クラスは常に登録済み。失敗時は HWND(0) が返るので検査する。
            let child = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!(""),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    0,
                    16,
                    16,
                    Some(HWND(parent_hwnd as *mut _)),
                    Some(HMENU(std::ptr::null_mut())),
                    None,
                    None,
                )
            }
            .map_err(|e| format!("CreateWindowExW failed: {e}"))?;
            if child.0.is_null() {
                return Err("CreateWindowExW returned null HWND".into());
            }
            let h = child.0 as isize;
            *guard = Some(h);
            Ok(h)
        }

        /// 子ウィンドウをプレイヤー領域の矩形 (親クライアント座標・物理px)
        /// に移動 / リサイズする。未作成なら何もしない。
        pub fn set_rect(&self, x: i32, y: i32, w: i32, h: i32) -> Result<(), String> {
            let guard = self.child.lock().expect("VideoEmbed lock");
            let Some(handle) = *guard else {
                return Ok(());
            };
            let w = w.max(1);
            let h = h.max(1);
            // SAFETY: handle は ensure_child で作った有効な子 HWND。
            unsafe {
                SetWindowPos(
                    HWND(handle as *mut _),
                    Some(HWND_TOP),
                    x,
                    y,
                    w,
                    h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            }
            .map_err(|e| format!("SetWindowPos failed: {e}"))
        }

        /// 現在の子ウィンドウのクライアント矩形 (デバッグ用)。
        #[allow(dead_code)]
        pub fn child_rect(&self) -> Option<(i32, i32, i32, i32)> {
            let guard = self.child.lock().expect("VideoEmbed lock");
            let handle = (*guard)?;
            let mut r = RECT::default();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetClientRect(
                    HWND(handle as *mut _),
                    &mut r,
                )
                .ok()?;
            }
            Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
        }
    }

    // 未使用 import を黙らせる (WPARAM/LPARAM は将来の WNDPROC 拡張用に予約)。
    #[allow(unused_imports)]
    use {LPARAM as _Lp, WPARAM as _Wp};
}

#[cfg(target_os = "windows")]
pub use imp::VideoEmbed;

#[cfg(not(target_os = "windows"))]
mod imp {
    /// 非 Windows ではメインウィンドウへ直接 attach するので子ウィンドウは
    /// 不要。API 互換のためのダミー。
    #[derive(Default)]
    pub struct VideoEmbed;

    impl VideoEmbed {
        pub fn new() -> Self {
            Self
        }
        pub fn ensure_child(&self, parent_hwnd: isize) -> Result<isize, String> {
            // 子ウィンドウを作らず、親 (= メインウィンドウ) をそのまま wid に使う。
            Ok(parent_hwnd)
        }
        pub fn set_rect(&self, _x: i32, _y: i32, _w: i32, _h: i32) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub use imp::VideoEmbed;

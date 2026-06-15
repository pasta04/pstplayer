//! libmpv 描画用の子ウィンドウ管理 (Windows 専用)。
//!
//! `wid` にメインウィンドウの HWND を直接渡すと、libmpv は WebView2 と
//! 同じ面に全域描画してしまい、BBS ペインや各バーまで覆ってしまう
//! (実機 QA で確認)。そこで「プレイヤー領域だけを占める子ウィンドウ」を
//! 1 枚作り、その HWND を `wid` に渡す。フロントが `.player-canvas` の
//! 物理ピクセル矩形を測って [`set_rect`] を呼ぶことでリサイズ追従する。
//!
//! ## マウス入力
//!
//! libmpv は `wid` の内側にさらに自前の描画ウィンドウを作るため、動画領域
//! 上のマウス操作 (右クリック / ホイール / クリック / ダブルクリック) は
//! その mpv 内側ウィンドウに吸われ、WebView 側のハンドラに届かない
//! (実機 QA: 右クリック・ホイール・背面時クリックでの前面化が全て不発)。
//! そこで wid ウィンドウを独自クラス + WNDPROC にし、さらに mpv が作る
//! 内側ウィンドウも `SetWindowSubclass` で横取りして、マウス操作を Tauri
//! イベント (`player:wheel` / `player:dblclick` / `player:contextmenu` /
//! `player:click`) に変換しフロントへ転送する。

/// 入力イベントをフロント (Tauri) へ転送するコールバック。
/// `(event, x, y, delta)` — event は "wheel"/"dblclick"/"contextmenu"/"click"。
/// x/y は wid ウィンドウのクライアント座標 (contextmenu 用)、delta はホイール量。
pub type InputEmitter = Box<dyn Fn(&str, i32, i32, i32) + Send + Sync>;

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::{Mutex, OnceLock};

    use windows::core::w;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, TRUE, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, EnumChildWindows, RegisterClassExW, SetWindowPos,
        CS_DBLCLKS, HMENU, HWND_TOP, SWP_NOACTIVATE, SWP_SHOWWINDOW, WINDOW_EX_STYLE,
        WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONUP, WNDCLASSEXW, WS_CHILD,
        WS_VISIBLE,
    };

    static EMIT: OnceLock<super::InputEmitter> = OnceLock::new();

    /// フロントへ入力イベントを転送するコールバックを登録する (起動時に 1 回)。
    pub fn set_input_emitter(f: super::InputEmitter) {
        let _ = EMIT.set(f);
    }

    fn emit_input(event: &str, x: i32, y: i32, delta: i32) {
        if let Some(f) = EMIT.get() {
            f(event, x, y, delta);
        }
    }

    /// マウスメッセージを判定して該当すれば Tauri イベントを emit する。
    /// 処理した場合 true (= 既定処理を抑止)。それ以外は false。
    fn dispatch_mouse(msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
        match msg {
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                emit_input("wheel", 0, 0, delta);
                true
            }
            WM_LBUTTONDBLCLK => {
                emit_input("dblclick", 0, 0, 0);
                true
            }
            WM_RBUTTONUP => {
                let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                emit_input("contextmenu", x, y, 0);
                true
            }
            WM_LBUTTONUP => {
                emit_input("click", 0, 0, 0);
                true
            }
            _ => false,
        }
    }

    /// wid ウィンドウ (独自クラス) の WNDPROC。
    unsafe extern "system" fn video_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if dispatch_mouse(msg, wparam, lparam) {
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    /// mpv が内側に作る描画ウィンドウ用のサブクラスプロシージャ。
    unsafe extern "system" fn child_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if dispatch_mouse(msg, wparam, lparam) {
            return LRESULT(0);
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    /// `EnumChildWindows` のコールバック。見つけた子ウィンドウを 1 つずつ
    /// サブクラス化する (mpv の描画ウィンドウを横取りするため)。
    unsafe extern "system" fn enum_subclass_children(child: HWND, _lp: LPARAM) -> BOOL {
        // 同じ id で 2 回呼んでも SetWindowSubclass は冪等 (既存を置換)。
        let _ = SetWindowSubclass(child, Some(child_subclass_proc), SUBCLASS_ID, 0);
        TRUE
    }

    const SUBCLASS_ID: usize = 0x70_73_74_70; // 'pstp'

    /// 独自ウィンドウクラスを 1 回だけ登録し、クラス名を返す。
    fn ensure_class() {
        static REGISTERED: OnceLock<()> = OnceLock::new();
        REGISTERED.get_or_init(|| unsafe {
            let hinst = GetModuleHandleW(None).map(Into::into).unwrap_or_default();
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                // CS_DBLCLKS: ダブルクリック (WM_LBUTTONDBLCLK) を受け取る。
                style: CS_DBLCLKS,
                lpfnWndProc: Some(video_wndproc),
                hInstance: hinst,
                lpszClassName: w!("PstplayerVideoSurface"),
                ..Default::default()
            };
            RegisterClassExW(&wc);
        });
    }

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
        ///
        /// WS_EX_TRANSPARENT は付けない。透過させても mpv 内側ウィンドウが
        /// イベントを食うため意味が無く、逆に自前 WNDPROC で拾えなくなる。
        pub fn ensure_child(&self, parent_hwnd: isize) -> Result<isize, String> {
            let mut guard = self.child.lock().expect("VideoEmbed lock");
            if let Some(h) = *guard {
                return Ok(h);
            }
            ensure_class();
            // SAFETY: parent_hwnd はメインウィンドウから取得した有効な HWND。
            // 独自クラスは ensure_class で登録済み。失敗時は null が返るので検査。
            let child = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("PstplayerVideoSurface"),
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
        ///
        /// このタイミングで mpv が内側に作った描画ウィンドウを毎回
        /// `EnumChildWindows` で拾ってサブクラス化する (冪等)。mpv は
        /// loadfile 時に描画ウィンドウを作るので、初期化直後だと間に合わ
        /// ないことがあるが、リサイズ追従で繰り返し呼ばれるため最終的に
        /// 必ず横取りできる。
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
                .map_err(|e| format!("SetWindowPos failed: {e}"))?;
                // mpv の内側ウィンドウを横取り (best-effort)。
                let _ = EnumChildWindows(
                    Some(HWND(handle as *mut _)),
                    Some(enum_subclass_children),
                    LPARAM(0),
                );
            }
            Ok(())
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
}

#[cfg(target_os = "windows")]
pub use imp::{set_input_emitter, VideoEmbed};

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

    /// 非 Windows では入力転送は不要 (mpv が直接メインウィンドウに描画)。
    pub fn set_input_emitter(_f: super::InputEmitter) {}
}

#[cfg(not(target_os = "windows"))]
pub use imp::{set_input_emitter, VideoEmbed};

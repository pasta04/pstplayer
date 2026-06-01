//! `channel_id` 単位の単一プロセス占有 (single instance) ロック。
//!
//! Desktop マルチビュー (ADR-0006 Step 4) の中核。同じ
//! `channel_id` の視聴ウィンドウを **同時に 2 つ立ち上げない** ための
//! 仕組み。
//!
//! # 設計
//!
//! 1. 取得側 (`acquire`) は OS の一時ディレクトリに `pstplayer-locks/`
//!    を掘り、`ch-{channel_id}.lock` を作る。ロックファイルには TOML で
//!    自分の PID と `127.0.0.1:NNNN` の IPC アドレスを書く
//! 2. 取得側は同時に `127.0.0.1:0` で `TcpListener` を bind する。
//!    フォーカス要求を受け取るためのソケット
//! 3. 後続プロセスは `acquire` 時にロックファイルを発見すると、書かれ
//!    ている `ipc_addr` に `ping\n` を送る。`pong\n` が返れば本物 →
//!    `Conflict` を返す。返ってこなければ stale → 削除して取り直し
//! 4. 後続側は `request_focus(info.ipc_addr)` で `focus\n` を送る。
//!    取得側のリスナースレッドが受け取り、Tauri ウィンドウを前面化する
//! 5. ハブ側は `request_close(info.ipc_addr)` で `close\n` を送って
//!    視聴ウィンドウを終了させられる (一括クローズ機能)。サーバ側は
//!    `serve` の `on_close` コールバックで `app.exit(0)` を呼ぶ
//!
//! # なぜ PID チェックでなくポート ping か
//!
//! クロスプラットフォームに PID 生存確認するには `sysinfo` や OS 別の
//! syscall が要り依存が増える。代わりに「自分が listen している
//! ランダムポートに ping したら自分が pong を返す」ことで「本物の
//! pstplayer がそのポートで生きている」ことを直接確認できる。別の
//! プロセスが偶然そのポートを掴んでいた場合でも、ping に対する
//! プロトコル違反な応答になるため誤判定しない。
//!
//! # 競合状態
//!
//! 2 プロセスが同時に同じ `channel_id` の `acquire` を呼ぶレースは
//! `OpenOptions::create_new(true)` で原子的に弾く。片方は IO エラー
//! (AlreadyExists) になり、もう片方が勝つ。

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const PING: &[u8] = b"ping\n";
const PONG: &[u8] = b"pong\n";
const FOCUS: &[u8] = b"focus\n";
const CLOSE: &[u8] = b"close\n";
const STATE: &[u8] = b"state\n";
const START_RECORD: &[u8] = b"startrec\n";
const STOP_RECORD: &[u8] = b"stoprec\n";
const OK: &[u8] = b"ok\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub pid: u32,
    pub ipc_addr: SocketAddr,
    pub channel_id: String,
}

pub enum AcquireResult {
    /// 自分がロックを取得した。
    Owned(LockHandle),
    /// 既に他プロセスが持っていて生存も確認できた。
    Conflict(LockInfo),
}

/// ロック保持中の RAII ハンドル。drop 時にロックファイルを削除する。
/// `take_listener` でリスナーを 1 回だけ取り出せる。
pub struct LockHandle {
    path: PathBuf,
    pub channel_id: String,
    listener: Option<TcpListener>,
}

impl LockHandle {
    pub fn take_listener(&mut self) -> Option<TcpListener> {
        self.listener.take()
    }
}

impl Drop for LockHandle {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SingleInstanceError {
    #[error("lock dir setup failed: {0}")]
    LockDir(#[source] std::io::Error),
    #[error("listener bind failed: {0}")]
    Bind(#[source] std::io::Error),
    #[error("write lock file failed: {0}")]
    Write(#[source] std::io::Error),
    #[error("serialise lock info failed: {0}")]
    Serialise(#[source] toml::ser::Error),
}

pub fn lock_dir() -> PathBuf {
    std::env::temp_dir().join("pstplayer-locks")
}

fn lock_path(channel_id: &str) -> PathBuf {
    // ASCII 英数のみに正規化 (path traversal / 制御文字対策)。空文字に
    // なった場合は別個のサニタイズ済みフォルダ名に逃がして、複数の不正
    // channel_id が同じ `ch-.lock` を奪い合うのを防ぐ。
    let safe: String = channel_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let filename = if safe.is_empty() {
        "ch-invalid.lock".to_string()
    } else {
        format!("ch-{safe}.lock")
    };
    lock_dir().join(filename)
}

/// 指定 `channel_id` のロックを取りにいく。
///
/// 既に他プロセスが生きていれば `Conflict`、そうでなければ
/// 新規にロックファイルを作成し `Owned(LockHandle)` を返す。
pub fn acquire(channel_id: &str) -> Result<AcquireResult, SingleInstanceError> {
    let dir = lock_dir();
    fs::create_dir_all(&dir).map_err(SingleInstanceError::LockDir)?;
    let path = lock_path(channel_id);

    // 既存ロックファイルの生死を確認。
    if let Ok(s) = fs::read_to_string(&path) {
        if let Ok(info) = toml::from_str::<LockInfo>(&s) {
            if probe_alive(info.ipc_addr) {
                return Ok(AcquireResult::Conflict(info));
            }
        }
        // stale → 削除して取り直し
        let _ = fs::remove_file(&path);
    }

    // ロックファイルを atomic に作る (CREATE_NEW で同名作成競合を弾く)。
    let listener = TcpListener::bind("127.0.0.1:0").map_err(SingleInstanceError::Bind)?;
    let addr = listener.local_addr().map_err(SingleInstanceError::Bind)?;
    let info = LockInfo {
        pid: std::process::id(),
        ipc_addr: addr,
        channel_id: channel_id.to_string(),
    };
    let body = toml::to_string(&info).map_err(SingleInstanceError::Serialise)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(SingleInstanceError::Write)?;
    file.write_all(body.as_bytes())
        .map_err(SingleInstanceError::Write)?;
    drop(file);

    Ok(AcquireResult::Owned(LockHandle {
        path,
        channel_id: channel_id.to_string(),
        listener: Some(listener),
    }))
}

/// 既存ロックが書かれているがプロセスが生きているか確認する。
fn probe_alive(addr: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    if stream.write_all(PING).is_err() {
        return false;
    }
    // PONG = b"pong\n" (5 バイト)。たまたまポートを掴んでいる別プロセスが
    // 任意のバイト列を返してきても誤判定しないよう、厳密に一致を要求する。
    let mut buf = [0u8; PONG.len()];
    match stream.read(&mut buf) {
        Ok(n) => n == PONG.len() && buf == *PONG,
        Err(_) => false,
    }
}

/// 既存プロセスにフォーカス要求を送る。OK 応答 (`ok\n`) を確認するまで
/// 待ち、応答が無い / 違う場合は Err を返す (= 呼び出し側が死亡判定可)。
pub fn request_focus(addr: SocketAddr) -> std::io::Result<()> {
    send_op(addr, FOCUS)
}

/// 既存プロセスにクローズ要求を送る。受け取った viewer 側は `serve`
/// の `on_close` コールバックでウィンドウを閉じる (= プロセス終了)。
/// OK 応答を確認 (close 後にプロセスが死ぬので、OK だけ受け取れれば成功)。
pub fn request_close(addr: SocketAddr) -> std::io::Result<()> {
    send_op(addr, CLOSE)
}

/// 既存プロセスに状態問い合わせを送り、録画中なら true を返す。
/// 応答が `1\n` なら true、それ以外は false。
pub fn query_recording(addr: SocketAddr) -> std::io::Result<bool> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(STATE)?;
    let mut buf = [0u8; 8];
    let n = stream.read(&mut buf).unwrap_or(0);
    Ok(n >= 1 && buf[0] == b'1')
}

/// 既存プロセスに録画停止要求を送る。viewer 側で stream-record を空に
/// 設定する。録画していない時は no-op。OK 応答 (`ok\n`) を待つ。
pub fn request_stop_recording(addr: SocketAddr) -> std::io::Result<()> {
    send_op(addr, STOP_RECORD)
}

/// 既存プロセスに録画開始要求を送る。viewer 側で stream-record を
/// `<recording_dir>/<timestamp>_<channel_name>.<ext>` に設定する。
/// 既に録画中の時は no-op (重複起動は viewer 側で防ぐ)。OK 応答を待つ。
pub fn request_start_recording(addr: SocketAddr) -> std::io::Result<()> {
    send_op(addr, START_RECORD)
}

/// 共通の「opcode を送って OK 応答を待つ」ヘルパ。応答が無い / `ok\n`
/// 以外なら Err を返す。これにより呼び出し側 (`spawn_viewer` の
/// request_focus 等) は IPC が本当に成立したかを判定できる。
fn send_op(addr: SocketAddr, op: &[u8]) -> std::io::Result<()> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(op)?;
    let mut buf = [0u8; 8];
    let n = stream.read(&mut buf)?;
    if n >= OK.len() && buf.starts_with(OK) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "viewer did not return ok",
        ))
    }
}

/// 既存ロックファイルを読む (なければ None)。スポーン側で「既に視聴
/// ウィンドウがあるか?」をチェックするための read-only 操作。
pub fn read_existing(channel_id: &str) -> Option<LockInfo> {
    let path = lock_path(channel_id);
    let s = fs::read_to_string(&path).ok()?;
    let info: LockInfo = toml::from_str(&s).ok()?;
    if probe_alive(info.ipc_addr) {
        Some(info)
    } else {
        // 死んでた → スポーン側で stale lock を消しておく (取り直しの
        // ためだが、acquire 側でも結局消すので best-effort)。
        let _ = fs::remove_file(&path);
        None
    }
}

/// 現在生きている (probe で応答する) ロック全部の一覧を返す。死んでる
/// stale ロックは best-effort で削除する。ハブ画面の「視聴中」タブ表示用。
pub fn list_active() -> Vec<LockInfo> {
    let dir = lock_dir();
    let Ok(read) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("lock") {
            continue;
        }
        let Ok(s) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(info) = toml::from_str::<LockInfo>(&s) else {
            continue;
        };
        if probe_alive(info.ipc_addr) {
            out.push(info);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
    out
}

/// `acquire` で受け取った `TcpListener` をブロッキングループで処理する。
///
/// - `on_focus`: `focus\n` 受信時に呼ぶ (ウィンドウを前面化する想定)
/// - `on_close`: `close\n` 受信時に呼ぶ (プロセス終了する想定)
/// - `is_recording`: `state\n` 受信時に呼ばれ、現在の録画状態を返す。
///   true なら `1\n`、false なら `0\n` を応答する
///
/// すべてのコールバックは UI スレッド外で呼ばれるので、内部で
/// `app.run_on_main_thread` 等を使うこと。
pub fn serve<F, C, S, Rs, Rt>(
    listener: TcpListener,
    on_focus: F,
    on_close: C,
    is_recording: S,
    on_start_record: Rs,
    on_stop_record: Rt,
) where
    F: Fn() + Send + 'static,
    C: Fn() + Send + 'static,
    S: Fn() -> bool + Send + Sync + 'static,
    Rs: Fn() + Send + 'static,
    Rt: Fn() + Send + 'static,
{
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
        let mut buf = [0u8; 16];
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            continue;
        }
        let req = &buf[..n];
        if req.starts_with(PING) {
            let _ = stream.write_all(PONG);
        } else if req.starts_with(FOCUS) {
            let _ = stream.write_all(OK);
            on_focus();
        } else if req.starts_with(CLOSE) {
            let _ = stream.write_all(OK);
            on_close();
        } else if req.starts_with(STATE) {
            let body: &[u8] = if is_recording() { b"1\n" } else { b"0\n" };
            let _ = stream.write_all(body);
        } else if req.starts_with(START_RECORD) {
            let _ = stream.write_all(OK);
            on_start_record();
        } else if req.starts_with(STOP_RECORD) {
            let _ = stream.write_all(OK);
            on_stop_record();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    // 並列テストで衝突しないようテスト用に suffix を生成する。
    static SUFFIX: AtomicU32 = AtomicU32::new(0);
    fn unique_id(prefix: &str) -> String {
        let n = SUFFIX.fetch_add(1, Ordering::SeqCst);
        format!("{prefix}{n:08x}deadbeefdeadbeefdeadbeefdeadbeef")
    }

    #[test]
    fn owned_then_conflict() {
        let ch = unique_id("a");
        let owned = match acquire(&ch).unwrap() {
            AcquireResult::Owned(h) => h,
            AcquireResult::Conflict(_) => panic!("expected owned"),
        };
        // serve loop を別スレッドで起こす
        let mut h = owned;
        let listener = h.take_listener().unwrap();
        let focused = Arc::new(AtomicU32::new(0));
        let focused_c = focused.clone();
        thread::spawn(move || {
            serve(
                listener,
                move || {
                    focused_c.fetch_add(1, Ordering::SeqCst);
                },
                || {},
                || false,
                || {},
                || {},
            );
        });
        // 同じ channel_id で 2 度目 acquire → Conflict
        match acquire(&ch).unwrap() {
            AcquireResult::Conflict(info) => {
                assert_eq!(info.channel_id, ch);
                // focus 要求を送る → serve が拾う
                request_focus(info.ipc_addr).unwrap();
                thread::sleep(Duration::from_millis(100));
                assert_eq!(focused.load(Ordering::SeqCst), 1);
            }
            AcquireResult::Owned(_) => panic!("expected conflict"),
        }
        // h を drop すると lock ファイルが消える
        drop(h);
        // 再 acquire できる (stale でなく、消えてるので Owned)
        match acquire(&ch).unwrap() {
            AcquireResult::Owned(_) => {}
            AcquireResult::Conflict(_) => panic!("expected owned after drop"),
        }
    }

    #[test]
    fn stale_lock_is_replaced() {
        let ch = unique_id("b");
        // 偽の lock を直接書く (ipc_addr は誰も listen していない)。
        let path = lock_path(&ch);
        fs::create_dir_all(lock_dir()).unwrap();
        let fake = LockInfo {
            pid: 999_999,
            ipc_addr: "127.0.0.1:1".parse().unwrap(),
            channel_id: ch.clone(),
        };
        fs::write(&path, toml::to_string(&fake).unwrap()).unwrap();
        // acquire → stale 判定 → Owned
        match acquire(&ch).unwrap() {
            AcquireResult::Owned(_) => {}
            AcquireResult::Conflict(_) => panic!("expected owned for stale"),
        }
    }

    #[test]
    fn read_existing_returns_none_for_stale() {
        let ch = unique_id("c");
        let path = lock_path(&ch);
        fs::create_dir_all(lock_dir()).unwrap();
        let fake = LockInfo {
            pid: 0,
            ipc_addr: "127.0.0.1:1".parse().unwrap(),
            channel_id: ch.clone(),
        };
        fs::write(&path, toml::to_string(&fake).unwrap()).unwrap();
        let res = read_existing(&ch);
        assert!(res.is_none(), "stale lock should not register as existing");
    }

    #[test]
    fn request_close_triggers_on_close_callback() {
        let ch = unique_id("g");
        let owned = match acquire(&ch).unwrap() {
            AcquireResult::Owned(h) => h,
            AcquireResult::Conflict(_) => panic!(),
        };
        let mut h = owned;
        let listener = h.take_listener().unwrap();
        let closed = Arc::new(AtomicU32::new(0));
        let closed_c = closed.clone();
        thread::spawn(move || {
            serve(
                listener,
                || {},
                move || {
                    closed_c.fetch_add(1, Ordering::SeqCst);
                },
                || false,
                || {},
                || {},
            );
        });
        // 別「プロセス」から close 要求 (= read_existing で addr 取得)
        let info = read_existing(&ch).expect("alive");
        request_close(info.ipc_addr).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(closed.load(Ordering::SeqCst), 1);
        drop(h);
    }

    #[test]
    fn list_active_finds_live_owners_only() {
        let ch_alive = unique_id("e");
        let owned = match acquire(&ch_alive).unwrap() {
            AcquireResult::Owned(h) => h,
            AcquireResult::Conflict(_) => panic!(),
        };
        let mut h = owned;
        let listener = h.take_listener().unwrap();
        thread::spawn(move || serve(listener, || {}, || {}, || false, || {}, || {}));

        // 偽の死んだロックも 1 つ書く
        let ch_dead = unique_id("f");
        let dead_path = lock_path(&ch_dead);
        fs::create_dir_all(lock_dir()).unwrap();
        let fake = LockInfo {
            pid: 999_999,
            ipc_addr: "127.0.0.1:1".parse().unwrap(),
            channel_id: ch_dead.clone(),
        };
        fs::write(&dead_path, toml::to_string(&fake).unwrap()).unwrap();

        let active = list_active();
        let ids: Vec<&str> = active.iter().map(|i| i.channel_id.as_str()).collect();
        assert!(
            ids.contains(&ch_alive.as_str()),
            "alive lock should appear in list_active"
        );
        assert!(
            !ids.contains(&ch_dead.as_str()),
            "dead lock should NOT appear in list_active"
        );
        // 死んでたロックは掃除されている
        assert!(!dead_path.exists(), "stale lock should be removed");
        drop(h);
    }

    #[test]
    fn query_recording_returns_true_or_false() {
        // 録画 ON 状態のロック
        let ch_rec = unique_id("h");
        let owned_rec = match acquire(&ch_rec).unwrap() {
            AcquireResult::Owned(h) => h,
            _ => panic!(),
        };
        let mut h_rec = owned_rec;
        let listener_rec = h_rec.take_listener().unwrap();
        thread::spawn(move || serve(listener_rec, || {}, || {}, || true, || {}, || {}));

        // 録画 OFF 状態のロック
        let ch_off = unique_id("i");
        let owned_off = match acquire(&ch_off).unwrap() {
            AcquireResult::Owned(h) => h,
            _ => panic!(),
        };
        let mut h_off = owned_off;
        let listener_off = h_off.take_listener().unwrap();
        thread::spawn(move || serve(listener_off, || {}, || {}, || false, || {}, || {}));

        thread::sleep(Duration::from_millis(50));

        let info_rec = read_existing(&ch_rec).expect("alive");
        let info_off = read_existing(&ch_off).expect("alive");
        assert!(query_recording(info_rec.ipc_addr).unwrap());
        assert!(!query_recording(info_off.ipc_addr).unwrap());

        drop(h_rec);
        drop(h_off);
    }

    #[test]
    fn lock_path_normalises_dangerous_chars() {
        // path traversal / 制御文字を含む channel_id が来ても
        // フィルタリングされる (ASCII 英数のみ)。
        let p = lock_path("../../../etc/passwd");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "ch-etcpasswd.lock");

        let p = lock_path("abc\0def");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "ch-abcdef.lock");
    }

    #[test]
    fn lock_path_empty_uses_invalid_marker() {
        // 全部削られて空になる場合は別個のファイルへ。複数の不正
        // channel_id が同じ lock ファイルを取り合うのを防ぐ。
        let p = lock_path("");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "ch-invalid.lock");

        let p = lock_path("..");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "ch-invalid.lock");

        let p = lock_path("///");
        let name = p.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "ch-invalid.lock");
    }

    #[test]
    fn read_existing_finds_live_owner() {
        let ch = unique_id("d");
        let owned = match acquire(&ch).unwrap() {
            AcquireResult::Owned(h) => h,
            AcquireResult::Conflict(_) => panic!(),
        };
        let mut h = owned;
        let listener = h.take_listener().unwrap();
        let stop = Arc::new(Mutex::new(false));
        let _stop_c = stop.clone();
        thread::spawn(move || serve(listener, || {}, || {}, || false, || {}, || {}));
        let info = read_existing(&ch).expect("live owner not found");
        assert_eq!(info.channel_id, ch);
        drop(h);
    }
}

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
const OK: &[u8] = b"ok\n";

/// 起動時の重複起動ポリシー。`LaunchPolicy::Single` の時のみ実際に
/// `single_instance::acquire` が呼ばれる想定。`NewWindow` は別 channel
/// なら常に新規起動、`Replace` は既存を kill して自分が取って代わる。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchPolicy {
    #[default]
    NewWindow,
    Replace,
    Single,
}

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
    let safe: String = channel_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    lock_dir().join(format!("ch-{safe}.lock"))
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
    let mut buf = [0u8; 8];
    let n = stream.read(&mut buf).unwrap_or(0);
    n >= PONG.len() && buf.starts_with(PONG)
}

/// 既存プロセスにフォーカス要求を送る。
pub fn request_focus(addr: SocketAddr) -> std::io::Result<()> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(FOCUS)?;
    let mut buf = [0u8; 8];
    let _ = stream.read(&mut buf);
    Ok(())
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
/// `on_focus` は `focus\n` を受け取った時に呼ばれる (UI スレッド外なので
/// 内部で `app.run_on_main_thread` 等を使うこと)。
pub fn serve<F: Fn() + Send + 'static>(listener: TcpListener, on_focus: F) {
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
            serve(listener, move || {
                focused_c.fetch_add(1, Ordering::SeqCst);
            });
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
        thread::spawn(move || serve(listener, || {}));
        let info = read_existing(&ch).expect("live owner not found");
        assert_eq!(info.channel_id, ch);
        drop(h);
    }
}

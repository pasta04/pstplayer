pub mod schema;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::util::errors::{AppError, AppResult};
pub use schema::Config;

/// プロセス内で並行する load+modify+save シーケンスを直列化するための
/// ロック。`update()` と `save()` が共有する。
///
/// 同一プロセス内のレースしか保護できない (別プロセスが同じ config.toml
/// を書きに来た場合は防げない) が、本アプリでは Desktop = 単一 Tauri
/// プロセス / pst-server = 単一 axum プロセスで完結するので十分。
static CONFIG_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Resolve the OS-standard config directory for PSTPlayer.
///
/// * Windows: `%APPDATA%\PSTPlayer\`
/// * macOS:   `~/Library/Application Support/PSTPlayer/`
/// * Linux:   `${XDG_CONFIG_HOME:-~/.config}/PSTPlayer/`
pub fn config_dir() -> AppResult<PathBuf> {
    let dirs = directories::ProjectDirs::from("io.github", "pasta04", "PSTPlayer")
        .ok_or_else(|| AppError::Decode("could not resolve project directory".into()))?;
    Ok(dirs.config_dir().to_path_buf())
}

pub fn config_path() -> AppResult<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// Load the config file, returning defaults if the file does not yet
/// exist. Returns an error on I/O or parse failures.
pub fn load() -> AppResult<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| AppError::Decode(format!("read {}: {e}", path.display())))?;
    let mut cfg: Config =
        toml::from_str(&raw).map_err(|e| AppError::Decode(format!("parse config: {e}")))?;
    // 旧形式のお気に入りルール (フィールド別パターン) を新形式
    // (pattern + 対象フィールド) へ自動移行する。保存時に新形式で
    // 書き出される。
    crate::favorites::migrate_rules(&mut cfg.favorites.rules);
    Ok(cfg)
}

/// Best-effort load: パース失敗時は破損ファイルをタイムスタンプ付き
/// `.bak.YYYYMMDD_HHMMSS` にバックアップして、デフォルト Config を
/// 返す。起動 (Tauri command) で使うとアプリが立ち上がらない状態を
/// 防げる。
///
/// 返り値の 2 要素目は「破損のためバックアップしたファイルのパス」。
/// None なら正常 load。Some(_) ならユーザーに通知してよい状況。
pub fn load_or_default() -> (Config, Option<PathBuf>) {
    match load() {
        Ok(cfg) => (cfg, None),
        Err(_) => {
            // バックアップを試みる (失敗してもデフォルトで起動する)。
            let bak = match config_path() {
                Ok(path) if path.exists() => {
                    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    let bak = path.with_extension(format!("toml.bak.{stamp}"));
                    if fs::copy(&path, &bak).is_ok() {
                        Some(bak)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            (Config::default(), bak)
        }
    }
}

/// Write the config to disk, creating the parent directory if needed.
/// 並行する load+save / save+save を直列化し、書き込みは tmp ファイル
/// 経由の atomic rename で行う (途中で停電 / クラッシュしても旧
/// `config.toml` が残る)。
pub fn save(cfg: &Config) -> AppResult<()> {
    let _guard = CONFIG_LOCK.lock().expect("config lock poisoned");
    save_locked(cfg)
}

/// `load` してから `modify` をかけて `save` するアトミックなヘルパ。
/// load+save 間に他の save が割り込めない (= TOCTOU race を防ぐ)。
/// 視聴履歴 / recent_hosts / ウィンドウ位置のような「部分書き換え」を
/// 行うコマンドが、独立した command 同士で並行に走った時に書き換え
/// 結果が消えないようにするため。
pub fn update<F>(modify: F) -> AppResult<Config>
where
    F: FnOnce(&mut Config),
{
    let _guard = CONFIG_LOCK.lock().expect("config lock poisoned");
    let mut cfg = load()?;
    modify(&mut cfg);
    save_locked(&cfg)?;
    Ok(cfg)
}

/// ロック取得済み前提の save。`save` と `update` の共通実装。
fn save_locked(cfg: &Config) -> AppResult<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Decode(format!("mkdir {}: {e}", parent.display())))?;
    }
    let body =
        toml::to_string_pretty(cfg).map_err(|e| AppError::Decode(format!("serialise: {e}")))?;
    // tmp ファイルに書き出してから rename で atomic 置換。std::fs::rename
    // は POSIX で atomic、Windows でも既存上書きが既定動作なので両 OS で
    // 「半端な書き込み途中の config.toml が残る」事故を防げる。
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, body).map_err(|e| AppError::Decode(format!("write {}: {e}", tmp.display())))?;
    fs::rename(&tmp, &path).map_err(|e| {
        AppError::Decode(format!(
            "rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_uses_pstplayer_dir() {
        let p = config_path().unwrap();
        let s = p.to_string_lossy().to_ascii_lowercase();
        assert!(s.contains("pstplayer"), "expected pstplayer in {s}");
        assert!(
            s.ends_with("config.toml"),
            "expected config.toml suffix in {s}"
        );
    }
}

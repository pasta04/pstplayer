//! Snapshot file naming and target-directory resolution.
//!
//! Behaviour per `docs/features.md` §1.1:
//!
//! * Default save directory is `<exe-dir>/snapshot/` (auto-created).
//! * Users can override with an absolute path or one starting with
//!   `~` (expands to the home directory).
//! * If the exe-relative target turns out to be unwritable (.app
//!   bundle, AppImage, `Program Files` without elevation, …) we fall
//!   back to `<OS Pictures>/PSTPlayer/snapshot/` and signal it so the
//!   caller can warn the user.

use std::path::{Path, PathBuf};

use crate::config::schema::PlayerConfig;

pub struct ResolvedDir {
    pub dir: PathBuf,
    pub fell_back: bool,
}

pub fn resolve_dir(cfg: &PlayerConfig, exe_dir: Option<&Path>) -> ResolvedDir {
    resolve_named(
        &cfg.snapshot_dir,
        exe_dir,
        "snapshot",
        pictures_dir,
        "snapshot",
    )
}

/// 録画ファイル用ディレクトリ解決。snapshot と同じ規則だが、
/// 既定サブディレクトリ名と OS 標準のフォールバック先 (Videos) が
/// 異なる。
pub fn resolve_record_dir(cfg: &PlayerConfig, exe_dir: Option<&Path>) -> ResolvedDir {
    resolve_named(
        &cfg.recording_dir,
        exe_dir,
        "recordings",
        videos_dir,
        "recordings",
    )
}

fn resolve_named(
    configured: &str,
    exe_dir: Option<&Path>,
    exe_subdir: &str,
    fallback_root: fn() -> Option<PathBuf>,
    fallback_subdir: &str,
) -> ResolvedDir {
    if !configured.trim().is_empty() {
        return ResolvedDir {
            dir: expand_tilde(configured),
            fell_back: false,
        };
    }
    if let Some(exe) = exe_dir {
        let candidate = exe.join(exe_subdir);
        if writable(&candidate) {
            return ResolvedDir {
                dir: candidate,
                fell_back: false,
            };
        }
    }
    let fallback = fallback_root()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PSTPlayer")
        .join(fallback_subdir);
    ResolvedDir {
        dir: fallback,
        fell_back: true,
    }
}

/// Build a filename per features.md §1.1: `{YYYYMMDD_HHmmss}_{channel}.{ext}`.
/// Characters that are illegal on common filesystems are replaced with `_`.
pub fn make_filename(channel_name: &str, ext: &str) -> String {
    let now = chrono::Local::now();
    let ts = now.format("%Y%m%d_%H%M%S");
    let safe: String = channel_name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name_part = if safe.trim().is_empty() {
        String::new()
    } else {
        format!("_{safe}")
    };
    let ext = ext.trim().trim_start_matches('.');
    format!("{ts}{name_part}.{ext}")
}

/// `dir/filename` が既に存在していたら、ファイル名末尾 (拡張子の前) に
/// `_2`, `_3`, ... を付けて衝突を回避する。録画やスナップショットを
/// 同秒内に複数取った時の上書きを防ぐ目的。
///
/// `ext` は拡張子の文字列 (`"png"` 等、ドットなし)。`filename` の末尾と
/// 一致している前提。最大 99 まで試して見つからなければ元の path を
/// 返す (= 上書きを許容。99 ファイル衝突は現実的にあり得ない)。
pub fn unique_path(dir: &Path, filename: &str, ext: &str) -> PathBuf {
    let p = dir.join(filename);
    if !p.exists() {
        return p;
    }
    let base = filename
        .strip_suffix(&format!(".{ext}"))
        .unwrap_or(filename);
    for counter in 2..100 {
        let candidate = dir.join(format!("{base}_{counter}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    p
}

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    if s == "~" {
        if let Some(home) = home_dir() {
            return home;
        }
    }
    PathBuf::from(s)
}

fn home_dir() -> Option<PathBuf> {
    directories::UserDirs::new().map(|d| d.home_dir().to_path_buf())
}

fn pictures_dir() -> Option<PathBuf> {
    directories::UserDirs::new().and_then(|d| d.picture_dir().map(|p| p.to_path_buf()))
}

fn videos_dir() -> Option<PathBuf> {
    directories::UserDirs::new().and_then(|d| d.video_dir().map(|p| p.to_path_buf()))
}

fn writable(p: &Path) -> bool {
    if let Some(parent) = p.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    if std::fs::create_dir_all(p).is_err() {
        return false;
    }
    let probe = p.join(".pstplayer-write-probe");
    let r = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_includes_channel_and_extension() {
        let n = make_filename("ニャム", "png");
        assert!(n.ends_with("_ニャム.png"));
    }

    #[test]
    fn filename_strips_unsafe_chars() {
        let n = make_filename("a/b:c*d?e", "png");
        assert!(n.contains("a_b_c_d_e"));
    }

    #[test]
    fn filename_handles_blank_channel() {
        let n = make_filename("   ", "png");
        // expect "YYYYMMDD_HHMMSS.png" with no channel suffix
        assert!(n.ends_with(".png"));
        assert!(!n.contains("__"));
    }

    #[test]
    fn filename_normalises_extension() {
        let n = make_filename("ch", ".jpg");
        assert!(n.ends_with("_ch.jpg"));
    }

    #[test]
    fn resolve_uses_explicit_dir_when_provided() {
        let cfg = PlayerConfig {
            snapshot_dir: "/tmp/foo".into(),
            ..PlayerConfig::default()
        };
        let r = resolve_dir(&cfg, None);
        assert_eq!(r.dir, PathBuf::from("/tmp/foo"));
        assert!(!r.fell_back);
    }

    #[test]
    fn unique_path_returns_original_when_not_existing() {
        let dir = std::env::temp_dir().join("pst-unique-test-a");
        let _ = std::fs::create_dir_all(&dir);
        let p = unique_path(&dir, "missing.png", "png");
        assert_eq!(p, dir.join("missing.png"));
    }

    #[test]
    fn unique_path_appends_counter_on_collision() {
        let dir = std::env::temp_dir().join("pst-unique-test-b");
        let _ = std::fs::create_dir_all(&dir);
        let occupied = dir.join("shot.png");
        let _ = std::fs::write(&occupied, b"x");
        let p = unique_path(&dir, "shot.png", "png");
        assert_eq!(p, dir.join("shot_2.png"));
        let _ = std::fs::remove_file(&occupied);
    }
}

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
    if !cfg.snapshot_dir.trim().is_empty() {
        return ResolvedDir {
            dir: expand_tilde(&cfg.snapshot_dir),
            fell_back: false,
        };
    }
    if let Some(exe) = exe_dir {
        let candidate = exe.join("snapshot");
        if writable(&candidate) {
            return ResolvedDir {
                dir: candidate,
                fell_back: false,
            };
        }
    }
    let fallback = pictures_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("PSTPlayer")
        .join("snapshot");
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
}

pub mod schema;

use std::fs;
use std::path::PathBuf;

use crate::util::errors::{AppError, AppResult};
pub use schema::Config;

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
    toml::from_str(&raw).map_err(|e| AppError::Decode(format!("parse config: {e}")))
}

/// Write the config to disk, creating the parent directory if needed.
pub fn save(cfg: &Config) -> AppResult<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Decode(format!("mkdir {}: {e}", parent.display())))?;
    }
    let body =
        toml::to_string_pretty(cfg).map_err(|e| AppError::Decode(format!("serialise: {e}")))?;
    fs::write(&path, body)
        .map_err(|e| AppError::Decode(format!("write {}: {e}", path.display())))?;
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

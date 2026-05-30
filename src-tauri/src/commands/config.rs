use crate::config::{self, Config};
use crate::util::errors::IpcError;

#[tauri::command]
pub fn get_config() -> Result<Config, IpcError> {
    config::load().map_err(Into::into)
}

#[tauri::command]
pub fn set_config(config: Config) -> Result<(), IpcError> {
    config::save(&config).map_err(Into::into)
}

#[tauri::command]
pub fn config_file_path() -> Result<String, IpcError> {
    config::config_path().map(|p| p.to_string_lossy().into_owned()).map_err(Into::into)
}

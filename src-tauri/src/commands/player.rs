use crate::player::engine::PlayerEngine;
use pst_core::util::errors::IpcError;
use tauri::State;

#[tauri::command]
pub fn player_load(url: String, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.load(&url).map_err(Into::into)
}

#[tauri::command]
pub fn player_stop(engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.stop().map_err(Into::into)
}

#[tauri::command]
pub fn player_set_pause(pause: bool, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.set_pause(pause).map_err(Into::into)
}

#[tauri::command]
pub fn player_set_volume(percent: u8, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.set_volume(percent).map_err(Into::into)
}

#[tauri::command]
pub fn player_set_mute(mute: bool, engine: State<'_, PlayerEngine>) -> Result<(), IpcError> {
    engine.set_mute(mute).map_err(Into::into)
}

#[derive(serde::Serialize)]
pub struct PlayerStatus {
    pub fps: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub time_pos: Option<f64>,
    pub paused: Option<bool>,
}

#[tauri::command]
pub fn player_status(engine: State<'_, PlayerEngine>) -> PlayerStatus {
    PlayerStatus {
        fps: engine.get_property::<f64>("estimated-vf-fps"),
        width: engine.get_property::<i64>("video-params/w"),
        height: engine.get_property::<i64>("video-params/h"),
        time_pos: engine.get_property::<f64>("time-pos"),
        paused: engine.get_property::<bool>("pause"),
    }
}

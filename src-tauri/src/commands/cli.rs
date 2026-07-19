use pst_core::cli::CliArgs;
use pst_core::config;
use pst_core::peercast::{client, types::PeerCastEndpoint};
use pst_core::util::errors::IpcError;
use tauri::State;

/// Surface the parsed CLI arguments to the frontend so it can drive
/// auto-play, suppress the BBS pane, etc.
#[tauri::command]
pub fn get_cli_args(args: State<'_, CliArgs>) -> CliArgs {
    (*args).clone()
}

/// Resolve the "default" PeerCast endpoint per the priority order
/// CLI URL > saved config > localhost:7144. Always returns an
/// endpoint with auth pulled from config (URLs do not carry creds).
#[tauri::command]
pub fn resolve_default_endpoint(args: State<'_, CliArgs>) -> Result<PeerCastEndpoint, IpcError> {
    let cfg = config::load().map_err(IpcError::from)?;
    Ok(client::resolve_endpoint(&args, &cfg.peercast))
}

/// High-level operations for reading and writing app configuration and state.
///
/// Wraps common::load_app_config, load_state, and save_state with
/// daemon-specific error context. All deploy and manage handlers
/// should go through this module instead of calling common directly.
use anyhow::Result;
use common::{AppConfig, AppState};
use std::path::PathBuf;

/// Resolve the app directory for a given app name.
pub fn app_dir(name: &str) -> PathBuf {
    common::app_dir(name)
}

/// Load and parse flare.toml from the app directory.
pub fn load_config(dir: &PathBuf) -> Result<AppConfig> {
    common::load_app_config(dir)
        .map_err(|e| anyhow::anyhow!("Failed to load flare.toml from {:?}: {}", dir, e))
}

/// Load app state from state.toml. Returns None if the file does not exist.
pub fn load_state(dir: &PathBuf) -> Result<Option<AppState>> {
    common::load_state(dir)
}

/// Persist app state to state.toml.
pub fn save_state(dir: &PathBuf, state: &AppState) -> Result<()> {
    common::save_state(dir, state)
}

/// Load state and require it to exist (app must have been deployed first).
pub fn load_state_or_bail(dir: &PathBuf) -> Result<AppState> {
    load_state(dir)?.ok_or_else(|| anyhow::anyhow!("App not found at {:?}. Deploy it first.", dir))
}

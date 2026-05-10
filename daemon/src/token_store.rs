use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persisted list of allowed token hashes for daemon authentication.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TokenStore {
    pub tokens: Vec<String>,
}

fn tokens_path() -> PathBuf {
    common::flare_dir().join("daemon_tokens.toml")
}

pub fn load_tokens() -> TokenStore {
    let path = tokens_path();
    if !path.exists() {
        return TokenStore::default();
    }

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    toml::from_str(&content).unwrap_or_default()
}

pub fn save_tokens(store: &TokenStore) -> Result<()> {
    let path = tokens_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, toml::to_string(store)?)?;
    Ok(())
}

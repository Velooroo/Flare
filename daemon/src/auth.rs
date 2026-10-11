//! Daemon-side token store: a list of argon2 hashes of accepted CLI tokens.
//!
//! INFO: `flare sync` generates a random token, hashes it, and sends only the
//!       hash here. The plain token never leaves the CLI.
//! WARN: Hashes are only ever appended — there is no revoke/expire path.
//! WARN: `load` silently falls back to an empty store if the file is corrupt,
//!       which would lock every CLI out without any visible error.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TokenStore {
    pub tokens: Vec<String>,
}

impl TokenStore {
    pub fn load() -> Self {
        let path = tokens_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => TokenStore::default(),
        }
    }

    fn save(&self) -> Result<()> {
        let path = tokens_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, toml::to_string(self)?)?;
        // INFO: Harden permissions (the file holds token hashes).
        common::restrict_permissions(&path);
        Ok(())
    }

    pub fn register(&mut self, token_hash: String) -> Result<()> {
        self.tokens.push(token_hash);
        self.save()?;
        info!("Registered new token");
        Ok(())
    }

    // INFO: True if the token matches any stored argon2 hash.
    pub fn verify(&self, token: &str) -> bool {
        self.tokens
            .iter()
            .any(|hash| common::verify_token(token, hash))
    }
}

fn tokens_path() -> PathBuf {
    common::flare_dir().join("daemon_tokens.toml")
}

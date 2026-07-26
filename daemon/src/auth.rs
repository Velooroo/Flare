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
        Ok(())
    }

    pub fn register(&mut self, token_hash: String) -> Result<()> {
        self.tokens.push(token_hash);
        self.save()?;
        info!("Registered new token");
        Ok(())
    }

    pub fn verify(&self, token: &str) -> bool {
        self.tokens
            .iter()
            .any(|hash| common::verify_token(token, hash))
    }
}

fn tokens_path() -> PathBuf {
    common::flare_dir().join("daemon_tokens.toml")
}

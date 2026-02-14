use crate::{AppConfig, AppState, Device, FlareConfig};
use anyhow::{Ok, Result};
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rand::RngCore;
use std::path::PathBuf;

pub fn flare_dir() -> PathBuf {
    // let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".flare")
}

pub fn apps_dir() -> PathBuf {
    std::env::var("FLARE_APPS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| flare_dir().join("apps"))
}

pub fn app_dir(name: &str) -> PathBuf {
    apps_dir().join(name.replace("/", "_"))
}

pub fn save_state(dir: &PathBuf, state: &AppState) -> Result<()> {
    let content = toml::to_string_pretty(state)?;
    std::fs::write(dir.join("state.toml"), content)?;
    Ok(())
}

pub fn load_state(dir: &PathBuf) -> Result<Option<AppState>> {
    let path = dir.join("state.toml");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)?;
    Ok(Some(toml::from_str(&content)?))
}

pub fn load_app_config(dir: &PathBuf) -> Result<AppConfig> {
    let path = dir.join("flare.toml");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Can't read {:?}: {}", path, e))?;
    Ok(toml::from_str(&content)?)
}

pub fn config_path() -> PathBuf {
    flare_dir().join("flare.conf")
}

pub fn load_config() -> Result<FlareConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(FlareConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&content)?)
}

pub fn save_config(config: &FlareConfig) -> Result<()> {
    let path = config_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn next_device_id(config: &FlareConfig) -> u32 {
    config.devices.iter().map(|d| d.id).max().unwrap_or(0) + 1
}

pub fn get_device(id_or_name: &str) -> Result<Device> {
    let config = load_config()?;

    // try by id
    if let Ok(id) = id_or_name.parse::<u32>() {
        if let Some(d) = config.devices.iter().find(|d| d.id == id) {
            return Ok(d.clone());
        }
    }

    // try by name
    if let Some(d) = config
        .devices
        .iter()
        .find(|d| d.name.as_deref() == Some(id_or_name))
    {
        return Ok(d.clone());
    }

    anyhow::bail!("Device not found: {}", id_or_name)
}

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_token(token: &str) -> Result<String> {
    use argon2::password_hash::rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Hash error: {}", e))?;
    Ok(hash.to_string())
}

pub fn verify_token(token: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(token.as_bytes(), &parsed_hash)
        .is_ok()
}

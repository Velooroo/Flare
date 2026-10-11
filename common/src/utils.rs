//! Filesystem paths and small helpers shared by the CLI and daemon.
//!
//! Layout under `~/.flare`:
//!   flare.conf        — CLI device list (saved by `flare sync`)
//!   auth.toml         — CLI git credentials (saved by `flare auth login`)
//!   daemon_tokens.toml— daemon-side argon2 hashes of accepted tokens
//!   apps/<name>/      — one directory per deployed app

use crate::{AppConfig, AppState, Device, FlareConfig};
use anyhow::Result;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rand::RngCore;
use std::path::PathBuf;

// INFO: Root of all Flare state (~/.flare). Falls back to /tmp when the home
//       directory cannot be determined, which mainly affects odd containers.
pub fn flare_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".flare")
}

// INFO: Honors $FLARE_APPS_DIR so apps can live outside the home directory.
pub fn apps_dir() -> PathBuf {
    std::env::var("FLARE_APPS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| flare_dir().join("apps"))
}

// INFO: App directories use `_` instead of `/`, so `user/repo` and `user_repo`
//       map to the same folder.
pub fn app_dir(name: &str) -> PathBuf {
    apps_dir().join(name.replace("/", "_"))
}

// INFO: Persisted per-app state (status, pid, port, health url). Lives at
//       <app_dir>/state.toml next to the deployed code.
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

// WARN: Looks for `flare.toml` directly in `dir`. Because archives unpack with a
//       top-level `owner-repo-sha/` folder, this only succeeds once that prefix
//       is stripped (it currently is not — see dispatch/deploy/core.rs).
pub fn load_app_config(dir: &PathBuf) -> Result<AppConfig> {
    let path = dir.join("flare.toml");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Can't read {:?}: {}", path, e))?;
    Ok(toml::from_str(&content)?)
}

// INFO: CLI-side config: the list of paired devices (`~/.flare/flare.conf`).
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
    std::fs::write(&path, content)?;
    // INFO: flare.conf holds plaintext device tokens; keep it owner-only.
    restrict_permissions(&path);
    Ok(())
}

// INFO: Best-effort hardening: restrict a file to its owner (mode 0600 on Unix).
//       No-op elsewhere. Errors are ignored on purpose.
pub fn restrict_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

// INFO: IDs are `max(existing) + 1`, so they keep growing after removals.
pub fn next_device_id(config: &FlareConfig) -> u32 {
    config.devices.iter().map(|d| d.id).max().unwrap_or(0) + 1
}

// INFO: Resolves a device by numeric id first, then by name.
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

// INFO: Reverse lookup used by `manage`: given a --host/--port, find a saved
//       device so the CLI can reuse its token for authentication.
pub fn find_device_by_addr(host: &str, port: u16) -> Option<Device> {
    let config = load_config().ok()?;
    config
        .devices
        .into_iter()
        .find(|d| d.host == host && d.port == port)
}

// INFO: 32 random bytes rendered as 64 hex characters.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

// INFO: Argon2id hash with a fresh random salt. Only the hash is stored by the daemon.
pub fn hash_token(token: &str) -> Result<String> {
    use argon2::password_hash::rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Hash error: {}", e))?;
    Ok(hash.to_string())
}

// INFO: Constant-time verification. A malformed hash simply returns `false`.
pub fn verify_token(token: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(token.as_bytes(), &parsed_hash)
        .is_ok()
}

//! Version storage for deployed apps.
//!
//! Layout under `~/.flare/apps/<app>/`:
//!   versions/<timestamp>/   one directory per deploy (the actual code)
//!   current                 a text file holding the live version's name
//!   state.toml              app state shared across versions (pid, status, ...)
//!
//! INFO: `current` is a plain file (not a symlink) so the daemon works on Windows
//!       without junction/symlink privileges. Rolling back just rewrites it.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

pub fn versions_dir(root: &Path) -> PathBuf {
    root.join("versions")
}

/// How many old versions to keep on disk (current + previous three).
pub const MAX_VERSIONS: usize = 4;

/// Creates a fresh, uniquely-named version directory and returns its path.
/// Timestamps are milliseconds, and we bump by 1 if two deploys collide.
pub fn new_version_dir(root: &Path) -> Result<PathBuf> {
    let dir = versions_dir(root);
    std::fs::create_dir_all(&dir)?;

    let mut stamp = chrono::Utc::now().timestamp_millis() as u128;
    while dir.join(stamp.to_string()).exists() {
        stamp += 1;
    }

    let version = dir.join(stamp.to_string());
    std::fs::create_dir_all(&version)?;
    Ok(version)
}

/// All version directories, oldest first.
pub fn list(root: &Path) -> Result<Vec<PathBuf>> {
    let dir = versions_dir(root);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            versions.push(entry.path());
        }
    }

    // Sort numerically by the timestamp name (lexicographic would break across
    // digit-length changes).
    versions.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.parse::<u128>().ok())
            .unwrap_or(0)
    });
    Ok(versions)
}

/// The version `current` points at, resolved to a path.
pub fn current(root: &Path) -> Option<PathBuf> {
    let name = std::fs::read_to_string(root.join("current")).ok()?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(versions_dir(root).join(name))
}

/// Points `current` at `version` by writing its name into the `current` file.
pub fn set_current(root: &Path, version: &Path) -> Result<()> {
    let name = match version.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => bail!("Invalid version path: {:?}", version),
    };
    std::fs::write(root.join("current"), name)?;
    Ok(())
}

/// The version immediately before the current one, if any.
pub fn previous(root: &Path) -> Result<Option<PathBuf>> {
    let versions = list(root)?;
    let cur = match current(root) {
        Some(c) => c,
        None => return Ok(None),
    };

    let idx = match versions.iter().position(|v| v == &cur) {
        Some(i) => i,
        None => return Ok(None),
    };

    if idx == 0 {
        Ok(None)
    } else {
        Ok(Some(versions[idx - 1].clone()))
    }
}

/// Deletes the oldest versions so that at most `keep` remain. The live version
/// (`current`) is never removed, even if the maths would include it.
pub fn prune(root: &Path, keep: usize) {
    let versions = match list(root) {
        Ok(v) => v,
        Err(_) => return,
    };
    if versions.len() <= keep {
        return;
    }

    let live = current(root);
    for version in &versions[..versions.len() - keep] {
        if Some(version) == live.as_ref() {
            continue;
        }
        let _ = std::fs::remove_dir_all(version);
    }
}

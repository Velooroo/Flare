//! Core deploy primitives: fetch a repo archive, unpack it, then build/run it.

use anyhow::Result;
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use tar::Archive;
use tracing::info;

use super::env::prepare_env;

// INFO: Hard cap for a downloaded source archive. Prevents a huge repo (or a
//       malicious server) from exhausting daemon memory.
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
// INFO: Overall HTTP timeout for the download.
const DOWNLOAD_TIMEOUT_SECS: u64 = 300;

// INFO: Fetches the repo archive. It is `async` so the daemon can `.await` it
//       directly on its own runtime. (An earlier version created a second Tokio
//       runtime and called `block_on`, which panics inside an async context.)
//
// URL: GitHub tarball API for `github`, otherwise the Gitea/Forgejo archive URL.
// Auth: bearer token for GitHub, or basic auth for other forges.
pub async fn download_source(
    repo: &str,
    forge: &str,
    branch: Option<&str>,
    auth_user: Option<&str>,
    auth_password: Option<&str>,
) -> Result<Vec<u8>> {
    let branch = branch.unwrap_or("main");

    let url = if forge == "github" {
        format!("https://api.github.com/repos/{}/tarball/{}", repo, branch)
    } else {
        // Gitea/Forgejo archive endpoint; the ref is optional but we always send it.
        format!("{}/git/{}/archive/{}.tar.gz", forge, repo, branch)
    };

    info!("Downloading {} (branch {})", url, branch);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()?;
    let mut r = client.get(&url).header("User-Agent", "Flared");

    // NOTE: For non-GitHub forges a username is required to send basic auth.
    if let Some(pass) = auth_password {
        if forge == "github" {
            r = r.bearer_auth(pass);
        } else if let Some(user) = auth_user {
            r = r.basic_auth(user, Some(pass));
        }
    }

    let mut resp = r.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }

    // INFO: Reject early when the server tells us it is too big.
    if let Some(len) = resp.content_length() {
        if len > MAX_ARCHIVE_BYTES {
            anyhow::bail!(
                "Archive too large: {} bytes (max {})",
                len,
                MAX_ARCHIVE_BYTES
            );
        }
    }

    // INFO: Stream the body in chunks and enforce the cap as we go.
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if buf.len() as u64 + chunk.len() as u64 > MAX_ARCHIVE_BYTES {
            anyhow::bail!("Archive exceeds {} bytes", MAX_ARCHIVE_BYTES);
        }
        buf.extend_from_slice(&chunk);
    }

    Ok(buf)
}

// INFO: Unpacks the archive into a new `versions/<timestamp>/` directory and
//       re-points `current` at it. Returns the directory that now holds the code.
// INFO: The archive's top-level folder (`owner-repo-sha/`) is stripped so that
//       `flare.toml` lands at the root of the version directory.
pub fn extract_archive(repo: &str, data: &[u8]) -> Result<PathBuf> {
    let root = common::app_dir(repo);
    let version = crate::versions::new_version_dir(&root)?;

    let gz = GzDecoder::new(Cursor::new(data));
    let mut archive = Archive::new(gz);

    // INFO: If unpacking fails, remove the half-written version dir so we do not
    //       leave an empty/garbage version behind.
    if let Err(e) = unpack_stripped(&mut archive, &version) {
        let _ = std::fs::remove_dir_all(&version);
        return Err(e);
    }

    crate::versions::set_current(&root, &version)?;
    // INFO: Keep disk usage bounded: retain only the newest versions.
    crate::versions::prune(&root, crate::versions::MAX_VERSIONS);
    info!("Extracted {} -> {:?} (current)", repo, version);
    Ok(version)
}

// INFO: Unpacks every entry, removing a single shared top-level directory, and
//       guarding against path traversal (`..`, absolute paths).
fn unpack_stripped<R: std::io::Read>(archive: &mut Archive<R>, dest: &PathBuf) -> Result<()> {
    use std::ffi::OsString;
    use std::path::Component;

    std::fs::create_dir_all(dest)?;

    let mut top: Option<OsString> = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();

        // Security: never trust archive paths.
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
        {
            continue;
        }

        let mut comps = path.components();
        let first = comps.next().map(|c| c.as_os_str().to_os_string());

        // Lock in the top-level folder from the first entry we see.
        if top.is_none() {
            top = first.clone();
        }

        // Drop the first component only when it is that shared top-level folder.
        let stripped: PathBuf = if first.is_some() && first == top {
            comps.map(|c| c.as_os_str()).collect()
        } else {
            path.clone()
        };

        if stripped.as_os_str().is_empty() {
            continue; // the top-level directory entry itself
        }

        let out = dest.join(&stripped);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            entry.unpack(&out)?;
        }
    }

    Ok(())
}

// INFO: Runs the `[build].command` in the app directory. Non-zero exit = failure.
pub fn run_build(cmd: &str, dir: &PathBuf) -> Result<()> {
    info!("Building: {}", cmd);
    let status = crate::platform::shell(cmd).current_dir(dir).status()?;

    if !status.success() {
        anyhow::bail!("Build failed");
    }
    Ok(())
}

// INFO: Spawns a process in its own group and reaps it in a background thread so
//       the daemon does not accumulate zombies. Returns the pid (also the pgid).
pub fn spawn_and_track(mut cmd: Command) -> Result<u32> {
    let mut child = cmd.spawn()?;
    let pid = child.id();

    // INFO: Wait in the background; dropping a Child without waiting would leave
    //       a zombie once the app exits.
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(pid)
}

// INFO: Spawns the app and returns its pid.
pub fn start_app(
    run: &common::RunSection,
    env: Option<&HashMap<String, String>>,
    isolation: Option<&common::IsolationSection>,
    dir: &PathBuf,
) -> Result<Option<u32>> {
    let cmd = build_run_command(run, env, isolation, dir)?;
    let pid = spawn_and_track(cmd)?;
    info!("Started PID {}", pid);
    Ok(Some(pid))
}

// INFO: Builds the Command that launches an app, applying isolation and env vars.
// WARN: `chroot` is Unix-only and sets a working directory inside the new root
//       that generally does not contain `sh`; on other platforms it is ignored.
//       `systemd` apps are launched as a systemd unit (see crate::systemd).
pub fn build_run_command(
    run: &common::RunSection,
    env: Option<&HashMap<String, String>>,
    isolation: Option<&common::IsolationSection>,
    dir: &PathBuf,
) -> Result<Command> {
    let final_vars = prepare_env(dir, env)?;
    let isolation_type = isolation.map(|i| i.r#type.as_str());

    let mut cmd = match isolation_type {
        #[cfg(unix)]
        Some("chroot") => {
            let mut c = Command::new("chroot");
            c.arg(dir).args(["sh", "-c", &run.command]);
            c.envs(final_vars);
            c
        }
        // INFO: Default (and any unknown value): run the command in a shell.
        _ => {
            let mut c = crate::platform::shell(&run.command);
            c.envs(final_vars);
            c
        }
    };

    cmd.current_dir(dir);
    // INFO: Put the app in its own process group so we can signal the whole tree
    //       (not just the shell) on stop.
    crate::platform::configure_process_group(&mut cmd);
    Ok(cmd)
}

// INFO: Waits 5 s, then probes the health URL once. Purely informational.
pub fn spawn_health_check(url: &str, name: &str) {
    let url = url.to_string();
    let name = name.to_string();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let result = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        match result {
            Ok(r) if r.status().is_success() => {
                info!("Health OK: {}", name);
            }
            Ok(r) => {
                tracing::warn!("Health failed {}: HTTP {}", name, r.status());
            }
            Err(e) => {
                tracing::error!("Health failed {}: {}", name, e);
            }
        }
    });
}

// INFO: Chooses the URL to probe.
//   * `auto_add = true` -> probe the app's own `[run].port` at `/health`.
//   * otherwise         -> use the explicit `[health].url`.
// NOTE: `app_name` is accepted for logging/back-compat but the auto URL no longer
//       uses the old (never-implemented) port 7531 collector.
pub fn determine_health_url(
    health: Option<&common::HealthSection>,
    run: Option<&common::RunSection>,
    _app_name: &str,
) -> Option<String> {
    let health = health?;

    if health.auto_add {
        let port = run.and_then(|r| r.port)?;
        Some(format!("http://localhost:{}/health", port))
    } else if health.url.is_empty() {
        None
    } else {
        Some(health.url.clone())
    }
}

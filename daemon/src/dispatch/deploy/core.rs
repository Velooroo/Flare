use anyhow::Result;
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use tar::Archive;
use tracing::info;

use super::env::prepare_env;

pub fn download_source(repo: &str, forge: &str, auth_user: Option<&str>, auth_password: Option<&str>) -> Result<Vec<u8>> {
    tokio::runtime::Runtime::new()?.block_on(download_source_async(repo, forge, auth_user, auth_password))
}

async fn download_source_async(repo: &str, forge: &str, auth_user: Option<&str>, auth_password: Option<&str>) -> Result<Vec<u8>> {
    let url = if forge == "github" {
        format!("https://api.github.com/repos/{}/tarball/main", repo)
    } else {
        format!("{}/git/{}/archive", forge, repo)
    };

    info!("Downloading {}", url);

    let client = reqwest::Client::new();
    let mut r = client.get(&url).header("User-Agent", "Flared");

    if let Some(pass) = auth_password {
        if forge == "github" {
            r = r.bearer_auth(pass);
        } else if let Some(user) = auth_user {
            r = r.basic_auth(user, Some(pass));
        }
    }

    let resp = r.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }

    Ok(resp.bytes().await?.to_vec())
}

pub fn extract_archive(repo: &str, data: &[u8]) -> Result<PathBuf> {
    let dir = common::app_dir(repo);
    std::fs::create_dir_all(&dir)?;

    backup_current(&dir)?;

    let gz = GzDecoder::new(Cursor::new(data));
    Archive::new(gz).unpack(&dir)?;

    info!("Extracted to {:?}", dir);
    Ok(dir)
}

fn backup_current(dir: &PathBuf) -> Result<()> {
    let current = dir.join("current");
    if !current.exists() {
        return Ok(());
    }

    let ts = chrono::Utc::now().timestamp();
    let backup = dir.join("versions").join(ts.to_string());
    std::fs::create_dir_all(backup.parent().unwrap())?;

    if let Ok(target) = std::fs::read_link(&current) {
        std::fs::rename(target, &backup)?;
    }
    Ok(())
}

pub fn run_build(cmd: &str, dir: &PathBuf) -> Result<()> {
    info!("Building: {}", cmd);
    let status = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(dir)
        .status()?;

    if !status.success() {
        anyhow::bail!("Build failed");
    }
    Ok(())
}

pub fn start_app(
    run: &common::RunSection,
    env: Option<&HashMap<String, String>>,
    isolation: Option<&common::IsolationSection>,
    dir: &PathBuf,
) -> Result<Option<u32>> {
    let mut cmd = build_run_command(run, env, isolation, dir);
    let child = cmd.spawn()?;
    let pid = child.id();
    info!("Started PID {}", pid);
    Ok(Some(pid))
}

pub fn build_run_command(
    run: &common::RunSection,
    env: Option<&HashMap<String, String>>,
    isolation: Option<&common::IsolationSection>,
    dir: &PathBuf,
) -> Command {
    let final_vars = prepare_env(dir, env);
    let isolation_type = isolation.map(|i| i.r#type.as_str());

    let mut cmd = match isolation_type {
        Some("systemd") => {
            let mut c = Command::new("systemd-run");
            c.args(["--user", "--scope"]);

            for (k, v) in final_vars {
                c.arg(format!("--setenv={}={}", k, v));
            }

            c.args(["sh", "-c", &run.command]);
            c
        }
        Some("chroot") => {
            let mut c = Command::new("chroot");
            c.arg(dir).args(["sh", "-c", &run.command]);
            c.envs(final_vars);
            c
        }
        _ => {
            let mut c = Command::new("sh");
            c.args(["-c", &run.command]);
            c.envs(final_vars);
            c
        }
    };

    cmd.current_dir(dir);
    cmd
}

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

pub fn determine_health_url(
    health: Option<&common::HealthSection>,
    app_name: &str,
) -> Option<String> {
    let health = health?;

    if health.auto_add {
        Some(format!("http://localhost:7531/health/{}", app_name))
    } else {
        Some(health.url.clone())
    }
}

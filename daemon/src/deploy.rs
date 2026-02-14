use anyhow::Result;
use common::{AppConfig, AppState, DeployRequest};
use common::{app_dir, load_app_config, save_state};
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::env;
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use tar::Archive;
use tracing::{info, warn};

use crate::env_loader::prepare_env;
use crate::server::HealthPids;
use crate::server::Routes;

pub async fn run(req: &DeployRequest, routes: Routes, health_pids: HealthPids) -> Result<PathBuf> {
    let archive = download(req).await?;
    let dir = extract(&req.repo, &archive)?;
    let config = load_app_config(&dir)?;

    crate::hooks::run_pre(&config, &dir);

    if let Some(build) = &config.build {
        build_app(&build.command, &dir)?;
    }

    if let Some(db) = &config.database {
        crate::database::setup(db, &dir)?;
    }

    let pid = start(&config, &dir, routes.clone()).await?;

    // Register PID for health check
    let app_name = &config.app.name;
    crate::health_server::update_pid(&health_pids, app_name, pid);

    let state = AppState {
        name: config.app.name.clone(),
        version: config.app.version.clone(),
        status: "running".into(),
        pid,
        port: config.run.as_ref().and_then(|r| r.port),
        health_url: determine_health_url(&config.health, app_name),
        isolation: config.isolation.as_ref().map(|i| i.r#type.clone()),
    };
    save_state(&dir, &state)?;

    // Spawn health check only if url is set
    if let Some(url) = &config.health {
        if let Some(health_url) = determine_health_url(&config.health, app_name) {
            spawn_health_check(&health_url, app_name);
        }
    }

    crate::hooks::run_post(&config, &dir);

    Ok(dir)
}

async fn download(req: &DeployRequest) -> Result<Vec<u8>> {
    let url = if req.forge == "github" {
        format!("https://api.github.com/repos/{}/tarball/main", req.repo)
    } else {
        format!("{}/git/{}/archive", req.forge, req.repo)
    };

    info!("Downloading {}", url);

    let client = reqwest::Client::new();
    let mut r = client.get(&url).header("User-Agent", "Flared");

    if let Some(pass) = &req.auth_password {
        if req.forge == "github" {
            r = r.bearer_auth(pass);
        } else if let Some(user) = &req.auth_user {
            r = r.basic_auth(user, Some(pass));
        }
    }

    let resp = r.send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }

    Ok(resp.bytes().await?.to_vec())
}

fn extract(repo: &str, data: &[u8]) -> Result<PathBuf> {
    let dir = app_dir(repo);
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

fn build_app(cmd: &str, dir: &PathBuf) -> Result<()> {
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

async fn start(config: &AppConfig, dir: &PathBuf, routes: Routes) -> Result<Option<u32>> {
    if let Some(web) = &config.web {
        let root = dir.join(web.root.as_deref().unwrap_or("."));
        routes
            .write()
            .await
            .static_sites
            .insert(web.domain.clone(), root.to_string_lossy().into());
        info!("Static site: {} -> {:?}", web.domain, root);
        return Ok(None);
    }

    let run = match &config.run {
        Some(r) => r,
        None => return Ok(None),
    };

    let mut cmd = build_run_command(run, config, dir);
    let child = cmd.spawn()?;
    let pid = child.id();

    info!("Started PID {}", pid);
    Ok(Some(pid))
}

fn build_run_command(run: &common::RunSection, config: &AppConfig, dir: &PathBuf) -> Command {
    let final_vars = prepare_env(dir, config.env.as_ref());
    let isolation = config.isolation.as_ref().map(|i| i.r#type.as_str());

    let mut cmd = match isolation {
        Some("systemd") => {
            let mut c = Command::new("systemd-run");
            c.args(["--user", "--scope"]);

            // env
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
            c.envs(final_vars); // Применяем переменные
            c
        }
    };

    cmd.current_dir(dir);
    cmd
}

fn spawn_health_check(url: &str, name: &str) {
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

fn determine_health_url(health: &Option<common::HealthSection>, app_name: &str) -> Option<String> {
    // Option<&HealthSection> -> &HealthSection
    let health = health.as_ref()?;

    if health.auto_add {
        Some(format!("http://localhost:7531/health/{}", app_name))
    } else {
        Some(health.url.clone())
    }
}

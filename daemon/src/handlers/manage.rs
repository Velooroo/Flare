use anyhow::Result;
use common::{ManageRequest, ManageResponse, send_json};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::info;

use crate::app_config;
use crate::deploy;

pub async fn handle_manage(
    mut socket: TlsStream<TcpStream>,
    req: ManageRequest,
) -> Result<()> {
    let result = match req.action.as_str() {
        "start" => start_app(&req.app),
        "stop" => stop_app(&req.app),
        "restart" => restart_app(&req.app),
        "rollback" => rollback_app(&req.app),
        _ => Err(anyhow::anyhow!("Unknown action: {}", req.action)),
    };

    let response = match result {
        Ok(msg) => ManageResponse {
            success: true,
            message: msg,
        },
        Err(e) => ManageResponse {
            success: false,
            message: e.to_string(),
        },
    };

    send_json(&mut socket, &response).await
}

fn start_app(app: &str) -> Result<String> {
    let dir = app_config::app_dir(app);
    let mut state = app_config::load_state_or_bail(&dir)?;

    if state.status == "running" {
        return Ok("Already running".into());
    }

    let config = app_config::load_config(&dir)?;
    let run = config
        .run
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No [run] section in config"))?;
    let mut cmd = deploy::build_run_command(run, config.env.as_ref(), config.isolation.as_ref(), &dir);
    let child = cmd.spawn()?;
    let pid = child.id();

    state.status = "running".into();
    state.pid = Some(pid);
    app_config::save_state(&dir, &state)?;

    info!("Started {} with PID {}", app, pid);
    Ok(format!("Started with PID {}", pid))
}

fn stop_app(app: &str) -> Result<String> {
    let dir = app_config::app_dir(app);
    let mut state = app_config::load_state_or_bail(&dir)?;

    if let Some(pid) = state.pid {
        let status = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status()?;

        if !status.success() {
            info!("kill {} returned non-zero (process may already be dead)", pid);
        }
    }

    state.status = "stopped".into();
    state.pid = None;
    app_config::save_state(&dir, &state)?;

    info!("Stopped {}", app);
    Ok("Stopped".into())
}

fn restart_app(app: &str) -> Result<String> {
    stop_app(app)?;
    std::thread::sleep(Duration::from_millis(500));
    start_app(app)
}

fn rollback_app(app: &str) -> Result<String> {
    let dir = app_config::app_dir(app);
    let versions = dir.join("versions");

    let mut entries: Vec<_> = std::fs::read_dir(&versions)?
        .filter_map(|e| e.ok())
        .collect();

    entries.sort_by_key(|e| e.path());
    let latest = entries
        .last()
        .ok_or_else(|| anyhow::anyhow!("No backups found for {}", app))?;

    let current = dir.join("current");
    if current.exists() {
        std::fs::remove_file(&current)?;
    }

    std::os::unix::fs::symlink(latest.path(), &current)?;
    info!("Rolled back {} to {:?}", app, latest.path());

    // restart if running
    let state = app_config::load_state(&dir)?;
    if let Some(s) = state {
        if s.status == "running" {
            restart_app(app)?;
        }
    }

    Ok("Rolled back".into())
}

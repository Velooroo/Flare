pub mod core;
pub mod database;
pub mod env;

use anyhow::Result;
use common::{
    AppConfig, AppState, BuildSection, DatabaseSection, DeployRequest, DeployResponse,
    HealthSection, HooksSection, IsolationSection, RunSection, WebSection, send_json,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::{info, warn};

use crate::app_config;
use crate::dispatch::token;
use crate::dispatch::manage::{HealthPids, update_health_pid};
use crate::server::Routes;
pub use core::build_run_command;

// — handler —

pub async fn handle_deploy(
    mut socket: TlsStream<TcpStream>,
    routes: Routes,
    health_pids: HealthPids,
    req: DeployRequest,
) -> Result<()> {
    let store = token::load_tokens();
    let token = req.daemon_token.as_deref().unwrap_or("");
    let valid = store
        .tokens
        .iter()
        .any(|hash| common::verify_token(token, hash));

    if !valid {
        warn!("Invalid token for deploy");
        send_json(&mut socket, &DeployResponse {
            success: false,
            message: "Invalid token".into(),
            app_dir: None,
        }).await?;
        return Ok(());
    }

    info!("Deploy: {}", req.repo);

    let response = match run(&req).await {
        Ok(result) => {
            if let Some((domain, root)) = &result.static_site {
                routes.write().await.static_sites.insert(domain.clone(), root.clone());
            }
            update_health_pid(&health_pids, &result.name, result.pid);
            DeployResponse {
                success: true,
                message: format!("Deployed to {}", result.dir.display()),
                app_dir: Some(result.dir.to_string_lossy().into()),
            }
        }
        Err(e) => DeployResponse {
            success: false,
            message: e.to_string(),
            app_dir: None,
        },
    };

    send_json(&mut socket, &response).await
}

// — deploy result —

pub struct DeployResult {
    pub dir: PathBuf,
    pub name: String,
    pub pid: Option<u32>,
    pub static_site: Option<(String, String)>,
}

// — config projection —

struct DeployInput<'a> {
    name: &'a str,
    version: &'a str,
    build: Option<&'a BuildSection>,
    database: Option<&'a DatabaseSection>,
    run: Option<&'a RunSection>,
    web: Option<&'a WebSection>,
    health: Option<&'a HealthSection>,
    env: Option<&'a HashMap<String, String>>,
    isolation: Option<&'a IsolationSection>,
    hooks: Option<&'a HooksSection>,
    port: Option<u16>,
}

impl<'a> From<&'a AppConfig> for DeployInput<'a> {
    fn from(cfg: &'a AppConfig) -> Self {
        Self {
            name: &cfg.app.name,
            version: &cfg.app.version,
            build: cfg.build.as_ref(),
            database: cfg.database.as_ref(),
            run: cfg.run.as_ref(),
            web: cfg.web.as_ref(),
            health: cfg.health.as_ref(),
            env: cfg.env.as_ref(),
            isolation: cfg.isolation.as_ref(),
            hooks: cfg.hooks.as_ref(),
            port: cfg.run.as_ref().and_then(|r| r.port),
        }
    }
}

// — orchestrator —

async fn run(req: &DeployRequest) -> Result<DeployResult> {
    let archive = core::download_source(req).await?;
    let dir = core::extract_archive(&req.repo, &archive)?;
    let config = app_config::load_config(&dir)?;
    let input = DeployInput::from(&config);

    validate(&input)?;
    run_hook(input.hooks, &dir, HookPhase::Pre);

    if let Some(build) = input.build {
        core::run_build(&build.command, &dir)?;
    }

    if let Some(db) = input.database {
        database::setup(db, &dir)?;
    }

    // start (or serve static site)
    let pid: Option<u32>;
    let static_site: Option<(String, String)>;

    if let Some(web) = input.web {
        let root = dir.join(web.root.as_deref().unwrap_or("."));
        static_site = Some((web.domain.clone(), root.to_string_lossy().to_string()));
        pid = None;
    } else if let Some(run) = input.run {
        static_site = None;
        pid = core::start_app(run, input.env, input.isolation, &dir).await?;
    } else {
        static_site = None;
        pid = None;
    };

    let state = AppState {
        name: input.name.to_string(),
        version: input.version.to_string(),
        status: "running".into(),
        pid,
        port: input.port,
        health_url: core::determine_health_url(input.health, input.name),
        isolation: input.isolation.map(|i| i.r#type.clone()),
    };
    app_config::save_state(&dir, &state)?;

    if let Some(h) = input.health {
        if let Some(url) = core::determine_health_url(Some(h), input.name) {
            core::spawn_health_check(&url, input.name);
        }
    }

    run_hook(input.hooks, &dir, HookPhase::Post);

    Ok(DeployResult {
        dir,
        name: input.name.to_string(),
        pid,
        static_site,
    })
}

// — validation —

fn validate(input: &DeployInput) -> Result<()> {
    if input.name.is_empty() {
        anyhow::bail!("app.name is required in flare.toml");
    }

    if let Some(db) = input.database {
        let valid = ["postgres", "mysql", "sqlite"].contains(&db.r#type.as_str());
        if !valid {
            anyhow::bail!(
                "Unsupported database type: {}. Supported: postgres, mysql, sqlite",
                db.r#type
            );
        }
    }

    if let Some(h) = input.health {
        if !h.auto_add && h.url.is_empty() {
            anyhow::bail!("health.url is required when auto_add is false");
        }
    }

    Ok(())
}

// — hooks —

enum HookPhase { Pre, Post }

/// Commands allowed in hook scripts.
/// Only these binaries may appear as the first token of a hook command.
/// This prevents arbitrary shell execution if flare.toml is compromised.
const ALLOWED_HOOK_COMMANDS: &[&str] = &[
    "sh", "bash", "make", "npm", "yarn", "pnpm",
    "cargo", "go", "python", "python3", "pip", "pip3",
    "docker", "podman", "git", "chmod", "chown",
    "mkdir", "cp", "mv", "rm", "ln", "cat", "echo",
    "grep", "sed", "awk", "find", "xargs", "curl", "wget",
];

/// Validate that a hook command starts with an allowed binary.
/// Returns Err if the command is empty or uses a disallowed executable.
fn validate_hook_command(cmd: &str) -> Result<()> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Hook command is empty");
    }

    // Extract the first token (the executable name)
    let first_token = trimmed.split_whitespace().next().unwrap_or("");

    // Strip path prefix: "/usr/bin/npm" → "npm"
    let binary_name = std::path::Path::new(first_token)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(first_token);

    if !ALLOWED_HOOK_COMMANDS.contains(&binary_name) {
        anyhow::bail!(
            "Hook command '{}' is not in the allowlist. Allowed: {}",
            binary_name,
            ALLOWED_HOOK_COMMANDS.join(", ")
        );
    }

    Ok(())
}

fn run_hook(hooks: Option<&HooksSection>, dir: &PathBuf, phase: HookPhase) {
    let hooks = match hooks {
        Some(h) => h,
        None => return,
    };

    let cmd = match phase {
        HookPhase::Pre => hooks.pre_deploy.as_ref(),
        HookPhase::Post => hooks.post_deploy.as_ref(),
    };

    let label = match phase {
        HookPhase::Pre => "Pre-deploy",
        HookPhase::Post => "Post-deploy",
    };

    let cmd = match cmd {
        Some(c) => c,
        None => return,
    };

    // Security: validate command against allowlist before execution
    if let Err(e) = validate_hook_command(cmd) {
        warn!("{} hook rejected: {}", label, e);
        return;
    }

    info!("{}: {}", label, cmd);
    let status = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(_) => warn!("{} hook failed: {}", label, cmd),
        Err(e) => warn!("{} hook skipped: {}", label, e),
    }
}

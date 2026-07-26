use anyhow::Result;
use common::{AppConfig, AppState, DeployRequest};
use std::path::PathBuf;

pub struct DeployContext {
    pub repo: String,
    pub forge: String,
    pub auth_user: Option<String>,
    pub auth_password: Option<String>,
    pub dir: PathBuf,
    pub config: AppConfig,
    pub pid: Option<u32>,
    pub static_site: Option<(String, String)>,
}

#[allow(dead_code)]
impl DeployContext {
    fn from_request(req: &DeployRequest, dir: PathBuf, config: AppConfig) -> Self {
        Self {
            repo: req.repo.clone(),
            forge: if req.forge.is_empty() { "github".into() } else { req.forge.clone() },
            auth_user: req.auth_user.clone(),
            auth_password: req.auth_password.clone(),
            dir,
            config,
            pid: None,
            static_site: None,
        }
    }
}

pub enum Outcome {
    Ok {
        dir: PathBuf,
        name: String,
        pid: Option<u32>,
        port: Option<u16>,
        static_site: Option<(String, String)>,
        message: String,
    },
    Err {
        message: String,
    },
}

pub async fn run(req: &DeployRequest) -> Outcome {
    let steps: Vec<(&str, fn(&mut DeployContext) -> Result<()>)> = vec![
        ("download", download),
        ("load_config", load_config),
        ("validate", validate),
        ("hooks:pre", hooks_pre),
        ("build", build),
        ("database", database),
        ("run", run_app),
        ("save_state", save_state),
        ("health", health_check),
        ("hooks:post", hooks_post),
    ];

    let mut ctx = DeployContext {
        repo: req.repo.clone(),
        forge: if req.forge == "github" || req.forge.is_empty() { "github".into() } else { req.forge.clone() },
        auth_user: req.auth_user.clone(),
        auth_password: req.auth_password.clone(),
        dir: PathBuf::new(),
        config: AppConfig {
            app: common::AppSection { name: String::new(), version: String::new() },
            build: None,
            run: None,
            env: None,
            web: None,
            health: None,
            isolation: None,
            storage: None,
            database: None,
            notify: None,
            secrets: None,
            resource_limits: None,
            hooks: None,
            metrics: None,
            strategy: None,
        },
        pid: None,
        static_site: None,
    };

    for (name, step) in &steps {
        if let Err(e) = step(&mut ctx) {
            let name_str = ctx.config.app.name.clone();
            let label = if name_str.is_empty() { name.to_string() } else { format!("{} ({})", name, name_str) };
            return Outcome::Err {
                message: format!("Step {}: {}", label, e),
            };
        }
    }

    Outcome::Ok {
        dir: ctx.dir.clone(),
        name: ctx.config.app.name.clone(),
        pid: ctx.pid,
        port: ctx.config.run.as_ref().and_then(|r| r.port),
        static_site: ctx.static_site,
        message: format!("Deployed to {}", ctx.dir.display()),
    }
}

fn download(ctx: &mut DeployContext) -> Result<()> {
    let archive = crate::dispatch::deploy::core::download_source(&ctx.repo, &ctx.forge, ctx.auth_user.as_deref(), ctx.auth_password.as_deref())?;
    ctx.dir = crate::dispatch::deploy::core::extract_archive(&ctx.repo, &archive)?;
    Ok(())
}

fn load_config(ctx: &mut DeployContext) -> Result<()> {
    ctx.config = common::load_app_config(&ctx.dir)
        .map_err(|e| anyhow::anyhow!("Failed to load flare.toml: {}", e))?;
    Ok(())
}

fn validate(ctx: &mut DeployContext) -> Result<()> {
    if ctx.config.app.name.is_empty() {
        anyhow::bail!("app.name is required in flare.toml");
    }

    if let Some(db) = &ctx.config.database {
        let valid = ["postgres", "mysql", "sqlite"].contains(&db.r#type.as_str());
        if !valid {
            anyhow::bail!(
                "Unsupported database type: {}. Supported: postgres, mysql, sqlite",
                db.r#type
            );
        }
    }

    if let Some(h) = &ctx.config.health {
        if !h.auto_add && h.url.is_empty() {
            anyhow::bail!("health.url is required when auto_add is false");
        }
    }

    Ok(())
}

fn hooks_pre(ctx: &mut DeployContext) -> Result<()> {
    run_hook(&ctx.config, &ctx.dir, HookPhase::Pre);
    Ok(())
}

fn hooks_post(ctx: &mut DeployContext) -> Result<()> {
    run_hook(&ctx.config, &ctx.dir, HookPhase::Post);
    Ok(())
}

fn build(ctx: &mut DeployContext) -> Result<()> {
    if let Some(b) = &ctx.config.build {
        crate::dispatch::deploy::core::run_build(&b.command, &ctx.dir)?;
    }
    Ok(())
}

fn database(ctx: &mut DeployContext) -> Result<()> {
    if let Some(db) = &ctx.config.database {
        crate::dispatch::deploy::database::setup(db, &ctx.dir)?;
    }
    Ok(())
}

fn run_app(ctx: &mut DeployContext) -> Result<()> {
    if let Some(web) = &ctx.config.web {
        let root = ctx.dir.join(web.root.as_deref().unwrap_or("."));
        ctx.static_site = Some((web.domain.clone(), root.to_string_lossy().to_string()));
        ctx.pid = None;
    } else if let Some(run) = &ctx.config.run {
        ctx.pid = crate::dispatch::deploy::core::start_app(
            run,
            ctx.config.env.as_ref(),
            ctx.config.isolation.as_ref(),
            &ctx.dir,
        )?;
        ctx.static_site = None;
    } else {
        ctx.static_site = None;
        ctx.pid = None;
    }
    Ok(())
}

fn save_state(ctx: &mut DeployContext) -> Result<()> {
    let state = AppState {
        name: ctx.config.app.name.clone(),
        version: ctx.config.app.version.clone(),
        status: "running".into(),
        pid: ctx.pid,
        port: ctx.config.run.as_ref().and_then(|r| r.port),
        health_url: crate::dispatch::deploy::core::determine_health_url(
            ctx.config.health.as_ref(),
            &ctx.config.app.name,
        ),
        isolation: ctx.config.isolation.as_ref().map(|i| i.r#type.clone()),
    };
    common::save_state(&ctx.dir, &state)?;
    Ok(())
}

fn health_check(ctx: &mut DeployContext) -> Result<()> {
    if let Some(h) = &ctx.config.health {
        if let Some(url) = crate::dispatch::deploy::core::determine_health_url(
            Some(h),
            &ctx.config.app.name,
        ) {
            crate::dispatch::deploy::core::spawn_health_check(&url, &ctx.config.app.name);
        }
    }
    Ok(())
}

// ── hooks ──

enum HookPhase { Pre, Post }

const ALLOWED_HOOK_COMMANDS: &[&str] = &[
    "sh", "bash", "make", "npm", "yarn", "pnpm",
    "cargo", "go", "python", "python3", "pip", "pip3",
    "docker", "podman", "git", "chmod", "chown",
    "mkdir", "cp", "mv", "rm", "ln", "cat", "echo",
    "grep", "sed", "awk", "find", "xargs", "curl", "wget",
];

fn run_hook(config: &AppConfig, dir: &PathBuf, phase: HookPhase) {
    let hooks = match &config.hooks {
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

    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return;
    }

    let first = trimmed.split_whitespace().next().unwrap_or("");
    let bin = std::path::Path::new(first)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(first);

    if !ALLOWED_HOOK_COMMANDS.contains(&bin) {
        tracing::warn!("{} hook rejected: '{}' not in allowlist", label, bin);
        return;
    }

    tracing::info!("{}: {}", label, cmd);
    let status = std::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(_) => tracing::warn!("{} hook failed: {}", label, cmd),
        Err(e) => tracing::warn!("{} hook skipped: {}", label, e),
    }
}

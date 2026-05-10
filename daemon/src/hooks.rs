use anyhow::Result;
use common::HooksSection;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

pub fn run_pre(hooks: Option<&HooksSection>, dir: &PathBuf) -> Result<()> {
    if let Some(hooks) = hooks {
        if let Some(cmd) = &hooks.pre_deploy {
            info!("Pre-deploy: {}", cmd);
            let status = Command::new("sh")
                .args(["-c", cmd])
                .current_dir(dir)
                .status()?;

            if !status.success() {
                anyhow::bail!("Pre-deploy hook failed: {}", cmd);
            }
        }
    }
    Ok(())
}

pub fn run_post(hooks: Option<&HooksSection>, dir: &PathBuf) -> Result<()> {
    if let Some(hooks) = hooks {
        if let Some(cmd) = &hooks.post_deploy {
            info!("Post-deploy: {}", cmd);
            let status = Command::new("sh")
                .args(["-c", cmd])
                .current_dir(dir)
                .status()?;

            if !status.success() {
                anyhow::bail!("Post-deploy hook failed: {}", cmd);
            }
        }
    }
    Ok(())
}

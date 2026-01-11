use common::AppConfig;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

pub fn run_pre(config: &AppConfig, dir: &PathBuf) {
    if let Some(hooks) = &config.hooks {
        if let Some(cmd) = &hooks.pre_deploy {
            info!("Pre-deploy: {}", cmd);
            let _ = Command::new("sh")
                .args(["-c", cmd])
                .current_dir(dir)
                .status();
        }
    }
}

pub fn run_post(config: &AppConfig, dir: &PathBuf) {
    if let Some(hooks) = &config.hooks {
        if let Some(cmd) = &hooks.post_deploy {
            info!("Post-deploy: {}", cmd);
            let _ = Command::new("sh")
                .args(["-c", cmd])
                .current_dir(dir)
                .status();
        }
    }
}

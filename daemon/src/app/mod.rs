use anyhow::Result;
use common::AppState;
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

pub struct AppManager;

impl AppManager {

    // Return path for dir application
    pub fn app_dir(name: &str) -> PathBuf {
        common::app_dir(name)
    }

    // Check if process is alive (Return: True)
    pub fn process_alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    // Function about started deploy in daemon
    pub fn start(app: &str) -> Result<String> {
        let dir = Self::app_dir(app);
        let mut state = Self::load_state_or_fail(&dir)?;

        if state.status == "running" {
            return Ok("Already running".into());
        }

        let config = common::load_app_config(&dir)?;
        let run = config.run.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No [run] section in config"))?;

        let mut cmd = crate::dispatch::deploy::build_run_command(
            run,
            config.env.as_ref(),
            config.isolation.as_ref(),
            &dir,
        );
        let child = cmd.spawn()?;
        let pid = child.id();

        state.status = "running".into();
        state.pid = Some(pid);
        common::save_state(&dir, &state)?;

        info!("Started {} with PID {}", app, pid);
        Ok(format!("Started with PID {}", pid))
    }

    pub fn stop(app: &str) -> Result<String> {
        let dir = Self::app_dir(app);
        let mut state = Self::load_state_or_fail(&dir)?;

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
        common::save_state(&dir, &state)?;

        info!("Stopped {}", app);
        Ok("Stopped".into())
    }

    pub fn restart(app: &str) -> Result<String> {
        Self::stop(app)?;
        std::thread::sleep(Duration::from_millis(500));
        Self::start(app)
    }

    // TODO: Not work now, need check and fix it (I don't do all on this function)
    pub fn rollback(app: &str) -> Result<String> {
        let dir = Self::app_dir(app);
        let versions_dir = dir.join("versions");

        let mut entries: Vec<_> = std::fs::read_dir(&versions_dir)?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.path());

        let latest = entries.last()
            .ok_or_else(|| anyhow::anyhow!("No backups found for {}", app))?;

        let current = dir.join("current");
        if current.exists() {
            std::fs::remove_file(&current)?;
        }

        std::os::unix::fs::symlink(latest.path(), &current)?;
        info!("Rolled back {} to {:?}", app, latest.path());

        let state = common::load_state(&dir)?;
        if let Some(s) = state {
            if s.status == "running" {
                Self::restart(app)?;
            }
        }

        Ok("Rolled back".into())
    }

    fn load_state_or_fail(dir: &PathBuf) -> Result<AppState> {
        common::load_state(dir)?
            .ok_or_else(|| anyhow::anyhow!("App not found at {:?}. Deploy it first.", dir))
    }
}

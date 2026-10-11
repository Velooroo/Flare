//! App lifecycle on the daemon side: start, stop, restart, rollback.
//!
//! An app lives at `~/.flare/apps/<app>/` with:
//!   versions/<ts>/   one directory per deploy (the code)
//!   current          a text file holding the live version's name
//!   state.toml       shared state (status, pid) kept outside the versions
//!
//! Rollback simply re-points `current` at the previous version and restarts.
//!
//! INFO: Apps isolated with `[isolation].type = "systemd"` are managed through
//!       systemctl (start/stop/restart); everything else is a real child process
//!       put in its own process group so the whole tree can be signalled.
//!
//! WARN: These functions are synchronous and blocking; the request handlers run
//!       them on the blocking pool (see handlers::manage).

use anyhow::Result;
use common::AppState;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::info;

pub struct AppManager;

impl AppManager {
    // Check if process is alive (Return: True)
    pub fn process_alive(pid: u32) -> bool {
        crate::platform::process_alive(pid)
    }

    // INFO: Resolves the live code directory for an app (`<root>/current`).
    fn live_dir(root: &Path) -> Result<PathBuf> {
        crate::versions::current(root)
            .ok_or_else(|| anyhow::anyhow!("No active version for this app. Deploy it first."))
    }

    // INFO: Finds an app's root directory by either its directory name (the
    //       normalized repo name used by deploy) or its `[app].name` from
    //       `flare.toml`. This makes `flare start/stop/rollback <name>` work no
    //       matter which of the two the user typed.
    pub fn resolve_root(input: &str) -> Result<PathBuf> {
        let direct = common::app_dir(input);
        if direct.join("state.toml").exists() {
            return Ok(direct);
        }

        if let Ok(entries) = std::fs::read_dir(common::apps_dir()) {
            for entry in entries.flatten() {
                let root = entry.path();
                if !root.is_dir() {
                    continue;
                }
                if let Some(live) = crate::versions::current(&root) {
                    if let Ok(config) = common::load_app_config(&live) {
                        if config.app.name == input {
                            return Ok(root);
                        }
                    }
                }
            }
        }

        // Fall back so the caller's error message still shows a sensible path.
        Ok(direct)
    }

    fn uses_systemd(config: &common::AppConfig) -> bool {
        config
            .isolation
            .as_ref()
            .map(|i| i.r#type == "systemd")
            .unwrap_or(false)
    }

    // INFO: Returns the app's configured name if it is systemd-isolated, so the
    //       unit name matches what was installed at deploy time.
    fn systemd_unit_name(root: &Path) -> Option<String> {
        let live = Self::live_dir(root).ok()?;
        let config = common::load_app_config(&live).ok()?;
        if Self::uses_systemd(&config) {
            Some(config.app.name)
        } else {
            None
        }
    }

    // Function about started deploy in daemon
    pub fn start(app: &str) -> Result<String> {
        let root = Self::resolve_root(app)?;
        let mut state = Self::load_state_or_fail(&root)?;

        // INFO: Only report "already running" if the process is actually alive.
        if state.status == "running" {
            if let Some(pid) = state.pid {
                if Self::process_alive(pid) {
                    return Ok("Already running".into());
                }
            }
            // Stale state (process died): fall through and start it again.
        }

        let live = Self::live_dir(&root)?;
        let config = common::load_app_config(&live)?;
        let run = config
            .run
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No [run] section in config"))?;

        let pid = if Self::uses_systemd(&config) {
            // INFO: The unit is named after `[app].name`, not the CLI argument.
            crate::systemd::control("start", &config.app.name)?;
            crate::systemd::main_pid(&config.app.name)
        } else {
            let cmd = crate::dispatch::deploy::build_run_command(
                run,
                config.env.as_ref(),
                config.isolation.as_ref(),
                &live,
            )?;
            Some(crate::dispatch::deploy::core::spawn_and_track(cmd)?)
        };

        state.status = "running".into();
        state.pid = pid;
        common::save_state(&root, &state)?;

        match pid {
            Some(p) => {
                info!("Started {} with PID {}", app, p);
                Ok(format!("Started with PID {}", p))
            }
            None => {
                info!("Started {}", app);
                Ok("Started".into())
            }
        }
    }

    // INFO: Stops the app and waits for the process to actually exit before
    //       returning (no guessing with a fixed sleep).
    pub fn stop(app: &str) -> Result<String> {
        let root = Self::resolve_root(app)?;
        let mut state = Self::load_state_or_fail(&root)?;

        if let Some(unit_app) = Self::systemd_unit_name(&root) {
            // systemctl stop already waits for the unit to stop.
            let _ = crate::systemd::control("stop", &unit_app);
        } else if let Some(pid) = state.pid {
            crate::platform::terminate(pid, false);
            // INFO: Give it up to 5 s to exit cleanly, then force-kill.
            for _ in 0..50 {
                if !Self::process_alive(pid) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if Self::process_alive(pid) {
                crate::platform::terminate(pid, true);
            }
        }

        state.status = "stopped".into();
        state.pid = None;
        common::save_state(&root, &state)?;

        info!("Stopped {}", app);
        Ok("Stopped".into())
    }

    // INFO: stop (waits for exit) -> start.
    pub fn restart(app: &str) -> Result<String> {
        Self::stop(app)?;
        Self::start(app)
    }

    // INFO: Restores the previous deploy by re-pointing `current` at the version
    //       that came before it, then restarting if the app was running.
    pub fn rollback(app: &str) -> Result<String> {
        let root = Self::resolve_root(app)?;

        let previous = crate::versions::previous(&root)?
            .ok_or_else(|| anyhow::anyhow!("No previous version to roll back to"))?;

        // Remember whether the app should keep running after the switch.
        let was_running = common::load_state(&root)?
            .map(|s| s.status == "running")
            .unwrap_or(false);

        if was_running {
            // Stop the current version before swapping so we do not run two at once.
            Self::stop(app)?;
        }

        crate::versions::set_current(&root, &previous)?;
        info!("Rolled back {} to {:?}", app, previous);

        if was_running {
            Self::restart(app)?;
        }

        let name = previous
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(format!("Rolled back to version {}", name))
    }

    // INFO: Shared helper: refuse to act on an app that was never deployed.
    fn load_state_or_fail(dir: &PathBuf) -> Result<AppState> {
        common::load_state(dir)?
            .ok_or_else(|| anyhow::anyhow!("App not found at {:?}. Deploy it first.", dir))
    }
}

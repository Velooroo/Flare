//! Optional systemd integration for `[isolation].type = "systemd"`.
//!
//! INFO: Instead of an ephemeral `systemd-run --scope`, we write a real unit file
//!       under `/etc/systemd/system` and enable it, so the app starts on boot and
//!       can be controlled with `systemctl {start,stop,restart}`.
//!
//! WARN: Unix-only (needs `systemctl` and, for system units, root). On other
//!       platforms the functions return an error / `None`.

#[cfg(unix)]
mod imp {
    use anyhow::{Context, Result};
    use std::collections::HashMap;
    use std::path::Path;
    use std::process::Command;

    // INFO: `user/repo` etc. contain characters invalid in a unit name; map
    //       anything outside `[A-Za-z0-9_-]` to `-`.
    pub fn unit_name(app: &str) -> String {
        let safe: String = app
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        format!("flare-{}.service", safe)
    }

    fn unit_path(app: &str) -> String {
        format!("/etc/systemd/system/{}", unit_name(app))
    }

    pub fn available() -> bool {
        Command::new("systemctl")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    // INFO: Writes/updates the unit for `app` and enables + starts it.
    pub fn install_and_start(
        app: &str,
        dir: &Path,
        run: &common::RunSection,
        env: &HashMap<String, String>,
    ) -> Result<()> {
        if !available() {
            anyhow::bail!("systemctl not found; cannot use systemd isolation");
        }

        let mut env_lines = String::new();
        for (k, v) in env {
            // NOTE: `{:?}` produces a quoted, escaped value that systemd accepts.
            env_lines.push_str(&format!("Environment={:?}\n", format!("{}={}", k, v)));
        }

        let cmd = &run.command;
        let dir_display = dir.display();
        let content = format!(
            "[Unit]\n\
             Description=Flare app {app}\n\
             After=network.target\n\
             \n\
             [Service]\n\
             Type=simple\n\
             WorkingDirectory={dir_display}\n\
             ExecStart=/bin/sh -c {cmd:?}\n\
             Restart=on-failure\n\
             {env_lines}\n\
             [Install]\n\
             WantedBy=multi-user.target\n"
        );

        let path = unit_path(app);
        std::fs::write(&path, content).with_context(|| format!("writing {}", path))?;

        let unit = unit_name(app);
        run_systemctl(&["daemon-reload"])?;
        run_systemctl(&["enable", "--now", &unit])?;
        Ok(())
    }

    // INFO: `start` / `stop` / `restart` for a systemd-managed app.
    pub fn control(action: &str, app: &str) -> Result<()> {
        let unit = unit_name(app);
        run_systemctl(&[action, &unit])
    }

    // INFO: Reads the current MainPID (0 means "not running").
    pub fn main_pid(app: &str) -> Option<u32> {
        let unit = unit_name(app);
        let out = Command::new("systemctl")
            .args(["show", "-p", "MainPID", "--value", &unit])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        text.trim().parse::<u32>().ok().filter(|p| *p != 0)
    }

    fn run_systemctl(args: &[&str]) -> Result<()> {
        let status = Command::new("systemctl").args(args).status()?;
        if !status.success() {
            anyhow::bail!("systemctl {:?} failed", args);
        }
        Ok(())
    }
}

#[cfg(not(unix))]
mod imp {
    use anyhow::Result;
    use std::collections::HashMap;
    use std::path::Path;

    pub fn install_and_start(
        _app: &str,
        _dir: &Path,
        _run: &common::RunSection,
        _env: &HashMap<String, String>,
    ) -> Result<()> {
        anyhow::bail!("systemd isolation is only supported on Unix")
    }

    pub fn control(_action: &str, _app: &str) -> Result<()> {
        anyhow::bail!("systemd isolation is only supported on Unix")
    }

    pub fn main_pid(_app: &str) -> Option<u32> {
        None
    }
}

pub use imp::*;

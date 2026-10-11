//! Database provisioning for `[database]` in a deployed app.
//!
//! INFO: postgres/mysql run as throwaway Docker containers; sqlite is just a file.
//!       The sqlite file lives in the persistent app *root*, not in the version
//!       directory, so it survives rollbacks and redeploys.
//! WARN: Requires the Docker daemon. This contradicts the README's "No Docker
//!       required" claim for anything other than sqlite.

use anyhow::Result;
use common::DatabaseSection;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{info, warn};

pub fn setup(db: &DatabaseSection, dir: &PathBuf, root: &PathBuf) -> Result<()> {
    let db_type = db.r#type.as_str();
    if db_type != "sqlite" && !docker_available() {
        warn!(
            "Docker is required for {} but was not found on this system. \
             Install Docker or use sqlite instead.",
            db_type
        );
        anyhow::bail!(
            "Docker not available. Database type '{}' requires Docker. \
             Use 'sqlite' for a Docker-free setup.",
            db_type
        );
    }

    match db_type {
        "postgres" => setup_postgres(db, dir, root),
        "mysql" => setup_mysql(db, dir, root),
        "sqlite" => setup_sqlite(db, root),
        t => anyhow::bail!("Unknown database: {}", t),
    }
}

fn docker_available() -> bool {
    Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// INFO: Container name is namespaced by the app's directory so two apps never
//       clash even when they use the same `[database].name`.
fn container_name(app: &str, db: &DatabaseSection) -> String {
    let safe: String = app
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let suffix = db.name.as_deref().unwrap_or("db");
    format!("flare-{}-{}", safe, suffix)
}

fn app_id(root: &PathBuf) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into())
}

// INFO: Recreates the container from scratch on every deploy (stop + rm + run).
fn setup_postgres(db: &DatabaseSection, dir: &PathBuf, root: &PathBuf) -> Result<()> {
    let name = db.name.as_deref().unwrap_or("postgres");
    let user = db.user.as_deref().unwrap_or("postgres");
    let pass = db.password.as_deref().unwrap_or("password");
    let requested_port = db.port.unwrap_or(5432);
    let container = container_name(&app_id(root), db);

    stop_container(&container);

    // try to find free port if default is busy
    let actual_port = find_free_port(requested_port)?;

    if actual_port != requested_port {
        info!(
            "Port {} busy, using {} for database",
            requested_port, actual_port
        );
    }

    let status = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &container,
            "-e",
            &format!("POSTGRES_DB={}", name),
            "-e",
            &format!("POSTGRES_USER={}", user),
            "-e",
            &format!("POSTGRES_PASSWORD={}", pass),
            "-p",
            &format!("{}:5432", actual_port),
            "postgres:14-alpine",
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to start postgres");
    }

    // INFO: Poll readiness instead of sleeping a fixed amount.
    wait_ready(
        &container,
        &["pg_isready", "-U", user],
        Duration::from_secs(30),
    )?;
    run_preseed(&container, db, dir, &["psql", "-U", user, "-d", name])?;

    info!("PostgreSQL ready on port {}", actual_port);
    Ok(())
}

// WARN: MySQL still uses a fixed port (no free-port fallback); a busy 3306 fails.
fn setup_mysql(db: &DatabaseSection, dir: &PathBuf, root: &PathBuf) -> Result<()> {
    let name = db.name.as_deref().unwrap_or("mysql");
    let user = db.user.as_deref().unwrap_or("root");
    let pass = db.password.as_deref().unwrap_or("password");
    let port = db.port.unwrap_or(3306);
    let container = container_name(&app_id(root), db);

    stop_container(&container);

    let status = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &container,
            "-e",
            &format!("MYSQL_DATABASE={}", name),
            "-e",
            &format!("MYSQL_USER={}", user),
            "-e",
            &format!("MYSQL_PASSWORD={}", pass),
            "-e",
            &format!("MYSQL_ROOT_PASSWORD={}", pass),
            "-p",
            &format!("{}:3306", port),
            "mysql:8.0",
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to start mysql");
    }

    // INFO: `mysqladmin ping` via MYSQL_PWD (no password on the command line).
    wait_ready_env(
        &container,
        "MYSQL_PWD",
        pass,
        &["mysqladmin", "ping", "-h", "127.0.0.1"],
        Duration::from_secs(45),
    )?;
    run_preseed_env(&container, db, dir, pass, &["mysql", "-u", user, name])?;

    info!("MySQL ready on port {}", port);
    Ok(())
}

// INFO: Creates the database file in the app root and, if `preseed` is set, pipes
//       the SQL file (from the version dir) through the `sqlite3` CLI.
fn setup_sqlite(db: &DatabaseSection, root: &PathBuf) -> Result<()> {
    let name = db.name.as_deref().unwrap_or("app.db");
    let path = root.join(name);

    if !path.exists() {
        std::fs::File::create(&path)?;
    }

    if let Some(preseed) = &db.preseed {
        let sql_path = root.join(preseed);
        if sql_path.exists() {
            Command::new("sqlite3")
                .arg(&path)
                .stdin(std::fs::File::open(&sql_path)?)
                .status()?;
        }
    }

    info!("SQLite ready: {:?}", path);
    Ok(())
}

// NOTE: Best-effort teardown; errors are ignored on purpose.
fn stop_container(name: &str) {
    let _ = Command::new("docker").args(["stop", name]).status();
    let _ = Command::new("docker").args(["rm", name]).status();
}

// INFO: Runs `docker exec <container> <cmd>` repeatedly until it succeeds or the
//       timeout is reached.
fn wait_ready(container: &str, cmd: &[&str], timeout: Duration) -> Result<()> {
    wait_ready_inner(container, cmd, None, timeout)
}

fn wait_ready_env(
    container: &str,
    key: &str,
    val: &str,
    cmd: &[&str],
    timeout: Duration,
) -> Result<()> {
    wait_ready_inner(container, cmd, Some((key, val)), timeout)
}

fn wait_ready_inner(
    container: &str,
    cmd: &[&str],
    env: Option<(&str, &str)>,
    timeout: Duration,
) -> Result<()> {
    let start = Instant::now();
    let mut args = vec!["exec", container];
    args.extend_from_slice(cmd);

    loop {
        let mut c = Command::new("docker");
        c.args(&args);
        if let Some((k, v)) = env {
            c.env(k, v);
        }
        let ok = c.output().map(|o| o.status.success()).unwrap_or(false);
        if ok {
            return Ok(());
        }
        if start.elapsed() > timeout {
            anyhow::bail!("database did not become ready within {:?}", timeout);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

// INFO: Pipes an SQL file into the container via `docker exec -i`.
fn run_preseed(container: &str, db: &DatabaseSection, dir: &PathBuf, cmd: &[&str]) -> Result<()> {
    run_preseed_inner(container, db, dir, None, cmd)
}

fn run_preseed_env(
    container: &str,
    db: &DatabaseSection,
    dir: &PathBuf,
    pass: &str,
    cmd: &[&str],
) -> Result<()> {
    run_preseed_inner(container, db, dir, Some(("MYSQL_PWD", pass)), cmd)
}

fn run_preseed_inner(
    container: &str,
    db: &DatabaseSection,
    dir: &PathBuf,
    env: Option<(&str, &str)>,
    cmd: &[&str],
) -> Result<()> {
    let preseed = match &db.preseed {
        Some(p) => p,
        None => return Ok(()),
    };

    let sql_path = dir.join(preseed);
    if !sql_path.exists() {
        return Ok(());
    }

    let mut args = vec!["exec", "-i", container];
    args.extend(cmd);

    let mut c = Command::new("docker");
    c.args(&args).stdin(std::fs::File::open(&sql_path)?);
    if let Some((k, v)) = env {
        c.env(k, v);
    }
    c.status()?;

    Ok(())
}

// WARN: Scanning from `start` upward can pick a port that another service grabs
//       a moment later (check-then-use race).
fn find_free_port(start: u16) -> Result<u16> {
    for port in start..65535 {
        if port_available(port) {
            return Ok(port);
        }
    }
    anyhow::bail!("No free ports")
}

fn port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

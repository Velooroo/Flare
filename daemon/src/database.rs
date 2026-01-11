use anyhow::Result;
use common::DatabaseSection;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

pub fn setup(db: &DatabaseSection, dir: &PathBuf) -> Result<()> {
    match db.r#type.as_str() {
        "postgres" => postgres(db, dir),
        "mysql" => mysql(db, dir),
        "sqlite" => sqlite(db, dir),
        t => anyhow::bail!("Unknown database: {}", t),
    }
}

fn postgres(db: &DatabaseSection, dir: &PathBuf) -> Result<()> {
    let name = db.name.as_deref().unwrap_or("postgres");
    let user = db.user.as_deref().unwrap_or("postgres");
    let pass = db.password.as_deref().unwrap_or("password");
    let requested_port = db.port.unwrap_or(5432);
    let container = format!("flare-{}-db", name);

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

    std::thread::sleep(std::time::Duration::from_secs(5));
    run_preseed(&container, db, dir, &["psql", "-U", user, "-d", name])?;

    info!("PostgreSQL ready on port {}", actual_port);
    Ok(())
}

fn mysql(db: &DatabaseSection, dir: &PathBuf) -> Result<()> {
    let name = db.name.as_deref().unwrap_or("mysql");
    let user = db.user.as_deref().unwrap_or("root");
    let pass = db.password.as_deref().unwrap_or("password");
    let port = db.port.unwrap_or(3306);
    let container = format!("flare-{}-db", name);

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

    std::thread::sleep(std::time::Duration::from_secs(10));
    run_preseed(
        &container,
        db,
        dir,
        &["mysql", "-u", user, &format!("-p{}", pass), name],
    )?;

    info!("MySQL ready on port {}", port);
    Ok(())
}

fn sqlite(db: &DatabaseSection, dir: &PathBuf) -> Result<()> {
    let name = db.name.as_deref().unwrap_or("app.db");
    let path = dir.join(name);

    if !path.exists() {
        std::fs::File::create(&path)?;
    }

    if let Some(preseed) = &db.preseed {
        let sql_path = dir.join(preseed);
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

fn stop_container(name: &str) {
    let _ = Command::new("docker").args(["stop", name]).status();
    let _ = Command::new("docker").args(["rm", name]).status();
}

fn run_preseed(container: &str, db: &DatabaseSection, dir: &PathBuf, cmd: &[&str]) -> Result<()> {
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

    Command::new("docker")
        .args(&args)
        .stdin(std::fs::File::open(&sql_path)?)
        .status()?;

    Ok(())
}

fn find_free_port(start: u16) -> Result<u16> {
    for port in start..65535 {
        if port_available(port) {
            return Ok(port);
        }
    }
    anyhow::bail!("No free ports")
}

fn port_available(port: u16) -> bool {
    match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(_) => true,
        Err(_) => false,
    }
}

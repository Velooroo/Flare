use anyhow::Result;
use common::{ManageRequest, ManageResponse, recv_json, send_json};
use tokio::net::TcpStream;
use tracing::info;

pub async fn start(host: String, port: u16, app: String) -> Result<()> {
    manage(host, port, app, "start".to_string()).await
}

pub async fn stop(host: String, port: u16, app: String) -> Result<()> {
    manage(host, port, app, "stop".to_string()).await
}

pub async fn restart(host: String, port: u16, app: String) -> Result<()> {
    manage(host, port, app, "restart".to_string()).await
}

pub async fn rollback(host: String, port: u16, app: String) -> Result<()> {
    manage(host, port, app, "rollback".to_string()).await
}

async fn manage(host: String, port: u16, app: String, action: String) -> Result<()> {
    let addr = format!("{}:{}", host, port);

    // TODO: In this moment it's have only on localhost.
    let tcp = TcpStream::connect(&addr).await?;

    let mut socket = crate::tls::connect(tcp, &host).await?;

    let app_normalize = app.replace("/", "_");

    let req = ManageRequest {
        msg_type: "manage".into(),
        app: app_normalize,
        action: action,
    };

    send_json(&mut socket, &req).await?;
    let resp: ManageResponse = recv_json(&mut socket).await?;

    if resp.success {
        info!("SUCCESS: {}", resp.message);
    } else {
        tracing::error!("ERROR: {}", resp.message);
    }

    Ok(())
}

// pub fn rollback(app: &str) -> Result<()> {
//     // Rollback don't have full functionality
//     // TODO: it will be necessary to transfer to the deployment part
//     let dir = common::app_dir(app);
//     let versions = dir.join("versions");

//     let mut entries: Vec<_> = std::fs::read_dir(&versions)?
//         .filter_map(|e| e.ok())
//         .collect();

//     entries.sort_by_key(|e| e.path());

//     let latest = entries
//         .last()
//         .ok_or_else(|| anyhow::anyhow!("No backups found"))?;

//     let current = dir.join("current");

//     if current.exists() {
//         std::fs::remove_file(&current)?;
//     }

//     std::os::unix::fs::symlink(latest.path(), &current)?;

//     info!("Rolled back: {}", app);
//     Ok(())
// }

pub async fn rollback(host: String, port: u16, app: String) -> Result<()> {
    manage(host, port, app, "rollback".to_string()).await
}

use anyhow::Result;
use common::{
    ManageRequest, ManageResponse, app_dir, load_state, recv_json, save_state, send_json,
};
use std::process::Command;
use tokio::net::TcpStream;
use tracing::info;

pub async fn start(app: &str) -> Result<()> {
    manage(app, "start").await
}

pub async fn stop(app: &str) -> Result<()> {
    manage(app, "stop").await
}

pub async fn restart(app: &str) -> Result<()> {
    manage(app, "restart").await
}

async fn manage(app: &str, action: &str) -> Result<()> {
    // TODO: In this moment it's have only on localhost.
    let tcp = TcpStream::connect("127.0.0.1:7530").await?;
    let mut socket = crate::tls::connect(tcp, "127.0.0.1").await?;

    let app_normalize = app.replace("/", "_");

    let req = ManageRequest {
        msg_type: "manage".into(),
        app: app_normalize.to_string(),
        action: action.to_string(),
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

pub async fn rollback(app: &str) -> Result<()> {
    manage(app, "rollback").await
}

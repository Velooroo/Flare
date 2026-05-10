use anyhow::Result;
use common::{ManageRequest, ManageResponse, recv_json, send_json};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};
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

    let tcp = timeout(
        Duration::from_secs(5),
        TcpStream::connect(format!("{}:{}", host, port)),
    )
    .await??;

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
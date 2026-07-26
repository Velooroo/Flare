use anyhow::Result;
use common::{ManageRequest, send_json};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::handlers::HealthStore;

#[derive(Serialize)]
struct HealthStatus {
    pub app: String,
    pub alive: bool,
    pub pid: Option<u32>,
}

pub async fn handle(
    mut socket: TlsStream<TcpStream>,
    msg: serde_json::Value,
    pids: HealthStore,
) -> Result<()> {
    let req: ManageRequest = serde_json::from_value(msg)?;
    let store = pids.read().await;
    let pid = store.get(&req.app).and_then(|p| *p);
    let alive = pid.map(crate::app::AppManager::process_alive).unwrap_or(false);

    send_json(&mut socket, &HealthStatus {
        app: req.app,
        alive,
        pid,
    }).await
}

//! `health` handler — report whether the last known pid for an app is alive.
//!
//! INFO: Requires a valid daemon token, like `deploy` and `manage`.
//! NOTE: Reuses `ManageRequest` as its input shape even though it ignores
//!       `action`. A dedicated `HealthRequest` would be clearer.
//! WARN: The pid table only tracks apps started through this daemon since boot.

use anyhow::Result;
use common::{ManageRequest, send_json};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::auth::TokenStore;
use crate::handlers::Context;

#[derive(Serialize)]
struct HealthStatus {
    pub app: String,
    pub alive: bool,
    pub pid: Option<u32>,
}

pub async fn handle(
    mut socket: TlsStream<TcpStream>,
    msg: serde_json::Value,
    ctx: Context,
) -> Result<()> {
    let req: ManageRequest = serde_json::from_value(msg)?;

    // INFO: Same authentication as the other control messages.
    let store = TokenStore::load();
    let token = req.daemon_token.as_deref().unwrap_or("");
    if !store.verify(token) {
        return send_json(
            &mut socket,
            &serde_json::json!({
                "protocol_version": common::PROTOCOL_VERSION,
                "success": false,
                "message": "Invalid token",
            }),
        )
        .await;
    }

    let store = ctx.health_pids.read().await;
    let pid = store.get(&req.app).and_then(|p| *p);
    // INFO: `kill(pid, 0)` probes liveness without sending a signal.
    let alive = pid
        .map(crate::app::AppManager::process_alive)
        .unwrap_or(false);

    send_json(
        &mut socket,
        &HealthStatus {
            app: req.app,
            alive,
            pid,
        },
    )
    .await
}

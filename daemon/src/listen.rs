use anyhow::Result;
use common::recv_json;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio_rustls::server::TlsStream;
use tracing::{error, info, warn};

use crate::dispatch::manage::HealthPids;
use crate::server::Routes;

/// Maximum number of concurrent client connections.
/// Prevents resource exhaustion from unbounded connection spawning.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;

pub async fn accept_loop(
    listener: TcpListener,
    routes: Routes,
    health_pids: HealthPids,
) -> Result<()> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    loop {
        let (tcp, addr) = listener.accept().await?;
        info!("Connection from {}", addr);

        // Rate-limit: acquire a permit before processing.
        // If all permits are taken, this await blocks until one is released,
        // naturally back-pressuring new connections without dropping them.
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                error!("Semaphore closed: {}", e);
                break;
            }
        };

        let socket = match crate::tls::accept(tcp).await {
            Ok(s) => s,
            Err(e) => {
                error!("TLS handshake failed: {}", e);
                drop(permit);
                continue;
            }
        };

        let routes = routes.clone();
        let health_pids = health_pids.clone();
        tokio::spawn(async move {
            // permit is held for the entire handler lifetime and dropped on completion
            let _permit = permit;
            if let Err(e) = dispatch(socket, routes, health_pids).await {
                error!("Handler error: {}", e);
            }
        });
    }

    Ok(())
}

async fn dispatch(
    mut socket: TlsStream<TcpStream>,
    routes: Routes,
    health_pids: HealthPids,
) -> Result<()> {
    let msg: serde_json::Value = recv_json(&mut socket).await?;
    let msg_type = msg.get("msg_type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "register_token" => {
            let req: common::RegisterTokenRequest = serde_json::from_value(msg)?;
            crate::dispatch::token::handle_register_token(socket, req).await
        }
        "deploy" => {
            let req: common::DeployRequest = serde_json::from_value(msg)?;
            crate::dispatch::deploy::handle_deploy(socket, routes, health_pids, req).await
        }
        "manage" => {
            let req: common::ManageRequest = serde_json::from_value(msg)?;
            crate::dispatch::manage::handle_manage(socket, req).await
        }
        "health" => {
            let req: common::ManageRequest = serde_json::from_value(msg)?;
            crate::dispatch::manage::handle_health(socket, health_pids, req).await
        }
        _ => {
            warn!("Unknown message type: {}", msg_type);
            Ok(())
        }
    }
}

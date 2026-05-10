use anyhow::Result;
use common::recv_json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_rustls::server::TlsStream;
use tracing::{error, info, warn};

pub type Routes = Arc<RwLock<GatewayState>>;

#[derive(Default)]
pub struct GatewayState {
    pub static_sites: HashMap<String, String>,
    pub proxy_routes: HashMap<String, u16>,
}

pub type HealthPids = Arc<RwLock<HashMap<String, Option<u32>>>>;

pub async fn run(port: u16) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Listening on port {}", port);

    let routes: Routes = Arc::new(RwLock::new(GatewayState::default()));
    let health_pids: HealthPids = Arc::new(RwLock::new(HashMap::new()));

    // start gateway
    let routes_clone = routes.clone();
    let health_pids_clone = health_pids.clone();

    tokio::spawn(async move {
        if let Err(e) = crate::gateway::run(routes_clone).await {
            error!("Gateway error: {}", e);
        }
    });

    // start health server
    tokio::spawn(async move {
        if let Err(e) = crate::health_server::run(health_pids_clone).await {
            error!("Health server error: {}", e);
        }
    });

    // start discovery
    tokio::spawn(async move {
        if let Err(e) = crate::discovery::run(7001).await {
            error!("Discovery error: {}", e);
        }
    });

    loop {
        let (tcp, addr) = listener.accept().await?;
        info!("Connection from {}", addr);

        let socket = match crate::tls::accept(tcp).await {
            Ok(s) => s,
            Err(e) => {
                error!("TLS handshake failed: {}", e);
                continue;
            }
        };

        let routes = routes.clone();
        let health_pids = health_pids.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(socket, routes, health_pids).await {
                error!("Handler error: {}", e);
            }
        });
    }
}

async fn handle(
    mut socket: TlsStream<TcpStream>,
    routes: Routes,
    health_pids: HealthPids,
) -> Result<()> {
    let msg: serde_json::Value = recv_json(&mut socket).await?;

    tracing::info!(
        "Raw message: {}",
        serde_json::to_string_pretty(&msg).unwrap()
    );

    let msg_type = msg.get("msg_type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "register_token" => {
            let req: common::RegisterTokenRequest = serde_json::from_value(msg)?;
            crate::handlers::token::handle_register_token(socket, req).await
        }
        "deploy" => {
            let req: common::DeployRequest = serde_json::from_value(msg)?;
            crate::handlers::deploy_handler::handle_deploy(socket, routes, health_pids, req).await
        }
        "manage" => {
            let req: common::ManageRequest = serde_json::from_value(msg)?;
            crate::handlers::manage::handle_manage(socket, req).await
        }
        _ => {
            warn!("Unknown message type: {}", msg_type);
            Ok(())
        }
    }
}

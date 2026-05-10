use anyhow::Result;
use common::{DeployRequest, DeployResponse, send_json};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::{info, warn};

use crate::deploy;
use crate::server::{HealthPids, Routes};
use crate::token_store;

pub async fn handle_deploy(
    mut socket: TlsStream<TcpStream>,
    routes: Routes,
    health_pids: HealthPids,
    req: DeployRequest,
) -> Result<()> {
    // verify daemon token
    let store = token_store::load_tokens();
    let token = req.daemon_token.as_deref().unwrap_or("");
    let valid = store
        .tokens
        .iter()
        .any(|hash| common::verify_token(token, hash));

    if !valid {
        warn!("Invalid token for deploy");
        let response = DeployResponse {
            success: false,
            message: "Invalid token".into(),
            app_dir: None,
        };
        send_json(&mut socket, &response).await?;
        return Ok(());
    }

    info!("Deploy: {}", req.repo);

    let response = match deploy::run(&req, routes, health_pids).await {
        Ok(dir) => DeployResponse {
            success: true,
            message: format!("Deployed to {}", dir.display()),
            app_dir: Some(dir.to_string_lossy().into()),
        },
        Err(e) => DeployResponse {
            success: false,
            message: e.to_string(),
            app_dir: None,
        },
    };

    send_json(&mut socket, &response).await
}

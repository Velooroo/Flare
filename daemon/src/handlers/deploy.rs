use anyhow::Result;
use common::{DeployRequest, DeployResponse, send_json};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::info;

use crate::auth::TokenStore;
use crate::gateway::Route;
use crate::handlers::Context;
use crate::pipeline;

pub async fn handle(
    mut socket: TlsStream<TcpStream>,
    msg: serde_json::Value,
    ctx: Context,
) -> Result<()> {
    let req: DeployRequest = serde_json::from_value(msg)?;

    let store = TokenStore::load();
    let token = req.daemon_token.as_deref().unwrap_or("");
    if !store.verify(token) {
        send_json(&mut socket, &DeployResponse {
            protocol_version: common::PROTOCOL_VERSION.to_string(),
            success: false,
            message: "Invalid token".into(),
            app_dir: None,
        }).await?;
        return Ok(());
    }

    info!("Deploy: {}", req.repo);

    let outcome = pipeline::run(&req).await;

    match &outcome {
        pipeline::Outcome::Ok { static_site, name, pid, port, .. } => {
            if let Some((domain, root)) = static_site {
                ctx.routes.write().await.insert(domain.clone(), Route::Static { root: root.clone() });
            } else if let Some(p) = port {
                ctx.routes.write().await.insert(name.clone(), Route::App { port: *p, pid: *pid });
            }
            ctx.health_pids.write().await.insert(name.clone(), *pid);
        }
        pipeline::Outcome::Err { .. } => {}
    }

    let response = match outcome {
        pipeline::Outcome::Ok { dir, message, .. } => DeployResponse {
            protocol_version: common::PROTOCOL_VERSION.to_string(),
            success: true,
            message,
            app_dir: Some(dir.to_string_lossy().into()),
        },
        pipeline::Outcome::Err { message } => DeployResponse {
            protocol_version: common::PROTOCOL_VERSION.to_string(),
            success: false,
            message,
            app_dir: None,
        },
    };

    send_json(&mut socket, &response).await
}

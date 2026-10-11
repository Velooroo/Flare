//! `manage` handler — start/stop/restart/rollback an app.
//!
//! INFO: The request must carry a valid daemon token (the same token used for
//!       deploy). This closes the previous hole where anyone reachable on the
//!       control port could control apps.
//!
//! WARN: AppManager runs blocking code (spawn/kill/sleep) directly inside this
//!       async handler, which stalls a Tokio worker thread.

use anyhow::Result;
use common::{ManageRequest, ManageResponse, send_json};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::app::AppManager;
use crate::auth::TokenStore;
use crate::gateway::Route;
use crate::handlers::Context;

pub async fn handle(
    mut socket: TlsStream<TcpStream>,
    msg: serde_json::Value,
    ctx: Context,
) -> Result<()> {
    let req: ManageRequest = serde_json::from_value(msg)?;

    // INFO: Authenticate before doing anything else.
    let store = TokenStore::load();
    let token = req.daemon_token.as_deref().unwrap_or("");
    if !store.verify(token) {
        let response = ManageResponse {
            protocol_version: common::PROTOCOL_VERSION.to_string(),
            success: false,
            message: "Invalid token".into(),
        };
        return send_json(&mut socket, &response).await;
    }

    // INFO: AppManager is blocking (spawn/kill/wait), so run it on the blocking
    //       pool instead of stalling the async worker.
    let app = req.app.clone();
    let action = req.action.clone();
    let result = match tokio::task::spawn_blocking(move || match action.as_str() {
        "start" => AppManager::start(&app),
        "stop" => AppManager::stop(&app),
        "restart" => AppManager::restart(&app),
        "rollback" => AppManager::rollback(&app),
        _ => Err(anyhow::anyhow!("Unknown action: {}", action)),
    })
    .await
    {
        Ok(r) => r,
        Err(e) => Err(anyhow::anyhow!("manage task failed: {}", e)),
    };

    // INFO: On success, re-publish the gateway route so a rollback/restart is
    //       reflected immediately (new version dir, new pid).
    if result.is_ok() {
        refresh_route(&req.app, &ctx).await;
    }

    let response = match result {
        Ok(msg) => ManageResponse {
            protocol_version: common::PROTOCOL_VERSION.to_string(),
            success: true,
            message: msg,
        },
        Err(e) => ManageResponse {
            protocol_version: common::PROTOCOL_VERSION.to_string(),
            success: false,
            message: e.to_string(),
        },
    };

    send_json(&mut socket, &response).await
}

// INFO: Re-reads the app's config/state and updates the shared route table.
async fn refresh_route(app: &str, ctx: &Context) {
    let root = match AppManager::resolve_root(app) {
        Ok(r) => r,
        Err(_) => return,
    };
    let live = match crate::versions::current(&root) {
        Some(v) => v,
        None => return,
    };
    let config = match common::load_app_config(&live) {
        Ok(c) => c,
        Err(_) => return,
    };

    let pid = common::load_state(&root).ok().flatten().and_then(|s| s.pid);
    let name = config.app.name.clone();

    if let Some(web) = &config.web {
        // Static site: point the domain at the (possibly new) version root.
        let site_root = live.join(web.root.as_deref().unwrap_or("."));
        ctx.routes.write().await.insert(
            web.domain.clone(),
            Route::Static {
                root: site_root.to_string_lossy().into(),
            },
        );
    } else if let Some(run) = &config.run {
        if let Some(port) = run.port {
            if pid.is_some() {
                ctx.routes
                    .write()
                    .await
                    .insert(name.clone(), Route::App { port, pid });
            } else {
                // Stopped: drop the route so the gateway reports "no route".
                ctx.routes.write().await.remove(&name);
            }
        }
    }

    ctx.health_pids.write().await.insert(name, pid);
}

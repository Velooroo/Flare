use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::error;

use crate::gateway;
use crate::handlers::Router;

pub async fn run(port: u16) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    let routes: gateway::Routes = Arc::new(RwLock::new(HashMap::new()));
    let router = Arc::new(Router::new(routes.clone()));

    let mut joins = JoinSet::new();

    let gw_routes = routes.clone();
    joins.spawn(async move {
        gateway::run(gw_routes).await;
    });

    joins.spawn(async move {
        if let Err(e) = crate::discovery::run(7001, port).await {
            error!("Discovery error: {}", e);
        }
    });

    let accept_result = crate::listen::accept_loop(listener, router).await;

    joins.shutdown().await;

    accept_result
}

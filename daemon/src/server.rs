//! Daemon bootstrap: bind the control port and fan out to the subsystems.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::error;

use crate::control::DaemonControl;
use crate::gateway;

pub async fn run(port: u16) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    // INFO: The route table is shared between the gateway (reader) and the
    //       control handlers (writer). It maps a Host name to a static dir or an
    //       app port.
    let routes: gateway::Routes = Arc::new(RwLock::new(HashMap::new()));

    // INFO: Build the control dispatcher once. `common::rpc::serve_on` wraps every
    //       connection in Flare's TLS and hands it to monolith.
    let mut dispatcher = common::monolith::Dispatcher::new();
    common::control::register(&mut dispatcher, DaemonControl::new(routes.clone()));
    let dispatcher = Arc::new(dispatcher);

    let mut joins = JoinSet::new();

    // INFO: Gateway binds port 80 by default ($FLARE_GATEWAY_PORT). It returns an
    //       error instead of panicking if the bind fails.
    let gw_routes = routes.clone();
    joins.spawn(async move {
        if let Err(e) = gateway::run(gw_routes).await {
            error!("Gateway error: {}", e);
        }
    });

    // INFO: Discovery UDP port from $FLARE_DISCOVERY_PORT (default 7001).
    let discovery_port: u16 = std::env::var("FLARE_DISCOVERY_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(7001);
    joins.spawn(async move {
        if let Err(e) = crate::discovery::run(discovery_port, port).await {
            error!("Discovery error: {}", e);
        }
    });

    // INFO: Runs until the TLS accept loop returns (which is effectively never).
    let accept_result = common::rpc::serve_on(dispatcher, listener).await;

    joins.shutdown().await;

    accept_result
}

//! One module per CLI command group. Each module talks to the daemon directly
//! (or writes local config) — there is no shared state between them.

pub mod apps;
pub mod auth;
pub mod deploy;
pub mod devices;
pub mod discovery;

use anyhow::Result;

/// Connects to a daemon over Flare's TLS and returns a typed control client.
///
/// INFO: The certificate is verified by `common::security` (pinned on the LAN),
///       and the same monolith service definition backs both sides.
pub async fn connect(host: &str, port: u16) -> Result<common::control::Client> {
    let endpoint = common::rpc::connect_endpoint(host, port).await?;
    Ok(common::control::Client::from_endpoint(endpoint))
}

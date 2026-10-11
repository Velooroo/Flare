//! Request router for the control port.
//!
//! A single JSON `Value` is parsed just enough to read `msg_type` and
//! `protocol_version`, then dispatched to the matching handler.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_rustls::server::TlsStream;

use crate::gateway::Routes;

pub mod deploy;
pub mod health;
pub mod manage;
pub mod token;

// INFO: Shared map of app name -> last known pid, used by the health handler.
pub type HealthStore = Arc<RwLock<HashMap<String, Option<u32>>>>;

pub struct Context {
    pub routes: Routes,
    pub health_pids: HealthStore,
    /// One-time code required for the very first token enrollment, if the daemon
    /// started with no tokens.
    pub pairing_code: Option<String>,
}

pub struct Router {
    pub routes: Routes,
    health_pids: HealthStore,
    pairing_code: Option<String>,
}

impl Router {
    pub fn new(routes: Routes) -> Self {
        // INFO: Only mint a pairing code when nobody is paired yet. Once a token
        //       exists, enrollment requires that token instead.
        let pairing_code = if crate::auth::TokenStore::load().tokens.is_empty() {
            let code = common::generate_token()[..6].to_uppercase();
            tracing::info!("First-time pairing code: {}", code);
            Some(code)
        } else {
            None
        };

        Self {
            routes,
            health_pids: Arc::new(RwLock::new(HashMap::new())),
            pairing_code,
        }
    }

    pub fn context(&self) -> Context {
        Context {
            routes: self.routes.clone(),
            health_pids: self.health_pids.clone(),
            pairing_code: self.pairing_code.clone(),
        }
    }

    pub async fn dispatch(
        &self,
        mut socket: TlsStream<TcpStream>,
        msg: serde_json::Value,
    ) -> Result<()> {
        let msg_type = msg.get("msg_type").and_then(|v| v.as_str()).unwrap_or("");

        // INFO: Refuse peers speaking a different protocol version.
        let version = msg
            .get("protocol_version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if version != common::PROTOCOL_VERSION {
            let resp = serde_json::json!({
                "protocol_version": common::PROTOCOL_VERSION,
                "success": false,
                "message": format!("Protocol version mismatch: got '{}', expected '{}'", version, common::PROTOCOL_VERSION),
            });
            common::send_json(&mut socket, &resp).await?;
            return Ok(());
        }

        // INFO: `deploy` and `manage` verify the daemon token. `health` verifies
        //       it too. `register_token` is the trust-on-first-use bootstrap.
        match msg_type {
            "register_token" => token::handle(socket, msg, self.context()).await,
            "deploy" => deploy::handle(socket, msg, self.context()).await,
            "manage" => manage::handle(socket, msg, self.context()).await,
            "health" => health::handle(socket, msg, self.context()).await,
            _ => {
                // WARN: Unknown message types get no reply, so the client blocks
                //       until its own timeout. Sending an error would be friendlier.
                tracing::warn!("Unknown message type: {}", msg_type);
                Ok(())
            }
        }
    }
}

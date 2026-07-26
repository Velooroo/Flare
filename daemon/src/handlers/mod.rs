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

pub type HealthStore = Arc<RwLock<HashMap<String, Option<u32>>>>;

pub struct Context {
    pub routes: Routes,
    pub health_pids: HealthStore,
}

pub struct Router {
    pub routes: Routes,
    health_pids: HealthStore,
}

impl Router {
    pub fn new(routes: Routes) -> Self {
        Self {
            routes,
            health_pids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn context(&self) -> Context {
        Context {
            routes: self.routes.clone(),
            health_pids: self.health_pids.clone(),
        }
    }

    pub async fn dispatch(
        &self,
        mut socket: TlsStream<TcpStream>,
        msg: serde_json::Value,
    ) -> Result<()> {
        let msg_type = msg.get("msg_type").and_then(|v| v.as_str()).unwrap_or("");

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

        match msg_type {
            "register_token" => token::handle(socket, msg).await,
            "deploy" => deploy::handle(socket, msg, self.context()).await,
            "manage" => manage::handle(socket, msg).await,
            "health" => health::handle(socket, msg, self.health_pids.clone()).await,
            _ => {
                tracing::warn!("Unknown message type: {}", msg_type);
                Ok(())
            }
        }
    }
}

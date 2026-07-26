use anyhow::Result;
use common::{ManageRequest, ManageResponse, send_json};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::app::AppManager;

pub async fn handle(mut socket: TlsStream<TcpStream>, msg: serde_json::Value) -> Result<()> {
    let req: ManageRequest = serde_json::from_value(msg)?;

    let result = match req.action.as_str() {
        "start" => AppManager::start(&req.app),
        "stop" => AppManager::stop(&req.app),
        "restart" => AppManager::restart(&req.app),
        "rollback" => AppManager::rollback(&req.app),
        _ => Err(anyhow::anyhow!("Unknown action: {}", req.action)),
    };

    let response = match result {
        Ok(msg) => ManageResponse { protocol_version: common::PROTOCOL_VERSION.to_string(), success: true, message: msg },
        Err(e) => ManageResponse { protocol_version: common::PROTOCOL_VERSION.to_string(), success: false, message: e.to_string() },
    };

    send_json(&mut socket, &response).await
}

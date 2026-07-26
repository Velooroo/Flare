use anyhow::Result;
use common::{RegisterTokenRequest, RegisterTokenResponse, send_json};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::auth::TokenStore;

pub async fn handle(mut socket: TlsStream<TcpStream>, msg: serde_json::Value) -> Result<()> {
    let req: RegisterTokenRequest = serde_json::from_value(msg)?;
    let mut store = TokenStore::load();
    store.register(req.token_hash)?;

    send_json(&mut socket, &RegisterTokenResponse {
        protocol_version: common::PROTOCOL_VERSION.to_string(),
        success: true,
    }).await
}

use anyhow::Result;
use common::{RegisterTokenRequest, RegisterTokenResponse, send_json};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::info;

use crate::token_store;

pub async fn handle_register_token(
    mut socket: TlsStream<TcpStream>,
    req: RegisterTokenRequest,
) -> Result<()> {
    let mut store = token_store::load_tokens();
    store.tokens.push(req.token_hash);
    token_store::save_tokens(&store)?;
    info!("Registered new token");

    let resp = RegisterTokenResponse { success: true };
    send_json(&mut socket, &resp).await?;

    Ok(())
}

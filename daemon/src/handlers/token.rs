//! `register_token` handler — enroll a new CLI token hash.
//!
//! INFO: On a fresh daemon (no tokens) the request must carry the one-time
//!       pairing code printed at startup (`pairing_code`). Once at least one
//!       token exists, the request must instead prove an existing token
//!       (`existing_token`). Either way, a random host on the network cannot
//!       enroll itself silently.

use anyhow::Result;
use common::{RegisterTokenRequest, RegisterTokenResponse, send_json};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::auth::TokenStore;
use crate::handlers::Context;

pub async fn handle(
    mut socket: TlsStream<TcpStream>,
    msg: serde_json::Value,
    ctx: Context,
) -> Result<()> {
    let req: RegisterTokenRequest = serde_json::from_value(msg)?;
    let mut store = TokenStore::load();

    let authorized = if store.tokens.is_empty() {
        // First-time pairing: compare against the startup code.
        match ctx.pairing_code.as_deref() {
            Some(expected) => req.pairing_code.as_deref() == Some(expected),
            // No code minted (e.g. tokens were cleared at runtime) -> allow.
            None => true,
        }
    } else {
        // Already paired: the caller must prove an existing token.
        store.verify(req.existing_token.as_deref().unwrap_or(""))
    };

    if !authorized {
        tracing::warn!("Rejected token enrollment (bad pairing code / existing token)");
        return send_json(
            &mut socket,
            &RegisterTokenResponse {
                protocol_version: common::PROTOCOL_VERSION.to_string(),
                success: false,
            },
        )
        .await;
    }

    store.register(req.token_hash)?;

    send_json(
        &mut socket,
        &RegisterTokenResponse {
            protocol_version: common::PROTOCOL_VERSION.to_string(),
            success: true,
        },
    )
    .await
}

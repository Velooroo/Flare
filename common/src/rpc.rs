//! Monolith over Flare's secure transport — **TLS only**.
//!
//! The rest of Flare never calls `monolith::Endpoint::connect` (plain TCP).
//! Instead it goes through these helpers, which wrap the stream in TLS first and
//! reuse [`crate::security`] for identity + certificate pinning.
//!
//! * [`connect_endpoint`] — client side: TCP → TLS → `monolith::Endpoint`.
//! * [`serve`] — server side: bind → TLS → `Dispatcher::handle_connection`.
//!
//! Because monolith is transport-agnostic, it needs no TLS code of its own: the
//! verified stream is simply handed to `Endpoint::from_stream` /
//! `Dispatcher::handle_connection`.

use anyhow::Result;
use monolith::{Dispatcher, Endpoint};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// Connects to a monolith server over TLS.
///
/// Local/private hosts are certificate-pinned (see [`crate::security`]); public
/// hosts are verified against the WebPKI roots.
pub async fn connect_endpoint(host: &str, port: u16) -> Result<Endpoint> {
    let tcp = TcpStream::connect((host, port)).await?;
    let tls = crate::security::connect_tls(tcp, host, port).await?;
    Ok(Endpoint::from_stream(tls))
}

/// Serves a monolith dispatcher over TLS until the process stops.
///
/// Each accepted connection gets its own task; the dispatcher is shared through
/// an `Arc`, which `handle_connection(&self)` supports natively.
pub async fn serve(
    dispatcher: Arc<Dispatcher>,
    bind: impl tokio::net::ToSocketAddrs,
) -> Result<()> {
    let listener = TcpListener::bind(bind).await?;
    serve_on(dispatcher, listener).await
}

/// Same as [`serve`], but on an already-bound listener.
///
/// Useful when the caller needs the real port (e.g. `bind("…:0")`) before the
/// accept loop starts, and for tests.
pub async fn serve_on(dispatcher: Arc<Dispatcher>, listener: TcpListener) -> Result<()> {
    let acceptor = crate::security::server_acceptor()?;
    let local = listener.local_addr()?;
    tracing::info!("monolith (TLS) listening on {local}");

    loop {
        let (tcp, addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let dispatcher = dispatcher.clone();

        tokio::spawn(async move {
            match crate::security::accept(&acceptor, tcp).await {
                Ok(tls) => {
                    if let Err(e) = dispatcher.handle_connection(tls).await {
                        tracing::warn!("monolith connection from {addr} failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("TLS handshake from {addr} failed: {e}"),
            }
        });
    }
}

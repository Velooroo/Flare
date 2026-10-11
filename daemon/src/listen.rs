//! Accept loop for the TLS control port.
//!
//! Every accepted TCP connection is wrapped in TLS, then handed to the Router as
//! a single request/response exchange (the daemon reads one framed JSON message
//! per connection).

use anyhow::Result;
use common::recv_json;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;
use tracing::{error, info};

use crate::handlers::Router;

// INFO: Caps how many connections are processed at once. This protects memory,
//       not request rate.
// WARN: There is no per-IP throttling, and a permit is held across the TLS
//       handshake, so a slow handshake can occupy a slot.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;

pub async fn accept_loop(
    listener: TcpListener,
    router: Arc<Router>,
    acceptor: TlsAcceptor,
) -> Result<()> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    loop {
        let (tcp, addr) = listener.accept().await?;
        info!("Connection from {}", addr);

        let permit = match semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                error!("Semaphore closed: {}", e);
                break;
            }
        };

        // INFO: The acceptor is built once at startup and reused here.
        let socket = match crate::tls::accept(&acceptor, tcp).await {
            Ok(s) => s,
            Err(e) => {
                error!("TLS handshake from {} failed: {}", addr, e);
                drop(permit);
                continue;
            }
        };

        let router = router.clone();
        tokio::spawn(async move {
            let _permit = permit; // released when the task ends
            if let Err(e) = handle(socket, router).await {
                error!("Handler error: {}", e);
            }
        });
    }

    Ok(())
}

// INFO: Exactly one request per connection; the socket is dropped afterwards.
async fn handle(mut socket: TlsStream<TcpStream>, router: Arc<Router>) -> Result<()> {
    let msg: serde_json::Value = recv_json(&mut socket).await?;
    router.dispatch(socket, msg).await
}

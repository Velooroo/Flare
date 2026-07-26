use anyhow::Result;
use common::recv_json;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio_rustls::server::TlsStream;
use tracing::{error, info};

use crate::handlers::Router;

const MAX_CONCURRENT_CONNECTIONS: usize = 64;

pub async fn accept_loop(listener: TcpListener, router: Arc<Router>) -> Result<()> {
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

        let socket = match crate::tls::accept(tcp).await {
            Ok(s) => s,
            Err(e) => {
                error!("TLS handshake from {} failed: {}", addr, e);
                drop(permit);
                continue;
            }
        };

        let router = router.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = handle(socket, router).await {
                error!("Handler error: {}", e);
            }
        });
    }

    Ok(())
}

async fn handle(mut socket: TlsStream<TcpStream>, router: Arc<Router>) -> Result<()> {
    let msg: serde_json::Value = recv_json(&mut socket).await?;
    router.dispatch(socket, msg).await
}

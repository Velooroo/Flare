//! flared — the daemon that runs on the target device.
//!
//! It starts three listeners:
//!   * TLS control server on TCP 7530  (deploy / manage / token / health)
//!   * UDP discovery responder on 7001  (replies to `flare discover`)
//!   * HTTP gateway on port 80          (static sites + reverse proxy)
//!
//! Comment legend: INFO / TODO / WARN / ERROR / FIXME / HACK / NOTE.

mod app;
mod auth;
mod discovery;
mod dispatch;
mod gateway;
mod handlers;
mod listen;
mod pipeline;
mod platform;
mod server;
mod systemd;
mod tls;
mod versions;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Flared starting...");

    // INFO: Control port from $FLARE_PORT (default 7530).
    let port: u16 = std::env::var("FLARE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(7530);

    // ERROR: `run` only returns when the accept loop dies, so any error below
    //        kills the whole daemon. In release builds a panic aborts the process
    //        outright (Cargo.toml sets panic = "abort").
    if let Err(e) = server::run(port).await {
        tracing::error!("Daemon crashed: {}", e);
    }
}

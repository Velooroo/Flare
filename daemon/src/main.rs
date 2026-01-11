mod database;
mod deploy;
mod discovery;
mod gateway;
mod hooks;
mod server;
mod tls;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Flared starting...");

    if let Err(e) = server::run(7530).await {
        tracing::error!("Daemon crashed: {}", e);
    }
}

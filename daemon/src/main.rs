mod database;
mod deploy;
mod discovery;
mod env_loader;
mod gateway;
mod health_server;
mod hooks;
mod server;
mod tls;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Flared starting...");

    if let Err(e) = server::run(7530).await {
        tracing::error!("Daemon crashed: {}", e);
    }
}

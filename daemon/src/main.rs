mod app_config;
mod database;
mod deploy;
mod deploy_config;
mod discovery;
mod env_loader;
mod gateway;
mod handlers;
mod health_server;
mod hooks;
mod server;
mod tls;
mod token_store;

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

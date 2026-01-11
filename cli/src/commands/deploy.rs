use anyhow::Result;
use common::{DeployRequest, DeployResponse, recv_json, send_json};
use tokio::net::TcpStream;
use tracing::{error, info};

pub async fn run(
    host: String,
    port: u16,
    repo: String,
    github: bool,
    forge: String,
    token: Option<String>,
    user: Option<String>,
) -> Result<()> {
    // load saved auth if not provided
    let auth = crate::commands::auth::load()?;

    let final_user = user
        .or(auth.user)
        .or_else(|| std::env::var("FLARE_USER").ok());

    let final_token = token
        .or(auth.password)
        .or_else(|| std::env::var("FLARE_PASS").ok());

    let final_forge = if github {
        "github".into()
    } else if forge != "http://localhost:8080" {
        forge
    } else {
        auth.forge.unwrap_or(forge)
    };

    let tcp = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let mut socket = crate::tls::connect(tcp, &host).await?;

    info!("Connected to {}:{}", host, port);

    tracing::info!("Sending auth_user: {:?}", final_user);
    tracing::info!("Sending auth_password: {:?}", final_token);

    let req = DeployRequest {
        msg_type: "deploy".into(),
        repo,
        forge: final_forge,
        auth_user: final_user,
        auth_password: final_token,
        daemon_token: None,
    };

    send_json(&mut socket, &req).await?;
    let resp: DeployResponse = recv_json(&mut socket).await?;

    if resp.success {
        info!("SUCCESS: {}", resp.message);
    } else {
        error!("ERROR: {}", resp.message);
    }

    Ok(())
}

pub async fn run_to_device(
    device_id: &str,
    repo: String,
    github: bool,
    forge: String,
    token: Option<String>,
    user: Option<String>,
) -> Result<()> {
    let device = common::get_device(device_id)?;
    let auth = crate::commands::auth::load().unwrap_or_default();

    let tcp = TcpStream::connect(format!("{}:{}", device.host, device.port)).await?;
    let mut socket = crate::tls::connect(tcp, &device.host).await?;

    let req = DeployRequest {
        msg_type: "deploy".into(),
        repo,
        forge: if github { "github".into() } else { forge },
        auth_user: user.or(auth.user),
        auth_password: token.or(auth.password),
        daemon_token: device.token.clone(),
    };

    send_json(&mut socket, &req).await?;
    let resp: DeployResponse = recv_json(&mut socket).await?;

    if resp.success {
        info!("SUCCESS: {}", resp.message);
    } else {
        error!("ERROR: {}", resp.message);
    }

    Ok(())
}

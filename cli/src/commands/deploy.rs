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
    deploy_to(host, port, repo, github, forge, token, user, None).await
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
    deploy_to(
        device.host,
        device.port,
        repo,
        github,
        forge,
        token,
        user,
        device.token,
    )
    .await
}

async fn deploy_to(
    host: String,
    port: u16,
    repo: String,
    github: bool,
    forge: String,
    token: Option<String>,
    user: Option<String>,
    daemon_token: Option<String>,
) -> Result<()> {
    let auth = crate::commands::auth::load().unwrap_or_default();
    let tcp = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let mut socket = crate::tls::connect(tcp, &host).await?;

    let req = DeployRequest {
        msg_type: "deploy".into(),
        repo,
        forge: if github { "github".into() } else { forge },
        auth_user: user.or(auth.user),
        auth_password: token.or(auth.password),
        daemon_token,
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

//! `flare deploy` — ask a daemon to fetch, build and start a repo.
//!
//! The CLI does not send the code. It only tells the daemon which repo/branch to
//! pull; the daemon downloads the archive itself (see daemon pipeline::download).

use anyhow::Result;
use common::{DeployRequest, DeployResponse, recv_json, send_json};
use tokio::net::TcpStream;
use tracing::info;

// INFO: Deploy to an explicit --host (no saved device, no daemon token).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    host: String,
    port: u16,
    repo: String,
    github: bool,
    forge: String,
    branch: String,
    token: Option<String>,
    user: Option<String>,
) -> Result<()> {
    deploy_to(host, port, repo, github, forge, branch, token, user, None).await
}

// INFO: Deploy to a device saved by `flare sync`, reusing its stored token.
#[allow(clippy::too_many_arguments)]
pub async fn run_to_device(
    device_id: &str,
    repo: String,
    github: bool,
    forge: String,
    branch: String,
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
        branch,
        token,
        user,
        device.token,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn deploy_to(
    host: String,
    port: u16,
    repo: String,
    github: bool,
    forge: String,
    branch: String,
    token: Option<String>,
    user: Option<String>,
    daemon_token: Option<String>,
) -> Result<()> {
    // NOTE: Missing auth file is fine — flags may already carry the credentials.
    let auth = crate::commands::auth::load().unwrap_or_default();
    let tcp = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let mut socket = crate::tls::connect(tcp, &host).await?;

    // INFO: `--github` forces the GitHub forge. Explicit --token/--user win over
    //       the saved auth.toml values. An empty branch means "forge default".
    let req = DeployRequest {
        protocol_version: common::PROTOCOL_VERSION.to_string(),
        msg_type: "deploy".into(),
        repo,
        forge: if github { "github".into() } else { forge },
        branch: if branch.is_empty() {
            None
        } else {
            Some(branch)
        },
        auth_user: user.or(auth.user),
        auth_password: token.or(auth.password),
        daemon_token,
    };

    send_json(&mut socket, &req).await?;
    let resp: DeployResponse = recv_json(&mut socket).await?;

    // ERROR: Surface a failed deploy as a non-zero exit code (main logs it).
    if resp.success {
        info!("SUCCESS: {}", resp.message);
        Ok(())
    } else {
        anyhow::bail!("{}", resp.message);
    }
}

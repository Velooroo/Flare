//! `flare deploy` — ask a daemon to fetch, build and start a repo.
//!
//! The CLI does not send the code. It only tells the daemon which repo/branch to
//! pull; the daemon downloads the archive itself (see daemon pipeline::download).
//! The request travels over Flare's TLS via the generated `control::Client`.

use anyhow::Result;
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
    let client = crate::commands::connect(&host, port).await?;

    // INFO: `--github` forces the GitHub forge. Explicit --token/--user win over
    //       the saved auth.toml values. An empty branch means "forge default".
    let reply = client
        .deploy(
            daemon_token,
            repo,
            if github { "github".into() } else { forge },
            if branch.is_empty() {
                None
            } else {
                Some(branch)
            },
            user.or(auth.user),
            token.or(auth.password),
        )
        .await?;

    // ERROR: Surface a failed deploy as a non-zero exit code (main logs it).
    if reply.success {
        info!("SUCCESS: {}", reply.message);
        Ok(())
    } else {
        anyhow::bail!("{}", reply.message);
    }
}

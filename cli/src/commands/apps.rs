//! `flare start|stop|restart|rollback` — remote app lifecycle.
//!
//! All four send a `manage` request over Flare's TLS. The daemon requires the
//! daemon token, so the CLI resolves one either from `--device <id|name>` or from
//! a saved device whose host:port matches. Without a token the daemon answers
//! "Invalid token".

use anyhow::Result;
use tokio::time::{Duration, timeout};
use tracing::info;

pub async fn start(host: String, port: u16, app: String, device: Option<String>) -> Result<()> {
    manage(host, port, app, device, "start".to_string()).await
}

pub async fn stop(host: String, port: u16, app: String, device: Option<String>) -> Result<()> {
    manage(host, port, app, device, "stop".to_string()).await
}

pub async fn restart(host: String, port: u16, app: String, device: Option<String>) -> Result<()> {
    manage(host, port, app, device, "restart".to_string()).await
}

pub async fn rollback(host: String, port: u16, app: String, device: Option<String>) -> Result<()> {
    manage(host, port, app, device, "rollback".to_string()).await
}

// INFO: Resolves the effective target and its token.
//   * `--device` wins: host/port/token all come from the saved device.
//   * otherwise host/port are used as given and the token is looked up from a
//     saved device with the same host:port (may be `None`).
fn resolve_target(
    host: String,
    port: u16,
    device: Option<String>,
) -> Result<(String, u16, Option<String>)> {
    if let Some(dev) = device {
        let d = common::get_device(&dev)?;
        return Ok((d.host, d.port, d.token));
    }
    let token = common::find_device_by_addr(&host, port).and_then(|d| d.token);
    Ok((host, port, token))
}

async fn manage(
    host: String,
    port: u16,
    app: String,
    device: Option<String>,
    action: String,
) -> Result<()> {
    let (host, port, token) = resolve_target(host, port, device)?;

    // INFO: 5-second connect timeout so a dead host fails fast.
    let client = timeout(
        Duration::from_secs(5),
        crate::commands::connect(&host, port),
    )
    .await??;

    // INFO: `user/repo` is normalized to `user_repo` to match the on-disk app dir.
    let app_normalize = app.replace("/", "_");

    let reply = client.manage(token, app_normalize, action).await?;

    if reply.success {
        info!("SUCCESS: {}", reply.message);
        Ok(())
    } else {
        // ERROR: Surface the daemon's reason (e.g. "Invalid token") as a CLI error
        //        instead of silently succeeding.
        anyhow::bail!("{}", reply.message);
    }
}

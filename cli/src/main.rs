//! flare — the command-line client.
//!
//! Sub-commands: auth, deploy, start/stop/restart/rollback, discover, sync, devices.
//! It talks to a `flared` daemon over TLS using the framed JSON protocol from `common`.
//!
//! Comment legend: INFO / TODO / WARN / ERROR / FIXME / HACK / NOTE.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::error;

mod commands;
mod tls;

#[derive(Parser)]
#[command(name = "flare", version, about = "Flare CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,

    // INFO: Global defaults. `--host localhost` and `--port 7530` match a daemon
    //       running on the same machine.
    #[arg(long, default_value = "localhost", global = true)]
    host: String,

    #[arg(long, default_value_t = 7530, global = true)]
    port: u16,
}

#[derive(Subcommand)]
enum Cmd {
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    Deploy {
        repo: String,
        // INFO: `--device <id|name>` uses a device saved by `flare sync`.
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        github: bool,
        // INFO: `github` (the default) uses the GitHub tarball API; any other
        //       value is treated as a Gitea/Forgejo base URL.
        #[arg(long, default_value = "github")]
        forge: String,
        // INFO: Git branch/ref to deploy. Defaults to `main`.
        #[arg(long, default_value = "main")]
        branch: String,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        user: Option<String>,
    },
    Start {
        app: String,
        // INFO: Manage a saved device (uses its token). Without `--device`, the
        //       token is looked up from the host/port below.
        #[arg(long)]
        device: Option<String>,
    },
    Stop {
        app: String,
        #[arg(long)]
        device: Option<String>,
    },
    Restart {
        app: String,
        #[arg(long)]
        device: Option<String>,
    },
    Rollback {
        app: String,
        #[arg(long)]
        device: Option<String>,
    },
    Discover,
    Sync {
        range: String,
        // INFO: One-time pairing code from the daemon's startup log. Only needed
        //       for the first-time pairing of a fresh daemon.
        #[arg(long)]
        code: Option<String>,
    },
    Devices {
        #[command(subcommand)]
        action: Option<DeviceAction>,
    },
}

#[derive(Subcommand)]
enum DeviceAction {
    Rm { id: String },
}

#[derive(Subcommand)]
enum AuthAction {
    Login,
    Logout,
    Status,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        error!("{}", e);
        std::process::exit(1);
    }
}

// INFO: Top-level dispatcher. `deploy` and the manage commands can target a
//       saved device via `--device`.
async fn run(cli: Cli) -> Result<()> {
    use commands::*;

    match cli.cmd {
        Cmd::Auth { action } => match action {
            AuthAction::Login => auth::login(),
            AuthAction::Logout => auth::logout(),
            AuthAction::Status => auth::status(),
        },
        Cmd::Deploy {
            repo,
            device,
            github,
            forge,
            branch,
            token,
            user,
        } => {
            if let Some(dev) = device {
                // deploy to saved device
                deploy::run_to_device(&dev, repo, github, forge, branch, token, user).await
            } else {
                // deploy to host from CLI args
                deploy::run(cli.host, cli.port, repo, github, forge, branch, token, user).await
            }
        }

        Cmd::Start { app, device } => apps::start(cli.host.clone(), cli.port, app, device).await,
        Cmd::Stop { app, device } => apps::stop(cli.host.clone(), cli.port, app, device).await,
        Cmd::Restart { app, device } => {
            apps::restart(cli.host.clone(), cli.port, app, device).await
        }
        Cmd::Rollback { app, device } => {
            apps::rollback(cli.host.clone(), cli.port, app, device).await
        }

        Cmd::Discover => discovery::discover().await,
        Cmd::Sync { range, code } => discovery::sync(range, code).await,

        Cmd::Devices { action } => match action {
            None => devices::list(),
            Some(DeviceAction::Rm { id }) => devices::remove(&id),
        },
    }
}

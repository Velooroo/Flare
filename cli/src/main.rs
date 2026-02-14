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

    #[arg(long, default_value = "127.0.0.1", global = true)]
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
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        github: bool,
        #[arg(long, default_value = "http://localhost:8080")]
        forge: String,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        user: Option<String>,
    },
    Start {
        app: String,
    },
    Stop {
        app: String,
    },
    Restart {
        app: String,
    },
    Rollback {
        app: String,
    },
    Discover,
    Sync {
        range: String,
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
            token,
            user,
        } => {
            if let Some(dev) = device {
                // deploy to saved device
                deploy::run_to_device(&dev, repo, github, forge, token, user).await
            } else {
                // deploy to host from CLI args
                deploy::run(cli.host, cli.port, repo, github, forge, token, user).await
            }
        }

        Cmd::Start { app } => apps::start(cli.host.clone(), cli.port, app).await,
        Cmd::Stop { app } => apps::stop(cli.host.clone(), cli.port, app).await,
        Cmd::Restart { app } => apps::restart(cli.host.clone(), cli.port, app).await,
        Cmd::Rollback { app } => apps::rollback(cli.host.clone(), cli.port, app).await,

        Cmd::Discover => discovery::discover().await,
        Cmd::Sync { range } => discovery::sync(range).await,

        Cmd::Devices { action } => match action {
            None => devices::list(),
            Some(DeviceAction::Rm { id }) => devices::remove(&id),
        },
    }
}

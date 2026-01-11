use anyhow::Result;
use common::{
    DeployRequest, ManageRequest, ManageResponse, RegisterTokenRequest, recv_json, send_json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_rustls::server::TlsStream;
use tracing::{error, info, warn};

pub type Routes = Arc<RwLock<GatewayState>>;

#[derive(Default)]
pub struct GatewayState {
    pub static_sites: HashMap<String, String>,
    pub proxy_routes: HashMap<String, u16>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TokenStore {
    tokens: Vec<String>,
}

fn tokens_path() -> PathBuf {
    common::flare_dir().join("daemon_tokens.toml")
}

fn load_tokens() -> TokenStore {
    let path = tokens_path();
    if !path.exists() {
        return TokenStore::default();
    }

    let content = std::fs::read_to_string(&path).unwrap_or_default();
    toml::from_str(&content).unwrap_or_default()
}

fn save_tokens(store: &TokenStore) -> Result<()> {
    let path = tokens_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, toml::to_string(store)?)?;
    Ok(())
}

pub async fn run(port: u16) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Listening on port {}", port);

    let routes: Routes = Arc::new(RwLock::new(GatewayState::default()));

    // start gateway
    let routes_clone = routes.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::gateway::run(routes_clone).await {
            error!("Gateway error: {}", e);
        }
    });

    // start discovery
    tokio::spawn(async move {
        if let Err(e) = crate::discovery::run(7001).await {
            error!("Discovery error: {}", e);
        }
    });

    loop {
        let (tcp, addr) = listener.accept().await?;
        info!("Connection from {}", addr);

        let socket = match crate::tls::accept(tcp).await {
            Ok(s) => s,
            Err(e) => {
                error!("TLS handshake failed: {}", e);
                continue;
            }
        };

        let routes = routes.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(socket, routes).await {
                error!("Handler error: {}", e);
            }
        });
    }
}

async fn handle(mut socket: TlsStream<TcpStream>, routes: Routes) -> Result<()> {
    let msg: serde_json::Value = common::recv_json(&mut socket).await?;

    tracing::info!(
        "Raw message: {}",
        serde_json::to_string_pretty(&msg).unwrap()
    );

    let msg_type = msg.get("msg_type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "register_token" => {
            let req: RegisterTokenRequest = serde_json::from_value(msg)?;
            handle_register_token(socket, req).await
        }
        "deploy" => {
            let req: DeployRequest = serde_json::from_value(msg)?;
            handle_deploy(socket, routes, req).await
        }
        "manage" => {
            let req: ManageRequest = serde_json::from_value(msg)?;
            handle_manage(socket, req).await
        }
        _ => {
            warn!("Unknown message type: {}", msg_type);
            Ok(())
        }
    }
}

async fn handle_manage(
    mut socket: tokio_rustls::server::TlsStream<TcpStream>,
    req: ManageRequest,
) -> Result<()> {
    let result = match req.action.as_str() {
        "start" => start_app(&req.app),
        "stop" => stop_app(&req.app),
        "restart" => restart_app(&req.app),
        "rollback" => rollback_app(&req.app), // добавь
        _ => Err(anyhow::anyhow!("Unknown action")),
    };

    let response = match result {
        Ok(msg) => ManageResponse {
            success: true,
            message: msg,
        },
        Err(e) => ManageResponse {
            success: false,
            message: e.to_string(),
        },
    };

    common::send_json(&mut socket, &response).await
}

fn start_app(app: &str) -> Result<String> {
    let dir = common::app_dir(app);
    let mut state = common::load_state(&dir)?.ok_or_else(|| anyhow::anyhow!("App not found"))?;

    if state.status == "running" {
        return Ok("Already running".into());
    }

    let config = common::load_app_config(&dir)?;
    let run = config
        .run
        .ok_or_else(|| anyhow::anyhow!("No [run] section"))?;

    let child = std::process::Command::new("systemd-run")
        .args(["--user", "--scope", "sh", "-c", &run.command])
        .current_dir(&dir)
        .spawn()?;

    let pid = child.id();

    state.status = "running".into();
    state.pid = Some(pid);
    common::save_state(&dir, &state)?;

    Ok(format!("Started with PID {}", pid))
}

fn stop_app(app: &str) -> Result<String> {
    let dir = common::app_dir(app);
    let mut state = common::load_state(&dir)?.ok_or_else(|| anyhow::anyhow!("App not found"))?;

    if let Some(pid) = state.pid {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }

    state.status = "stopped".into();
    state.pid = None;
    common::save_state(&dir, &state)?;

    Ok("Stopped".into())
}

fn restart_app(app: &str) -> Result<String> {
    stop_app(app)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    start_app(app)
}

fn rollback_app(app: &str) -> Result<String> {
    let dir = common::app_dir(app);
    let versions = dir.join("versions");

    let mut entries: Vec<_> = std::fs::read_dir(&versions)?
        .filter_map(|e| e.ok())
        .collect();

    entries.sort_by_key(|e| e.path());

    let latest = entries
        .last()
        .ok_or_else(|| anyhow::anyhow!("No backups found"))?;

    let current = dir.join("current");

    if current.exists() {
        std::fs::remove_file(&current)?;
    }

    std::os::unix::fs::symlink(latest.path(), &current)?;

    // restart if running
    let state = common::load_state(&dir)?;
    if let Some(s) = state {
        if s.status == "running" {
            restart_app(app)?;
        }
    }

    Ok("Rolled back".into())
}

async fn handle_register_token(
    mut socket: tokio_rustls::server::TlsStream<TcpStream>,
    req: common::RegisterTokenRequest,
) -> Result<()> {
    let mut store = load_tokens();
    // add token hash
    store.tokens.push(req.token_hash);

    save_tokens(&store)?;
    info!("Registered new token");

    let resp = common::RegisterTokenResponse { success: true };
    common::send_json(&mut socket, &resp).await?;

    Ok(())
}

async fn handle_deploy(
    mut socket: tokio_rustls::server::TlsStream<TcpStream>,
    routes: Routes,
    req: common::DeployRequest,
) -> Result<()> {
    // verify token
    let store = load_tokens();

    let token = req.daemon_token.as_deref().unwrap_or("");
    let valid = store
        .tokens
        .iter()
        .any(|hash| common::verify_token(token, hash));

    if !valid {
        warn!("Invalid token");
        let response = common::DeployResponse {
            success: false,
            message: "Invalid token".into(),
            app_dir: None,
        };
        common::send_json(&mut socket, &response).await?;
        return Ok(());
    }

    info!("Deploy: {}", req.repo);

    let response = match crate::deploy::run(&req, routes).await {
        Ok(dir) => common::DeployResponse {
            success: true,
            message: format!("Deployed to {}", dir.display()),
            app_dir: Some(dir.to_string_lossy().into()),
        },
        Err(e) => common::DeployResponse {
            success: false,
            message: e.to_string(),
            app_dir: None,
        },
    };

    common::send_json(&mut socket, &response).await
}

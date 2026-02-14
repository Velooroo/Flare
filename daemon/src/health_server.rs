use anyhow::Result;
use axum::{Router, extract::State, http::StatusCode, response::Json};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone)]
pub struct AppState {
    // app_name -> PID
    pub pids: Arc<RwLock<HashMap<String, Option<u32>>>>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    app: String,
    pid: Option<u32>,
    timestamp: i64,
}

pub async fn run(pids: Arc<RwLock<HashMap<String, Option<u32>>>>) -> Result<()> {
    let state = AppState { pids };
    let app = Router::new()
        .route("/health/:app_name", axum::routing::get(handler))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:7531").await?;
    info!("Health server on :7531");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Ok(())
}

async fn handler(
    State(state): State<AppState>,
    axum::extract::Path(app_name): axum::extract::Path<String>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let pids = state.pids.read().await;

    // Get PID for this app
    let pid = pids.get(&app_name).and_then(|p| *p);

    // Check if process is still running
    let is_alive = pid.map(|p| is_process_alive(p)).unwrap_or(false);

    let response = HealthResponse {
        status: if is_alive { "healthy" } else { "unhealthy" }.to_string(),
        app: app_name,
        pid,
        timestamp: chrono::Utc::now().timestamp(),
    };

    Ok(Json(response))
}

fn is_process_alive(pid: u32) -> bool {
    use std::process::Command;

    // Linux/Unix: check if process exists
    let output = Command::new("kill")
        .arg("-0") // Doesn't actually kill, just checks
        .arg(pid.to_string())
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

// Called when app starts/stops
pub fn update_pid(
    pids: &Arc<RwLock<HashMap<String, Option<u32>>>>,
    app_name: &str,
    pid: Option<u32>,
) {
    let mut pids = pids.blocking_write();
    pids.insert(app_name.to_string(), pid);
}

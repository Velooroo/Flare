use anyhow::Result;
use axum::{
    Router,
    body::Body,
    extract::{Host, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use tokio::net::TcpListener;
use tower::ServiceExt;
use tower_http::services::ServeDir;
use tracing::info;

use crate::server::Routes;

pub async fn run(routes: Routes) -> Result<()> {
    let app = Router::new().fallback(handler).with_state(routes);

    let listener = TcpListener::bind("0.0.0.0:80").await?;
    info!("Gateway on :80");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn handler(State(routes): State<Routes>, Host(host): Host, req: Request<Body>) -> Response {
    let host = host.split(':').next().unwrap_or(&host).to_string();
    let state = routes.read().await;
    let path = req.uri().path();

    // Handle /health through Flare's health server
    if path.starts_with("/health") {
        return proxy_to_health_server(path).await;
    }

    if let Some(path) = state.static_sites.get(&host) {
        return match ServeDir::new(path).oneshot(req).await {
            Ok(r) => r.into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };
    }

    if let Some(port) = state.proxy_routes.get(&host) {
        return (StatusCode::OK, format!("proxy -> localhost:{}", port)).into_response();
    }

    (StatusCode::NOT_FOUND, "Not found").into_response()
}

async fn proxy_to_health_server(path: &str) -> Response {
    let url = format!("http://localhost:7531{}", path);

    match reqwest::get(&url).await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let body = resp.bytes().await.unwrap_or_default();

            Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Health check failed: {}", e),
        )
            .into_response(),
    }
}

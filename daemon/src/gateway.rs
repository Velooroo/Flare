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

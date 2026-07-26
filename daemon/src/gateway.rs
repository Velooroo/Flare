use axum::{
    Router,
    body::Body,
    extract::{Host, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower::ServiceExt;
use tower_http::services::ServeDir;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub enum Route {
    Static { root: String },
    App { port: u16, pid: Option<u32> },
}

pub type Routes = Arc<RwLock<HashMap<String, Route>>>;

fn gateway_port() -> u16 {
    std::env::var("FLARE_GATEWAY_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(80)
}

pub async fn run(routes: Routes) {
    let app = Router::new()
        .route("/.flare/health", axum::routing::get(|| async { "ok" }))
        .fallback(handler)
        .with_state(routes);

    let port = gateway_port();
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    info!("Gateway on :{}", port);

    axum::serve(listener, app).await.unwrap();
}

async fn handler(
    State(routes): State<Routes>,
    Host(host): Host,
    req: Request<Body>,
) -> Response {
    let host = host.split(':').next().unwrap_or(&host).to_string();
    let path = req.uri().path().to_string();
    let guard = routes.read().await;
    let route = guard.get(&host).or_else(|| guard.get("*"));

    match route {
        Some(Route::Static { root }) => match ServeDir::new(root).oneshot(req).await {
            Ok(r) => r.into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        Some(Route::App { port, pid }) => {
            if path == "/health" {
                return app_health(host, *port, *pid);
            }
            proxy_to_backend(*port, req).await
        }
        None => {
            let hosts: Vec<_> = guard.keys().collect();
            (StatusCode::NOT_FOUND, format!("No route for {}\nHosts: {:?}", host, hosts))
                .into_response()
        }
    }
}

fn app_health(host: String, port: u16, pid: Option<u32>) -> Response {
    use crate::app::AppManager;

    let alive = if let Some(p) = pid {
        AppManager::process_alive(p)
    } else {
        std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
    };

    let json = serde_json::json!({
        "app": host,
        "alive": alive,
    });

    let status = if alive { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, [("content-type", "application/json")], json.to_string()).into_response()
}

async fn proxy_to_backend(port: u16, req: Request<Body>) -> Response {
    let client = reqwest::Client::new();
    let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let url = format!("http://127.0.0.1:{}{}", port, path);

    let method = match req.method().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        other => other.parse().unwrap_or(reqwest::Method::GET),
    };

    let body = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    let rb = if body.is_empty() {
        client.request(method, &url)
    } else {
        client.request(method, &url).body(body.to_vec())
    };

    match rb.send().await {
        Ok(r) => {
            let code = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let b = r.bytes().await.unwrap_or_default();
            (code, Body::from(b)).into_response()
        }
        Err(e) => {
            warn!("Proxy error :{}: {}", port, e);
            (StatusCode::SERVICE_UNAVAILABLE, Body::from("backend down")).into_response()
        }
    }
}

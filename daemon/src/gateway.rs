//! Built-in HTTP(S) gateway on port 80 (override with `FLARE_GATEWAY_PORT`).
//!
//! INFO: Requests are matched by the `Host` header:
//!   * `Route::Static` — served from disk via `ServeDir`
//!   * `Route::App`    — reverse-proxied to `127.0.0.1:<port>`
//!   * `*`             — a catch-all route if one was registered
//! A special `/.flare/health` route always answers `ok`.
//!
//! INFO: If `$FLARE_TLS_CERT` and `$FLARE_TLS_KEY` are set, the gateway serves
//!       HTTPS; otherwise it serves plain HTTP.

use axum::{
    Router,
    body::Body,
    extract::{Host, State},
    http::{HeaderName, HeaderValue, Request, StatusCode},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
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

// INFO: One shared client for all proxied requests (connection pooling).
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

// INFO: Returns an error instead of panicking when the port is taken/privileged.
pub async fn run(routes: Routes) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/.flare/health", axum::routing::get(|| async { "ok" }))
        .fallback(handler)
        .with_state(routes);

    let port = gateway_port();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    match (
        std::env::var("FLARE_TLS_CERT"),
        std::env::var("FLARE_TLS_KEY"),
    ) {
        (Ok(cert), Ok(key)) => {
            let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key).await?;
            info!("Gateway (HTTPS) on :{}", port);
            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service())
                .await?;
        }
        _ => {
            let listener = TcpListener::bind(addr).await?;
            info!("Gateway (HTTP) on :{}", port);
            axum::serve(listener, app).await?;
        }
    }

    Ok(())
}

async fn handler(State(routes): State<Routes>, Host(host): Host, req: Request<Body>) -> Response {
    // INFO: Strip the optional :port from the Host header before matching.
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
                return app_health(host, *port, *pid).await;
            }
            proxy_to_backend(*port, req).await
        }
        None => {
            let hosts: Vec<_> = guard.keys().collect();
            (
                StatusCode::NOT_FOUND,
                format!("No route for {}\nHosts: {:?}", host, hosts),
            )
                .into_response()
        }
    }
}

// INFO: Async liveness check (no blocking connect).
async fn app_health(host: String, port: u16, pid: Option<u32>) -> Response {
    use crate::app::AppManager;

    let alive = if let Some(p) = pid {
        AppManager::process_alive(p)
    } else {
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
    };

    let json = serde_json::json!({
        "app": host,
        "alive": alive,
    });

    let status = if alive {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        [("content-type", "application/json")],
        json.to_string(),
    )
        .into_response()
}

// INFO: Minimal reverse proxy. Forwards method, path+query, request headers, and
//       the body; copies response status, headers, and body back.
// WARN: Bodies are fully buffered (up to 10 MB). No streaming or WebSocket yet.
async fn proxy_to_backend(port: u16, req: Request<Body>) -> Response {
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let url = format!("http://127.0.0.1:{}{}", port, path);

    let method = match req.method().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        other => other.parse().unwrap_or(reqwest::Method::GET),
    };

    // INFO: Collect headers before consuming the request body.
    let headers: Vec<(HeaderName, HeaderValue)> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let body = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    let mut rb = client().request(method, &url);
    for (name, value) in headers {
        // INFO: Skip Host / Content-Length (reqwest sets them). Values are sent as
        //       UTF-8 strings because axum (http 1.x) and reqwest 0.11 (http 0.2)
        //       use different header types.
        if name == axum::http::header::HOST || name == axum::http::header::CONTENT_LENGTH {
            continue;
        }
        if let Ok(v) = value.to_str() {
            rb = rb.header(name.as_str(), v);
        }
    }
    if !body.is_empty() {
        rb = rb.body(body.to_vec());
    }

    match rb.send().await {
        Ok(r) => {
            let status =
                StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

            // INFO: Rebuild the response, copying upstream headers as strings.
            let mut builder = Response::builder().status(status);
            for (name, value) in r.headers() {
                if let Ok(v) = value.to_str() {
                    builder = builder.header(name.as_str(), v);
                }
            }

            let bytes = match r.bytes().await {
                Ok(b) => b,
                Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
            };

            match builder.body(Body::from(bytes)) {
                Ok(resp) => resp,
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => {
            warn!("Proxy error :{}: {}", port, e);
            (StatusCode::SERVICE_UNAVAILABLE, Body::from("backend down")).into_response()
        }
    }
}

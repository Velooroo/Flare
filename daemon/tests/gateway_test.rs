use axum::{Router, body::Body, extract::{Host, State}, http::{Request, StatusCode}, response::{IntoResponse, Response}};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
enum Route {
    #[allow(dead_code)]
    Static { root: String },
    App { port: u16, pid: Option<u32> },
}

type Routes = Arc<RwLock<HashMap<String, Route>>>;

fn make_app(routes: Routes) -> Router {
    Router::new()
        .route("/.flare/health", axum::routing::get(|| async { "ok" }))
        .fallback(handler)
        .with_state(routes)
}

async fn handler(
    State(routes): State<Routes>,
    Host(host): Host,
    req: Request<Body>,
) -> Response {
    let host = host.split(':').next().unwrap_or(&host).to_string();
    let guard = routes.read().await;
    let route = guard.get(&host).or_else(|| guard.get("*"));

    match route {
        Some(Route::App { port, .. }) => {
            let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");
            let url = format!("http://127.0.0.1:{}{}", port, path);
            let method = req.method().clone();
            let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await.unwrap();

            let reqwest_method = match method.as_str() {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "PUT" => reqwest::Method::PUT,
                "DELETE" => reqwest::Method::DELETE,
                other => other.parse().unwrap_or(reqwest::Method::GET),
            };

            let rb = if body_bytes.is_empty() {
                reqwest::Client::new().request(reqwest_method, &url)
            } else {
                reqwest::Client::new().request(reqwest_method, &url).body(body_bytes.to_vec())
            };

            match rb.send().await {
                Ok(r) => {
                    let code = StatusCode::from_u16(r.status().as_u16()).unwrap();
                    let body = r.bytes().await.unwrap_or_default();
                    (code, Body::from(body)).into_response()
                }
                Err(e) => (StatusCode::SERVICE_UNAVAILABLE, format!("backend down: {}", e)).into_response()
            }
        }
        _ => (StatusCode::NOT_FOUND, "no route").into_response(),
    }
}

#[tokio::test]
async fn test_proxy_routes_to_backend() {
    // start fake backend
    let backend = Router::new()
        .route("/", axum::routing::get(|| async { "root" }))
        .route("/api/test", axum::routing::get(|| async { "api-ok" }));

    let bl = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = bl.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(bl, backend).await.unwrap() });

    // start gateway with route pointing to backend
    let routes: Routes = Arc::new(RwLock::new(HashMap::new()));
    routes.write().await.insert("test.local".into(), Route::App {
        port: backend_addr.port(),
        pid: None,
    });

    let gw = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_addr = gw.local_addr().unwrap();
    let gw_routes = routes.clone();
    tokio::spawn(async move {
        axum::serve(gw, make_app(gw_routes)).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client = reqwest::Client::new();

    // health endpoint
    let resp = client.get(format!("http://{}:{}/.flare/health", gw_addr.ip(), gw_addr.port()))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");

    // proxy to backend root
    let resp = client.get(format!("http://{}:{}/", gw_addr.ip(), gw_addr.port()))
        .header("Host", "test.local")
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "root");

    // proxy to backend /api/test
    let resp = client.get(format!("http://{}:{}/api/test", gw_addr.ip(), gw_addr.port()))
        .header("Host", "test.local")
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "api-ok");

    // wrong host = no route
    let resp = client.get(format!("http://{}:{}/", gw_addr.ip(), gw_addr.port()))
        .header("Host", "unknown.local")
        .send().await.unwrap();
    assert_eq!(resp.status(), 404);
}

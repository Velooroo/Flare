//! Round-trips the generated `common::control` service over Flare's TLS.
//!
//! This proves the four control methods (register_token / deploy / manage /
//! health) serialize correctly between the generated client and the generated
//! server trait, on the same pinned-TLS transport the daemon uses.

use std::sync::Arc;

struct Dummy;

impl common::control::Handler for Dummy {
    async fn register_token(
        &self,
        token_hash: String,
        existing_token: Option<String>,
        pairing_code: Option<String>,
    ) -> common::RegisterReply {
        let _ = (existing_token, pairing_code);
        common::RegisterReply {
            success: token_hash == "good",
        }
    }

    async fn deploy(
        &self,
        token: Option<String>,
        repo: String,
        forge: String,
        branch: Option<String>,
        auth_user: Option<String>,
        auth_password: Option<String>,
    ) -> common::DeployReply {
        let _ = (token, branch, auth_user, auth_password);
        common::DeployReply {
            success: true,
            message: format!("{forge}:{repo}"),
            app_dir: Some("/tmp/app".into()),
        }
    }

    async fn manage(
        &self,
        token: Option<String>,
        app: String,
        action: String,
    ) -> common::ManageReply {
        common::ManageReply {
            success: token.is_some(),
            message: format!("{action} {app}"),
        }
    }

    async fn health(&self, token: Option<String>, app: String) -> common::HealthReply {
        common::HealthReply {
            app,
            alive: token.is_some(),
            pid: Some(7),
            message: token.is_none().then(|| "Invalid token".into()),
        }
    }
}

fn isolate_flare_home() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "flare_control_test_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp FLARE_HOME");
    // SAFETY: this integration test binary has a single test, and it sets the
    // variable before touching any Flare path.
    unsafe {
        std::env::set_var("FLARE_HOME", &dir);
    }
    dir
}

#[tokio::test]
async fn control_service_roundtrips_over_tls() {
    isolate_flare_home();

    let mut dispatcher = common::monolith::Dispatcher::new();
    common::control::register(&mut dispatcher, Dummy);
    let dispatcher = Arc::new(dispatcher);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("listener addr");
    let server = tokio::spawn(common::rpc::serve_on(dispatcher, listener));

    let client = common::control::Client::from_endpoint(
        common::rpc::connect_endpoint("127.0.0.1", addr.port())
            .await
            .expect("TLS connect"),
    );

    let ok = client
        .register_token("good".into(), None, Some("CODE".into()))
        .await
        .expect("register_token");
    assert!(ok.success);

    let bad = client
        .register_token("bad".into(), None, None)
        .await
        .expect("register_token");
    assert!(!bad.success);

    let deploy = client
        .deploy(
            Some("t".into()),
            "repo".into(),
            "github".into(),
            None,
            None,
            None,
        )
        .await
        .expect("deploy");
    assert!(deploy.success);
    assert_eq!(deploy.message, "github:repo");
    assert_eq!(deploy.app_dir.as_deref(), Some("/tmp/app"));

    let manage = client
        .manage(Some("t".into()), "app".into(), "start".into())
        .await
        .expect("manage");
    assert!(manage.success);
    assert_eq!(manage.message, "start app");

    let health = client.health(None, "app".into()).await.expect("health");
    assert_eq!(health.app, "app");
    assert_eq!(health.pid, Some(7));
    assert!(!health.alive);
    assert_eq!(health.message.as_deref(), Some("Invalid token"));

    server.abort();
}

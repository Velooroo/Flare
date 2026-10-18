//! End-to-end proof that monolith runs over Flare's TLS transport and that the
//! certificate pin is actually enforced.

use std::sync::Arc;

monolith::api! {
    service demo {
        fn ping(text: String) -> String;
    }
}

struct Demo;

impl demo::Handler for Demo {
    async fn ping(&self, text: String) -> String {
        format!("pong:{text}")
    }
}

fn isolate_flare_home() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "flare_tls_test_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp FLARE_HOME");
    // SAFETY: this integration test binary only has one test, and it sets the
    // variable before touching any Flare path.
    unsafe {
        std::env::set_var("FLARE_HOME", &dir);
    }
    dir
}

#[tokio::test]
async fn monolith_over_tls_and_pins_certificate() {
    isolate_flare_home();

    let mut dispatcher = monolith::Dispatcher::new();
    demo::register(&mut dispatcher, Demo);
    let dispatcher = Arc::new(dispatcher);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("listener addr");
    let server = tokio::spawn(common::rpc::serve_on(dispatcher, listener));

    // First connection: trust-on-first-use, then pin.
    let client = demo::Client::from_endpoint(
        common::rpc::connect_endpoint("127.0.0.1", addr.port())
            .await
            .expect("TLS connect"),
    );
    assert_eq!(client.ping("hi".into()).await.expect("call"), "pong:hi");

    let pin = common::security::pinned_fingerprint("127.0.0.1", addr.port())
        .expect("certificate should be pinned after the first connection");
    assert_eq!(pin.len(), 64, "fingerprint is SHA-256 hex");

    // Second connection: the pinned certificate still matches.
    let client = demo::Client::from_endpoint(
        common::rpc::connect_endpoint("127.0.0.1", addr.port())
            .await
            .expect("TLS connect with matching pin"),
    );
    assert_eq!(
        client.ping("again".into()).await.expect("call"),
        "pong:again"
    );

    // Tamper with the pin: the same daemon must now be refused.
    common::security::pin_fingerprint("127.0.0.1", addr.port(), "deadbeef").expect("rewrite pin");
    let err = match common::rpc::connect_endpoint("127.0.0.1", addr.port()).await {
        Ok(_) => panic!("connection should have been refused after the pin changed"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("changed"),
        "expected a pin-mismatch error, got: {err}"
    );

    server.abort();
}

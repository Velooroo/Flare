//! Unit tests for the framing protocol in `common::network`.
//! INFO: Uses in-memory `duplex` streams, so no sockets or TLS are involved.

use common::{recv_json, recv_msg, send_json, send_msg};
use tokio::io::duplex;

#[tokio::test]
async fn roundtrip_small() {
    let (mut tx, mut rx) = duplex(1024);
    let data = b"hello world";

    tokio::spawn(async move {
        send_msg(&mut tx, data).await.unwrap();
    });

    let result = recv_msg(&mut rx).await.unwrap();
    assert_eq!(result, data);
}

#[tokio::test]
async fn roundtrip_empty() {
    let (mut tx, mut rx) = duplex(64);
    let data: &[u8] = &[];

    tokio::spawn(async move {
        send_msg(&mut tx, data).await.unwrap();
    });

    let result = recv_msg(&mut rx).await.unwrap();
    assert_eq!(result, data);
}

#[tokio::test]
async fn roundtrip_large() {
    let (mut tx, mut rx) = duplex(2 * 1024 * 1024);
    let data = vec![0xABu8; 64 * 1024];
    let expected = data.clone();

    tokio::spawn(async move {
        send_msg(&mut tx, &data).await.unwrap();
    });

    let result = recv_msg(&mut rx).await.unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn json_roundtrip() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Clone)]
    struct TestMsg {
        field: String,
        count: u32,
    }

    let msg = TestMsg {
        field: "value".into(),
        count: 42,
    };
    let expected = msg.clone();

    let (mut tx, mut rx) = duplex(1024);

    tokio::spawn(async move {
        send_json(&mut tx, &msg).await.unwrap();
    });

    let result: TestMsg = recv_json(&mut rx).await.unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn multiple_messages() {
    let (mut tx, mut rx) = duplex(1024);

    tokio::spawn(async move {
        send_msg(&mut tx, b"first").await.unwrap();
        send_msg(&mut tx, b"second").await.unwrap();
    });

    let msg1 = recv_msg(&mut rx).await.unwrap();
    let msg2 = recv_msg(&mut rx).await.unwrap();

    assert_eq!(msg1, b"first");
    assert_eq!(msg2, b"second");
}

use common::{app_dir, generate_token, hash_token, next_device_id, verify_token, Device, FlareConfig};

#[test]
fn test_generate_token_length() {
    let token = generate_token();
    assert_eq!(token.len(), 64);
}

#[test]
fn test_generate_token_is_random() {
    let t1 = generate_token();
    let t2 = generate_token();
    assert_ne!(t1, t2);
}

#[test]
fn test_hash_and_verify_token() {
    let token = generate_token();
    let hash = hash_token(&token).unwrap();
    assert!(verify_token(&token, &hash));
}

#[test]
fn test_verify_wrong_token() {
    let token = generate_token();
    let hash = hash_token(&token).unwrap();
    assert!(!verify_token("wrong-token", &hash));
}

#[test]
fn test_verify_bad_hash_format() {
    assert!(!verify_token("anything", "not-a-valid-hash"));
}

#[test]
fn test_next_device_id_empty() {
    let config = FlareConfig { devices: vec![] };
    assert_eq!(next_device_id(&config), 1);
}

#[test]
fn test_next_device_id_increment() {
    let config = FlareConfig {
        devices: vec![
            Device {
                id: 5,
                name: None,
                host: "a".into(),
                port: 7530,
                token: None,
            },
            Device {
                id: 3,
                name: None,
                host: "b".into(),
                port: 7530,
                token: None,
            },
        ],
    };
    assert_eq!(next_device_id(&config), 6);
}

#[test]
fn test_app_dir_sanitizes_slashes() {
    let dir = app_dir("user/repo");
    let ends_with = dir.to_string_lossy().ends_with("user_repo");
    assert!(
        ends_with,
        "Expected path ending with 'user_repo', got {:?}",
        dir
    );
}

#[test]
fn test_app_dir_no_slashes() {
    let dir = app_dir("myapp");
    let ends_with = dir.to_string_lossy().ends_with("myapp");
    assert!(
        ends_with,
        "Expected path ending with 'myapp', got {:?}",
        dir
    );
}

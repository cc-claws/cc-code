use super::*;

#[test]
fn test_http_health_initially_closed() {
    let health = HttpHookHealth::new();
    assert!(!health.is_open());
}

#[test]
fn test_http_health_trip_and_is_open() {
    let health = HttpHookHealth::new();
    let first_trip = health.trip();
    assert!(first_trip);
    assert!(health.is_open());
}

#[test]
fn test_http_health_trip_dedup() {
    let health = HttpHookHealth::new();
    assert!(health.trip());
    assert!(!health.trip());
    assert!(!health.trip());
    assert!(health.is_open());
}

#[test]
fn test_http_health_reset() {
    let health = HttpHookHealth::new();
    health.trip();
    assert!(health.is_open());
    health.reset();
    assert!(!health.is_open());
}

#[test]
fn test_extract_endpoint_formats() {
    assert_eq!(
        extract_endpoint("http://127.0.0.1:23333/permission"),
        "http://127.0.0.1:23333"
    );
    assert_eq!(
        extract_endpoint("http://127.0.0.1:23333/other/path?foo=bar"),
        "http://127.0.0.1:23333"
    );
    assert_eq!(
        extract_endpoint("https://api.example.com/hooks"),
        "https://api.example.com"
    );
    assert_eq!(extract_endpoint("invalid-url"), "invalid-url");
}

#[tokio::test]
async fn test_http_health_for_shares_endpoint() {
    let _guard = test_lock().await;
    reset_all_http_health_for_test();
    let h1 = http_health_for("http://127.0.0.1:23333/permission");
    let h2 = http_health_for("http://127.0.0.1:23333/other_route");
    let h3 = http_health_for("http://127.0.0.1:24444/permission");

    assert!(!h1.is_open());
    assert!(!h2.is_open());
    assert!(!h3.is_open());

    h1.trip();
    assert!(h1.is_open());
    assert!(h2.is_open());
    assert!(!h3.is_open());

    reset_all_http_health_for_test();
}

#[tokio::test]
async fn test_connect_timeout_env_override() {
    let _guard = test_lock().await;
    assert_eq!(
        connect_timeout(),
        Duration::from_millis(DEFAULT_CONNECT_TIMEOUT_MS)
    );
    std::env::set_var("HTTP_HOOK_CONNECT_TIMEOUT_MS", "1500");
    assert_eq!(connect_timeout(), Duration::from_millis(1500));
    std::env::remove_var("HTTP_HOOK_CONNECT_TIMEOUT_MS");
    assert_eq!(
        connect_timeout(),
        Duration::from_millis(DEFAULT_CONNECT_TIMEOUT_MS)
    );
}

use super::*;

#[test]
fn test_bind_failed_error_format() {
    let err = CallbackError::BindFailed("addr in use".to_string());
    assert!(err.to_string().contains("绑定失败"));
}

#[test]
fn test_bind_returns_valid_redirect_uri() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(OAuthCallbackServer::bind());
    assert!(result.is_ok());
    let (_server, uri) = result.unwrap();
    assert!(uri.starts_with("http://127.0.0.1:"));
    assert!(uri.ends_with("/callback"));
}

#[test]
fn test_parse_callback_url_valid() {
    let result = parse_callback_url("/callback?code=abc123&state=mystate", "mystate");
    assert!(result.is_ok());
    let (code, state) = result.unwrap();
    assert_eq!(code, "abc123");
    assert_eq!(state, "mystate");
}

#[test]
fn test_parse_callback_url_missing_code() {
    let result = parse_callback_url("/callback?state=mystate", "mystate");
    assert!(result.is_err());
}

#[test]
fn test_parse_callback_url_missing_state() {
    let result = parse_callback_url("/callback?code=abc123", "mystate");
    assert!(result.is_err());
}

#[test]
fn test_parse_callback_url_invalid_path() {
    let result = parse_callback_url("not-a-url", "mystate");
    assert!(result.is_err());
}

#[test]
fn test_parse_callback_url_state_mismatch() {
    let result = parse_callback_url("/callback?code=abc&state=wrong", "correct");
    assert!(result.is_err());
}

#[test]
fn test_parse_code_from_url_valid() {
    let result = parse_code_from_url("http://localhost:12345/callback?code=xyz&state=s");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_wait_for_code_timeout() {
    let (server, _uri) = OAuthCallbackServer::bind().await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        server.wait_for_code(),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bind_multiple_servers() {
    let (s1, uri1) = OAuthCallbackServer::bind().await.unwrap();
    let (s2, uri2) = OAuthCallbackServer::bind().await.unwrap();
    assert_ne!(uri1, uri2);
    drop(s1);
    drop(s2);
}

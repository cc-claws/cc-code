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

/// #16：set_expected_state 应该更新内部 state_param，
/// 使 wait_for_code 通过 parse_callback_url 严格校验 state 一致。
#[tokio::test]
async fn test_set_expected_state_updates_validation() {
    let (mut server, _uri) = OAuthCallbackServer::bind().await.unwrap();
    // 默认 state_param 为空字符串
    assert_eq!(server.state_param, "");
    server.set_expected_state("csrf-token-123");
    assert_eq!(server.state_param, "csrf-token-123");
}

/// #16：state 校验函数在 expected_state 非空时应严格匹配，
/// 不一致时返回 ParseFailed 错误（CSRF 防御）。
#[test]
fn test_parse_callback_url_strict_state_validation_when_nonempty() {
    // 一致 → 通过
    let ok = parse_callback_url("/cb?code=c&state=abc", "abc");
    assert!(ok.is_ok(), "state 一致应通过: {:?}", ok);

    // 不一致 → 拒绝
    let mismatch = parse_callback_url("/cb?code=c&state=xyz", "abc");
    assert!(mismatch.is_err(), "state 不一致应拒绝");
    let err_msg = mismatch.unwrap_err().to_string();
    assert!(
        err_msg.contains("CSRF") || err_msg.contains("state"),
        "错误信息应提及 CSRF/state: {err_msg}"
    );
}

/// #16：expected_state 为空时仍跳过校验（向后兼容老测试 / 未设置场景）。
#[test]
fn test_parse_callback_url_skip_validation_when_expected_empty() {
    let result = parse_callback_url("/cb?code=c&state=anything", "");
    assert!(result.is_ok(), "expected_state 为空时不应强制校验");
}

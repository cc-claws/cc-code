use super::*;

#[test]
fn test_retryable_http_429() {
    let err = AgentError::LlmHttpError {
        status: 429,
        message: "rate limited".into(),
    };
    assert!(err.is_retryable());
}

#[test]
fn test_retryable_http_503() {
    let err = AgentError::LlmHttpError {
        status: 503,
        message: "unavailable".into(),
    };
    assert!(err.is_retryable());
}

#[test]
fn test_retryable_http_408() {
    let err = AgentError::LlmHttpError {
        status: 408,
        message: "timeout".into(),
    };
    assert!(err.is_retryable());
}

#[test]
fn test_not_retryable_http_400() {
    let err = AgentError::LlmHttpError {
        status: 400,
        message: "bad request".into(),
    };
    assert!(!err.is_retryable());
}

#[test]
fn test_not_retryable_http_401() {
    let err = AgentError::LlmHttpError {
        status: 401,
        message: "unauthorized".into(),
    };
    assert!(!err.is_retryable());
}

#[test]
fn test_not_retryable_http_404() {
    let err = AgentError::LlmHttpError {
        status: 404,
        message: "not found".into(),
    };
    assert!(!err.is_retryable());
}

#[test]
fn test_retryable_network_connection() {
    let err = AgentError::LlmError("connection refused".into());
    assert!(err.is_retryable());
}

#[test]
fn test_retryable_connection_reset() {
    let err = AgentError::LlmError("connection reset by peer".into());
    assert!(err.is_retryable());
}

#[test]
fn test_not_retryable_connection_pool() {
    let err = AgentError::LlmError("connection pool is full".into());
    assert!(!err.is_retryable(), "connection pool 满不是临时网络错误");
}

#[test]
fn test_retryable_network_timeout() {
    let err = AgentError::LlmError("reqwest timeout exceeded".into());
    assert!(err.is_retryable());
}

#[test]
fn test_not_retryable_parse_error() {
    let err = AgentError::LlmError("parse error".into());
    assert!(!err.is_retryable());
}

#[test]
fn test_retryable_error_sending_request() {
    let err = AgentError::LlmError(
        "error sending request for url (https://token-plan-sgp.xiaomimimo.com/anthropic/v1/messages)".into(),
    );
    assert!(err.is_retryable(), "reqwest 网络层错误应可重试");
}

#[test]
fn test_retryable_error_decoding_response_body() {
    let err = AgentError::LlmError("流式读取失败: error decoding response body".into());
    assert!(err.is_retryable(), "流式传输解码错误应可重试");
}

#[test]
fn test_retryable_connection_closed() {
    let err = AgentError::LlmError("connection closed before message completed".into());
    assert!(err.is_retryable(), "连接中途关闭应可重试");
}

#[test]
fn test_retryable_incomplete_body() {
    let err = AgentError::LlmError("incomplete body".into());
    assert!(err.is_retryable(), "响应体截断应可重试");
}

#[test]
fn test_retryable_request_or_response_body() {
    let err = AgentError::LlmError("request or response body error: channel closed".into());
    assert!(err.is_retryable(), "reqwest body 错误应可重试");
}

#[test]
fn test_retryable_read_response_body_failed() {
    let err = AgentError::LlmError("读取响应体失败: error decoding response body".into());
    assert!(err.is_retryable(), "读取响应体失败含可重试关键词应可重试");
}

#[test]
fn test_not_retryable_other_errors() {
    let err = AgentError::ToolNotFound("x".into());
    assert!(!err.is_retryable());
}

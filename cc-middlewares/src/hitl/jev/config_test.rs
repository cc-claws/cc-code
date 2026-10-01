//! config.rs 单元测试

use super::*;

#[test]
fn test_default_config() {
    let c = JevConfig::default();
    assert!(c.endpoint.contains("localhost:20130"));
    // 拿不准 → 问人，而不是替人拒绝；用户可能确实想干这件事
    assert_eq!(c.uncertain, UncertainPolicy::Ask);
    assert_eq!(c.gate_scope, GateScope::All);
    // 5000：覆盖本地端点冷启动（实测冷态 >2s、热态 0.7–1.5s），
    // 2000 会让冷启动直接 timeout 变成无理由误拦。
    assert_eq!(c.timeout_ms, 5000);
}

#[test]
fn test_systemone_url() {
    let c = JevConfig {
        endpoint: "http://localhost:20130/".to_string(),
        ..Default::default()
    };
    assert_eq!(c.systemone_url(), "http://localhost:20130/v1/systemone");
}

#[test]
fn test_api_key_from_env() {
    let c = JevConfig {
        api_key_env: "JEV_TEST_KEY_XYZ".to_string(),
        ..Default::default()
    };
    // 未设置 → None
    assert!(c.api_key().is_none());
}

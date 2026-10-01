//! client.rs 单元测试（仅纯解析，不打网络）

use super::*;

fn questions(ids: &[&str]) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    for id in ids {
        m.insert(
            (*id).to_string(),
            json!({"type": "noul", "instructions": "?"}),
        );
    }
    m
}

#[test]
fn test_parse_extracts_noul() {
    let raw: RawResponse = serde_json::from_str(
        r#"{"model":"typesafe/jev-1.13","answers":{"q":{"type":"noul","noul":0.91}},"usage":{"cost":0.00001}}"#,
    )
    .unwrap();
    let out = JevClient::parse(raw, &questions(&["q"]));
    assert_eq!(out.probabilities.get("q"), Some(&0.91));
    assert_eq!(out.cost, 0.00001);
}

#[test]
fn test_parse_omits_missing_answers() {
    // 响应缺少 no_secret_egress → 概率表不含该 key（上层默认 0=拒绝）
    let raw: RawResponse = serde_json::from_str(
        r#"{"model":"x","answers":{"intent_coverage":{"type":"noul","noul":0.9}}}"#,
    )
    .unwrap();
    let out = JevClient::parse(raw, &questions(&["intent_coverage", "no_secret_egress"]));
    assert!(out.probabilities.contains_key("intent_coverage"));
    assert!(!out.probabilities.contains_key("no_secret_egress"));
}

#[test]
fn test_no_key_is_error() {
    let cfg = JevConfig {
        api_key_env: "JEV_DEFINITELY_UNSET_XYZ".to_string(),
        ..Default::default()
    };
    let client = JevClient::new(cfg).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let res = rt.block_on(client.judge(&json!({}), &serde_json::Map::new()));
    assert!(matches!(res, Err(JevError::NoKey)));
}

use std::fs;
use tempfile::tempdir;

use super::*;
use crate::hitl::jev::rules::{JevRule, JevRules};

fn mock_sha256(seed: u8) -> String {
    format!("{:02x}", seed).repeat(32)
}

#[test]
fn test_sanitize_project_name() {
    assert_eq!(sanitize_project_name("my_project"), "my_project");
    assert_eq!(sanitize_project_name("my-project"), "my-project");
    assert_eq!(sanitize_project_name("my@project#name!"), "my-project-name");
    assert_eq!(sanitize_project_name("---"), "project");
    assert_eq!(sanitize_project_name(""), "project");
}

#[test]
fn test_cached_rules_file_roundtrip() {
    let file = CachedRulesFile {
        version: 1,
        signature: mock_sha256(1),
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        project_path: Some("/mock/project".to_string()),
        provider: Some("mock-provider".to_string()),
        model: Some("mock-model".to_string()),
        sources: vec![SourceMeta {
            source: "project".to_string(),
            present: true,
            char_count: Some(100),
            sha256: Some(mock_sha256(2)),
        }],
        rules: vec![JevRule {
            text: "mock_rule_alpha".to_string(),
            source: "project".to_string(),
        }],
        protected_paths: vec!["mock/path/**".to_string()],
    };
    let json_bytes = serde_json::to_vec_pretty(&file).unwrap();
    let deserialized: CachedRulesFile = serde_json::from_slice(&json_bytes).unwrap();
    assert_eq!(file, deserialized);
    let rules = deserialized.to_jev_rules();
    assert_eq!(rules.rules.len(), 1);
    assert_eq!(rules.rules[0].text, "mock_rule_alpha");
}

#[test]
fn test_rules_cache_write_and_read() {
    let dir = tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let signature = mock_sha256(10);
    let metadata = CacheMetadata {
        project_path: Some("/mock/workspace".to_string()),
        provider: Some("mock-provider".to_string()),
        model: Some("mock-model".to_string()),
        sources: vec![SourceMeta {
            source: "project".to_string(),
            present: true,
            char_count: Some(50),
            sha256: Some(mock_sha256(20)),
        }],
    };
    let cache = RulesCache::new_in(cache_dir.clone(), signature.clone(), Some(metadata));
    let rules = JevRules {
        rules: vec![JevRule {
            text: "mock_rule_beta".to_string(),
            source: "project".to_string(),
        }],
        protected_paths: vec!["mock/protected/**".to_string()],
    };

    cache.write(&rules).unwrap();

    let single_file = cache_dir.join(format!("{signature}.json"));
    let index_file = cache_dir.join(format!("{signature}.index.json"));
    assert!(single_file.is_file(), "必须存在单文件缓存");
    assert!(!index_file.exists(), "不应产生冗余的 index.json");

    let bytes = fs::read(&single_file).unwrap();
    let cached: CachedRulesFile = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cached.version, 1);
    assert_eq!(cached.signature, signature);
    assert_eq!(cached.project_path.as_deref(), Some("/mock/workspace"));
    assert_eq!(cached.provider.as_deref(), Some("mock-provider"));
    assert_eq!(cached.model.as_deref(), Some("mock-model"));
    assert!(cached.created_at.is_some());
    assert_eq!(cached.rules.len(), 1);
    assert_eq!(cached.rules[0].text, "mock_rule_beta");

    let loaded = cache.read().unwrap();
    assert_eq!(loaded.rules.len(), 1);
    assert_eq!(loaded.rules[0].text, "mock_rule_beta");
    assert_eq!(loaded.protected_paths, vec!["mock/protected/**"]);
}

use std::fs;
use tempfile::tempdir;

use super::*;
use crate::hitl::jev::rules::{JevRule, JevRules};

fn mock_sha256(seed: u8) -> String {
    format!("{:02x}", seed).repeat(32)
}

#[test]
fn test_cache_index_backward_compatibility_without_metadata() {
    let sig = mock_sha256(1);
    let rules_id = mock_sha256(2);
    let legacy_json = format!(
        r#"{{
            "version": 1,
            "signature": "{sig}",
            "rules_id": "{rules_id}"
        }}"#
    );
    let index: CacheIndex = serde_json::from_str(&legacy_json).unwrap();
    assert_eq!(index.version, 1);
    assert_eq!(index.signature, sig);
    assert_eq!(index.rules_id, rules_id);
    assert!(index.created_at.is_none());
    assert!(index.project_path.is_none());
    assert!(index.provider.is_none());
    assert!(index.model.is_none());
    assert!(index.sources.is_empty());
}

#[test]
fn test_cache_index_roundtrip_with_metadata() {
    let index = CacheIndex {
        version: 1,
        signature: mock_sha256(10),
        rules_id: mock_sha256(20),
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        project_path: Some("/mock/project/path".to_string()),
        provider: Some("mock-provider".to_string()),
        model: Some("mock-model".to_string()),
        sources: vec![
            SourceMeta {
                source: "personal".to_string(),
                present: false,
                char_count: None,
                sha256: None,
            },
            SourceMeta {
                source: "project".to_string(),
                present: true,
                char_count: Some(128),
                sha256: Some(mock_sha256(30)),
            },
        ],
    };
    let json_bytes = serde_json::to_vec_pretty(&index).unwrap();
    let deserialized: CacheIndex = serde_json::from_slice(&json_bytes).unwrap();
    assert_eq!(index, deserialized);
}

#[test]
fn test_rules_cache_write_and_read_populates_index_metadata() {
    let dir = tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    let signature = mock_sha256(40);
    let metadata = CacheMetadata {
        project_path: Some("/mock/workspace".to_string()),
        provider: Some("mock-provider".to_string()),
        model: Some("mock-model".to_string()),
        sources: vec![SourceMeta {
            source: "project".to_string(),
            present: true,
            char_count: Some(64),
            sha256: Some(mock_sha256(50)),
        }],
    };
    let cache = RulesCache::new_in(cache_dir.clone(), signature.clone(), Some(metadata));
    let rules = JevRules {
        rules: vec![JevRule {
            text: "mock_rule_text_alpha".to_string(),
            source: "project".to_string(),
        }],
        protected_paths: vec!["mock/protected/**".to_string()],
    };
    cache.write(&rules).unwrap();
    let index_file = cache_dir.join(format!("{signature}.index.json"));
    let index_bytes = fs::read(&index_file).unwrap();
    let index: CacheIndex = serde_json::from_slice(&index_bytes).unwrap();
    assert_eq!(index.version, 1);
    assert_eq!(index.signature, signature);
    assert_eq!(index.project_path.as_deref(), Some("/mock/workspace"));
    assert_eq!(index.provider.as_deref(), Some("mock-provider"));
    assert_eq!(index.model.as_deref(), Some("mock-model"));
    assert!(index.created_at.is_some());
    assert_eq!(index.sources.len(), 1);
    assert_eq!(index.sources[0].source, "project");
    assert_eq!(index.sources[0].char_count, Some(64));
    assert_eq!(index.sources[0].sha256.as_deref(), Some(mock_sha256(50).as_str()));
    let loaded_rules = cache.read().unwrap();
    assert_eq!(loaded_rules.rules.len(), 1);
    assert_eq!(loaded_rules.rules[0].text, "mock_rule_text_alpha");
    assert_eq!(loaded_rules.protected_paths, vec!["mock/protected/**"]);
}

#[test]
fn test_rules_cache_load_legacy_index_without_metadata() {
    let dir = tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();
    let rules = JevRules {
        rules: vec![JevRule {
            text: "mock_rule_text_legacy".to_string(),
            source: "project".to_string(),
        }],
        protected_paths: vec![],
    };
    let rules_bytes = serde_json::to_vec_pretty(&rules).unwrap();
    let rules_id = digest(&rules_bytes);
    fs::write(cache_dir.join(format!("{rules_id}.json")), &rules_bytes).unwrap();
    let signature = mock_sha256(60);
    let legacy_index_json = format!(
        r#"{{
            "version": 1,
            "signature": "{signature}",
            "rules_id": "{rules_id}"
        }}"#
    );
    fs::write(
        cache_dir.join(format!("{signature}.index.json")),
        legacy_index_json.as_bytes(),
    )
    .unwrap();
    let cache = RulesCache::new_in(cache_dir, signature, None);
    let loaded = cache.read().unwrap();
    assert_eq!(loaded.rules.len(), 1);
    assert_eq!(loaded.rules[0].text, "mock_rule_text_legacy");
}

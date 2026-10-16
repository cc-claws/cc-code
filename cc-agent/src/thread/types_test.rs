#[test]
fn test_thread_meta_default_values() {
    // new() 创建的根线程应具有正确的默认值
    let meta = ThreadMeta::new("/tmp/test");
    assert_eq!(meta.parent_thread_id, None);
    assert_eq!(meta.snapshot_at_message_id, None);
    assert!(!meta.hidden);
    assert_eq!(meta.cancel_policy, "cascade");
    assert_eq!(meta.config, None);
    assert_eq!(meta.cached_context, None);
    assert_eq!(meta.agent_status, "active");
    assert_eq!(meta.latest_recap, None);
    assert_eq!(meta.last_task_summary, None);
    assert_eq!(meta.model, None);
    assert_eq!(meta.reasoning_effort, None);
    assert_eq!(meta.branch, None);
    assert!(meta.is_root());
}

#[test]
fn test_thread_meta_is_root() {
    // parent_thread_id 为 None 时是根 agent
    let mut meta = ThreadMeta::new("/tmp/test");
    assert!(meta.is_root());

    // 设置 parent_thread_id 后不是根 agent
    meta.parent_thread_id = Some("parent-uuid".to_string());
    assert!(!meta.is_root());
}

#[test]
fn test_thread_meta_deserialize_defaults() {
    // 反序列化旧格式 JSON 时新字段应使用默认值
    let json = r#"{"id":"test-id","title":null,"cwd":"/tmp","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","message_count":0,"content_size":0}"#;
    let meta: ThreadMeta = serde_json::from_str(json).unwrap();
    assert_eq!(meta.parent_thread_id, None);
    assert_eq!(meta.snapshot_at_message_id, None);
    assert!(!meta.hidden);
    assert_eq!(meta.cancel_policy, "cascade");
    assert_eq!(meta.config, None);
    assert_eq!(meta.cached_context, None);
    assert_eq!(meta.agent_status, "active");
    assert_eq!(meta.latest_recap, None);
    assert_eq!(meta.last_task_summary, None);
    assert_eq!(meta.model, None);
    assert_eq!(meta.reasoning_effort, None);
    assert_eq!(meta.branch, None);
}

#[test]
fn test_task_summary_roundtrip() {
    // Arrange
    let summary = TaskSummary {
        verb: "Cooked".to_string(),
        elapsed_ms: 25_000,
        done_at: Utc::now(),
    };

    // Act
    let json = serde_json::to_string(&summary).unwrap();
    let back: TaskSummary = serde_json::from_str(&json).unwrap();

    // Assert
    assert_eq!(back, summary);
}

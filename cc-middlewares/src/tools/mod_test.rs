use super::*;

#[tokio::test]
async fn test_tool_wrappers_preserve_write_file_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrapped.txt");
    std::fs::write(&path, "包装前的旧内容\n").unwrap();
    let input = serde_json::json!({
        "file_path": "wrapped.txt",
        "content": "子 Agent 写入的新内容\n"
    });
    let expected = Some(FileSnapshot {
        old_content: Some("包装前的旧内容\n".to_string()),
    });
    let boxed = BoxToolWrapper(Box::new(WriteFileTool::new(dir.path().to_string_lossy())));
    assert_eq!(
        boxed.file_snapshot(&input),
        expected,
        "Box 包装应透传文件快照"
    );
    let shared = ArcToolWrapper(Arc::new(boxed));
    assert_eq!(
        shared.file_snapshot(&input),
        expected,
        "子 Agent 的 Arc 包装应透传文件快照"
    );
    let result = shared.invoke(input).await;
    assert!(result.is_ok(), "包装后的 Write 应正常执行: {result:?}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("应读取实际写入内容"),
        "子 Agent 写入的新内容\n",
        "快照透传不能影响真实 Write 执行"
    );
}

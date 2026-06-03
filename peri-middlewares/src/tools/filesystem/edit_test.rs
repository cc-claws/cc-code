    #[tokio::test]
    async fn test_edit_file_single_replace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello foo world").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(
                serde_json::json!({"file_path": "f.txt", "old_string": "foo", "new_string": "bar"}),
            )
            .await
            .unwrap();
        // "foo" → "bar": same line count, one occurrence
        assert!(
            result.contains("Replaced text"),
            "unexpected: {result}"
        );
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "hello bar world");
    }

    #[tokio::test]
    async fn test_edit_file_old_string_not_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello world").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "missing", "new_string": "x"}))
            .await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "should report not found: {err}"
        );
    }

    #[tokio::test]
    async fn test_edit_file_replace_all() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x x x").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({
            "file_path": "f.txt",
            "old_string": "x",
            "new_string": "y",
            "replace_all": true
        }))
        .await
        .unwrap();
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "y y y");
    }

    #[tokio::test]
    async fn test_edit_file_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "foo and foo").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(
                serde_json::json!({"file_path": "f.txt", "old_string": "foo", "new_string": "bar"}),
            )
            .await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not unique"),
            "should report ambiguity: {err}"
        );
    }

    #[tokio::test]
    async fn test_edit_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(
                serde_json::json!({"file_path": "ghost.txt", "old_string": "x", "new_string": "y"}),
            )
            .await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("File not found"),
            "should report file not found: {err}"
        );
    }

    #[tokio::test]
    async fn test_edit_file_empty_old_string_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello world").unwrap();
        let tool = EditFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "f.txt", "old_string": "", "new_string": "x", "replace_all": true}))
            .await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("cannot be empty"),
            "empty old_string should be rejected: {err}"
        );
        // 文件内容不应被修改
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "hello world", "file should not be modified");
    }

    #[test]
    fn test_description_extended() {
        let tool = EditFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("old_string"), "description 应提及 old_string");
        assert!(
            desc.contains("replace_all"),
            "description 应提及 replace_all"
        );
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_tool_name_is_Edit() {
        let tool = EditFileTool::new("/tmp");
        assert_eq!(tool.name(), "Edit");
    }

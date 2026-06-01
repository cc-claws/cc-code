    #[tokio::test]
    async fn test_write_file_creates_new() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "new.txt", "content": "hello"}))
            .await
            .unwrap();
        let content = std::fs::read_to_string(dir.path().join("new.txt")).unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_write_file_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "old").unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "f.txt", "content": "new"}))
            .await
            .unwrap();
        let content = std::fs::read_to_string(dir.path().join("f.txt")).unwrap();
        assert_eq!(content, "new");
    }

    #[tokio::test]
    async fn test_write_file_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "sub/dir/file.txt", "content": "deep"}))
            .await
            .unwrap();
        assert!(dir.path().join("sub/dir/file.txt").exists());
    }

    #[tokio::test]
    async fn test_write_file_missing_content_param() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        let result = tool.invoke(serde_json::json!({"file_path": "f.txt"})).await;
        assert!(result.is_err(), "missing content should return Err");
    }

    #[tokio::test]
    async fn test_write_file_success_message() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "msg.txt", "content": "x"}))
            .await
            .unwrap();
        assert!(
            result.contains("Wrote 1 line"),
            "unexpected message: {result}"
        );
    }

    #[tokio::test]
    async fn test_write_file_multiline_message() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "multi.txt", "content": "a\nb\nc"}))
            .await
            .unwrap();
        assert!(
            result.contains("Wrote 3 lines"),
            "unexpected message: {result}"
        );
    }

    #[tokio::test]
    async fn test_write_file_no_tmp_residual() {
        let dir = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(dir.path().to_str().unwrap());
        tool.invoke(serde_json::json!({"file_path": "clean.txt", "content": "data"}))
            .await
            .unwrap();
        // 原子写入后不应残留任何 .tmp.* 临时文件
        let tmp_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("clean.tmp."))
            .collect();
        assert!(tmp_files.is_empty(), "临时文件应在 rename 后被清除");
        assert!(dir.path().join("clean.txt").exists());
    }

    #[tokio::test]
    async fn test_write_file_error_propagates() {
        let dir = tempfile::tempdir().unwrap();
        // 在只读目录上写入应返回 Err
        let readonly_dir = dir.path().join("readonly");
        std::fs::create_dir(&readonly_dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&readonly_dir, std::fs::Permissions::from_mode(0o444))
                .unwrap();
        }
        let tool = WriteFileTool::new(readonly_dir.to_str().unwrap());
        let _result = tool
            .invoke(serde_json::json!({"file_path": "sub/nope.txt", "content": "x"}))
            .await;
        #[cfg(unix)]
        assert!(_result.is_err(), "写入只读目录应返回 Err");
    }

    #[test]
    fn test_description_extended() {
        let tool = WriteFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("atomic write"), "description 应提及原子写入");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_tool_name_is_Write() {
        let tool = WriteFileTool::new("/tmp");
        assert_eq!(tool.name(), "Write");
    }

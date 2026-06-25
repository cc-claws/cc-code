    #[tokio::test]
    async fn test_glob_match_simple() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "*.rs"}))
            .await
            .unwrap();
        assert!(result.contains("a.rs"), "should find a.rs: {result}");
        assert!(result.contains("b.rs"), "should find b.rs: {result}");
        assert!(!result.contains("c.txt"), "should not find c.txt: {result}");
    }

    #[tokio::test]
    async fn test_glob_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "*.go"}))
            .await
            .unwrap();
        assert_eq!(result, "No files found.");
    }

    #[tokio::test]
    async fn test_glob_recursive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/d.rs"), "").unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "**/*.rs"}))
            .await
            .unwrap();
        assert!(result.contains("d.rs"), "should find nested d.rs: {result}");
    }

    #[tokio::test]
    async fn test_glob_dir_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "*.rs", "path": "nonexistent_dir"}))
            .await;
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Directory not found"),
            "should report missing dir: {err_msg}"
        );
    }

    #[test]
    fn test_description_extended() {
        let tool = GlobFilesTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(
            desc.contains("modification time"),
            "description 应提及排序规则"
        );
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_tool_name_is_Glob() {
        let tool = GlobFilesTool::new("/tmp");
        assert_eq!(tool.name(), "Glob");
    }

    #[tokio::test]
    async fn test_glob_truncation_persists_full_output() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..1001 {
            std::fs::write(dir.path().join(format!("file_{:04}.rs", i)), "").unwrap();
        }
        let tool = GlobFilesTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"pattern": "*.rs"}))
            .await
            .unwrap();
        assert!(
            result.contains("Output truncated"),
            "应显示截断信息: {result}"
        );
        assert!(
            result.contains("Read tool"),
            "应包含 Read tool 提示: {result}"
        );
        assert!(
            result.contains("peri-tool-output-"),
            "应包含文件路径: {result}"
        );
    }

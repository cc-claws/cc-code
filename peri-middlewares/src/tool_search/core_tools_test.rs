    #[test]
    fn test_core_tool_not_deferred() {
        assert!(!is_deferred_tool("Read"));
    }

    #[test]
    fn test_meta_tool_not_deferred() {
        assert!(!is_deferred_tool("SearchExtraTools"));
        assert!(!is_deferred_tool("ExecuteExtraTool"));
    }

    #[test]
    fn test_deferred_tool() {
        assert!(is_deferred_tool("CronRegister"));
        assert!(is_deferred_tool("CronList"));
        assert!(is_deferred_tool("CronRemove"));
    }

    #[test]
    fn test_mcp_tool_deferred() {
        assert!(is_deferred_tool("mcp__slack__send_message"));
        assert!(is_deferred_tool("mcp__read_resource"));
    }

    #[test]
    fn test_unknown_tool_deferred() {
        assert!(is_deferred_tool("UnknownTool"));
        assert!(is_deferred_tool(""));
    }

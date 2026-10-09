    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_tool_use(name: &str, input: serde_json::Value) -> ContentBlock {
        ContentBlock::tool_use("call_1", name, input)
    }

    fn make_tool_result(body: &str, is_error: bool) -> ContentBlock {
        ContentBlock::tool_result("call_1", vec![ContentBlock::text(body)], is_error)
    }

    /// 构造一条 AI 消息，内含单个 ToolUse block。
    fn make_ai_message_with_tool_use(name: &str, input: serde_json::Value) -> BaseMessage {
        BaseMessage::ai_from_blocks(vec![make_tool_use(name, input)])
    }

    /// 构造一条 Tool 消息，内含单个 ToolResult block（is_error 记录在 block 上）。
    fn make_tool_message_with_result(body: &str, is_error: bool) -> BaseMessage {
        let blocks = cc_agent::messages::MessageContent::blocks(vec![make_tool_result(body, is_error)]);
        BaseMessage::tool_result("call_1", blocks)
    }

    // ── 既有测试（迁移） ──────────────────────────────────────────────────────

    #[test]
    fn test_render_plain_text_skips_system_messages() {
        let messages = vec![
            BaseMessage::system("You are helpful"),
            BaseMessage::human("hello"),
            BaseMessage::ai("hi there"),
        ];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(!text.contains("You are helpful"), "应跳过 System 消息");
        assert!(text.contains("hello"), "应包含 Human 消息");
        assert!(text.contains("hi there"), "应包含 Ai 消息");
    }

    #[test]
    fn test_render_plain_text_user_assistant_labels() {
        let messages = vec![BaseMessage::human("question"), BaseMessage::ai("answer")];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(text.contains("=== User ==="), "应有 User 标签");
        assert!(text.contains("=== Assistant ==="), "应有 Assistant 标签");
    }

    #[test]
    fn test_render_markdown_contains_frontmatter() {
        let messages = vec![BaseMessage::human("test")];
        let md = render_messages(&messages, ExportFormat::Markdown);
        assert!(md.starts_with("---"), "Markdown 应以 frontmatter 开头");
        assert!(md.contains("# Conversation Export"), "应有标题");
        assert!(md.contains("## User"), "应有 User heading");
    }

    #[test]
    fn test_render_json_is_valid_json() {
        let messages = vec![BaseMessage::human("test")];
        let json = render_messages(&messages, ExportFormat::Json);
        assert!(
            serde_json::from_str::<serde_json::Value>(&json).is_ok(),
            "应为合法 JSON"
        );
    }

    #[test]
    fn test_render_plain_text_contains_content() {
        let messages = vec![BaseMessage::human("read file")];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(text.contains("read file"), "应包含消息内容");
    }

    #[test]
    fn test_render_markdown_separates_turns() {
        let messages = vec![
            BaseMessage::human("q1"),
            BaseMessage::ai("a1"),
            BaseMessage::human("q2"),
        ];
        let md = render_messages(&messages, ExportFormat::Markdown);
        assert!(md.contains("---"), "应有 turn 分隔线");
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::PlainText.extension(), "txt");
        assert_eq!(ExportFormat::Markdown.extension(), "md");
        assert_eq!(ExportFormat::Json.extension(), "json");
    }

    // ── 新增：完整保留工具调用信息 ────────────────────────────────────────────

    #[test]
    fn test_render_markdown_keeps_full_tool_use_input() {
        let long_command = "echo ".to_string() + &"a".repeat(300);
        let messages = vec![make_ai_message_with_tool_use(
            "Bash",
            serde_json::json!({ "command": long_command }),
        )];
        let md = render_messages(&messages, ExportFormat::Markdown);
        assert!(
            md.contains(&long_command),
            "Markdown 应完整保留超过 200 字符的工具参数"
        );
    }

    #[test]
    fn test_render_markdown_includes_tool_result_body() {
        let body = "line one\nline two\nline three";
        let messages = vec![make_tool_message_with_result(body, false)];
        let md = render_messages(&messages, ExportFormat::Markdown);
        assert!(md.contains(body), "Markdown 应包含工具结果完整正文");
        assert!(
            !md.contains("(3 lines)"),
            "不应再退化为仅行数统计的 `(N lines)`"
        );
    }

    #[test]
    fn test_render_markdown_tool_result_error_marker() {
        let messages = vec![make_tool_message_with_result("boom", true)];
        let md = render_messages(&messages, ExportFormat::Markdown);
        assert!(md.contains("Tool Result (error)"), "错误结果应有 (error) 标记");
    }

    #[test]
    fn test_render_plain_text_keeps_full_tool_use_input() {
        let long_command = "echo ".to_string() + &"b".repeat(300);
        let messages = vec![make_ai_message_with_tool_use(
            "Bash",
            serde_json::json!({ "command": long_command }),
        )];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(text.contains(&long_command), "PlainText 应完整保留工具参数");
    }

    #[test]
    fn test_render_plain_text_includes_tool_result_body() {
        let body = "alpha\nbeta";
        let messages = vec![make_tool_message_with_result(body, false)];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(text.contains(body), "PlainText 应包含工具结果完整正文");
    }

    #[test]
    fn test_render_markdown_tool_use_input_with_backticks() {
        // 命令内含连续反引号（JSON 不转义反引号，故字面保留）
        let script = "echo ```nested``` end";
        let messages = vec![make_ai_message_with_tool_use(
            "Bash",
            serde_json::json!({ "command": script }),
        )];
        let md = render_messages(&messages, ExportFormat::Markdown);
        // 围栏必须长于正文中最长的连续反引号（3 个 → 围栏 4 个）
        assert!(
            md.contains("\n````json\n"),
            "正文含 ``` 时围栏应升级为 4 个反引号"
        );
        assert!(md.contains(script), "脚本内容应完整保留");
    }

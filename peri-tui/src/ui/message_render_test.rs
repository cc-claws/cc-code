    fn make_agent(id: &str, task: &str, tools: usize, error: bool) -> AgentSummary {
        AgentSummary {
            agent_id: id.to_string(),
            task_preview: task.to_string(),
            tool_count: tools,
            is_error: error,
            final_result: if error {
                Some("failed".to_string())
            } else {
                Some("done".to_string())
            },
        }
    }

    fn rendered_text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn test_render_batch_summary_collapsed() {
        let agents = vec![
            make_agent("agent-1", "task one", 3, false),
            make_agent("agent-2", "task two", 5, false),
            make_agent("agent-3", "task three", 0, false),
        ];
        let lines = render_batch_summary(&agents, &true);
        // Header + 3 行 agent 摘要 = 4 行
        assert_eq!(lines.len(), 4, "折叠态应有 header + 3 行摘要");
        // Header 应包含 "3 agents finished"
        let header_text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
        assert!(
            header_text.contains("3 agents finished"),
            "header 应显示 agent 数量: {}",
            header_text
        );
    }

    #[test]
    fn test_render_batch_summary_expanded() {
        let agents = vec![
            make_agent("agent-1", "task one", 3, false),
            make_agent("agent-2", "task two", 5, false),
        ];
        let lines = render_batch_summary(&agents, &false);
        // Header + 2 * (task_preview + final_result) = 5 行
        assert_eq!(lines.len(), 5, "展开态应有 header + 2*(task+result)");
    }

    #[test]
    fn test_render_batch_summary_with_error() {
        let agents = vec![
            make_agent("agent-1", "task one", 3, false),
            make_agent("agent-2", "task two", 1, true),
            make_agent("agent-3", "task three", 2, true),
        ];
        let lines = render_batch_summary(&agents, &true);
        let header_text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
        assert!(
            header_text.contains("2 failed"),
            "header 应显示失败数: {}",
            header_text
        );
    }

    #[test]
    fn test_render_batch_summary_tree_connectors() {
        let agents = vec![
            make_agent("agent-1", "task one", 3, false),
            make_agent("agent-2", "task two", 5, false),
            make_agent("agent-3", "task three", 0, false),
        ];
        let lines = render_batch_summary(&agents, &true);
        // 第一个 agent 应使用 ├─
        let line1_text: String = lines[1].spans.iter().map(|s| s.content.clone()).collect();
        assert!(
            line1_text.contains("├─"),
            "非最后一个 agent 应使用 ├─: {}",
            line1_text
        );
        // 最后一个 agent 应使用 └─
        let line3_text: String = lines[3].spans.iter().map(|s| s.content.clone()).collect();
        assert!(
            line3_text.contains("└─"),
            "最后一个 agent 应使用 └─: {}",
            line3_text
        );
    }

    #[test]
    fn test_render_single_agent_unchanged() {
        // batch_agents 为空时走现有渲染路径，不经过 render_batch_summary
        // 此测试验证 render_batch_summary 对空 agents 列表的边界行为
        let agents: Vec<AgentSummary> = vec![];
        let lines = render_batch_summary(&agents, &true);
        assert_eq!(lines.len(), 1, "空 agents 应只有 header");
        let header_text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
        assert!(
            header_text.contains("0 agents"),
            "header 应包含 0 agents: {}",
            header_text
        );
    }

    // ─── 从 headless_test.rs 迁移的 render_view_model 测试 ──────────────────

    #[test]
    fn test_system_note_error_detection() {
        let error_content = "Compact failed: No LLM Provider";
        assert!(
            error_content.contains("failed") || error_content.contains("Compact failed"),
            "应检测到错误标记"
        );
        let warn_content = "⚠ Interrupted";
        assert!(warn_content.contains("⚠"), "应检测到警告标记");
        let info_content = "Configuration saved";
        assert!(
            !info_content.contains("❌")
                && !info_content.contains("failed")
                && !info_content.contains("⚠"),
            "普通消息不应被标记为错误"
        );
    }

    #[test]
    fn test_shell_command_render_header_and_truncation() {
        let stdout = (0..60)
            .map(|idx| format!("line {idx:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut vm = MessageViewModel::ShellCommand {
            id: "shell-1".to_string(),
            command: "git status".to_string(),
            cwd: ".".to_string(),
            stdin: Vec::new(),
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
            collapsed: true,
            content_hash: 0,
        };
        vm.recompute_hash();

        let lines = render_view_model(&vm, None, 80, false, 0);
        let full_text = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.clone()))
            .collect::<Vec<_>>()
            .join("");

        assert!(full_text.contains("> !git status"), "应显示带 ! 前缀的命令");
        assert!(
            full_text.contains("Ctrl+O for details"),
            "超长输出应显示 Ctrl+O 详细模式提示"
        );
        assert!(full_text.contains("line 05"), "普通模式应展示前 6 行");
        assert!(!full_text.contains("line 06"), "普通模式应截断第 7 行后的输出");

        let detail_lines = render_view_model(&vm, None, 80, true, 0);
        let detail_text = detail_lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.clone()))
            .collect::<Vec<_>>()
            .join("");
        assert!(detail_text.contains("line 39"), "详细模式应显示前 40 行");
        assert!(
            !detail_text.contains("line 40"),
            "详细模式也应在 40 行后截断"
        );
        assert!(
            detail_text.contains("output truncated at 40 lines"),
            "详细模式截断后应提示硬上限"
        );
    }

    #[test]
    fn test_shell_command_render_preserves_basic_ansi_color() {
        let mut vm = MessageViewModel::ShellCommand {
            id: "shell-2".to_string(),
            command: "echo color".to_string(),
            cwd: ".".to_string(),
            stdin: Vec::new(),
            stdout: "\x1b[31mred\x1b[0m".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            collapsed: true,
            content_hash: 0,
        };
        vm.recompute_hash();

        let lines = render_view_model(&vm, None, 80, false, 0);
        let has_red_span = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.as_ref() == "red" && span.style.fg == Some(Color::Red)
        });

        assert!(has_red_span, "ANSI 31m 应渲染为红色 span");
    }

    #[test]
    fn test_shell_command_stderr_exit0_uses_muted_not_error() {
        // exit 0 + stderr → MUTED（成功，stderr 不应显示红色）
        let mut vm_ok = MessageViewModel::ShellCommand {
            id: "shell-stderr-ok".to_string(),
            command: "git status".to_string(),
            cwd: ".".to_string(),
            stdin: Vec::new(),
            stdout: String::new(),
            stderr: "warning: something".to_string(),
            exit_code: Some(0),
            collapsed: true,
            content_hash: 0,
        };
        vm_ok.recompute_hash();
        let lines_ok = render_view_model(&vm_ok, None, 80, false, 0);
        let stderr_span_ok = lines_ok
            .iter()
            .flat_map(|l| &l.spans)
            .find(|s| s.content.contains("warning: something"));
        assert!(stderr_span_ok.is_some(), "应找到 stderr 内容");
        assert_eq!(
            stderr_span_ok.unwrap().style.fg,
            Some(crate::ui::theme::MUTED),
            "exit 0 时 stderr 应为 MUTED 色，非 ERROR 红色"
        );
        // exit 1 + stderr → ERROR（失败，stderr 应显示红色）
        let mut vm_err = MessageViewModel::ShellCommand {
            id: "shell-stderr-err".to_string(),
            command: "bad_cmd".to_string(),
            cwd: ".".to_string(),
            stdin: Vec::new(),
            stdout: String::new(),
            stderr: "command not found".to_string(),
            exit_code: Some(1),
            collapsed: true,
            content_hash: 0,
        };
        vm_err.recompute_hash();
        let lines_err = render_view_model(&vm_err, None, 80, false, 0);
        let stderr_span_err = lines_err
            .iter()
            .flat_map(|l| &l.spans)
            .find(|s| s.content.contains("command not found"));
        assert!(stderr_span_err.is_some(), "应找到 stderr 内容");
        assert_eq!(
            stderr_span_err.unwrap().style.fg,
            Some(crate::ui::theme::ERROR),
            "exit 非 0 时 stderr 应为 ERROR 红色"
        );
    }

    #[test]
    fn test_tool_block_error_visible_when_collapsed() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_err".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("bad_command".to_string()),
            content: "command not found: bad_command\nexit code 127".to_string(),
            is_error: true,
            collapsed: true,
            color: crate::ui::theme::ERROR,
            diff_input: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        assert!(
            lines.len() >= 3,
            "collapsed error ToolBlock should have header + error lines, got {}",
            lines.len()
        );
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");
        assert!(
            text.contains("command not found"),
            "error content should be visible: {}",
            text
        );
    }

    #[test]
    fn test_tool_block_read_collapsed_shows_summary() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Read".to_string(),
            tool_call_id: "tc_ok".to_string(),
            display_name: "Read".to_string(),
            args_display: Some("file.txt".to_string()),
            content: "file contents here".to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let text = rendered_text(&lines);
        assert!(text.contains("Read(file.txt)"), "Read header 应显示参数: {text}");
        assert!(text.contains("Read 1 lines"), "Read 折叠态应显示行数摘要: {text}");
    }

    #[test]
    fn test_tool_block_detail_mode_includes_diff_lines() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Edit".to_string(),
            tool_call_id: "tc_diff".to_string(),
            display_name: "Edit".to_string(),
            args_display: Some("file.rs".to_string()),
            content: "edited file.rs".to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: Some(peri_widgets::DiffInput {
                file_path: "file.rs".to_string(),
                old_content: "old line".to_string(),
                new_content: "new line".to_string(),
                is_new_file: false,
                is_deleted_file: false,
                is_binary: false,
            }),
            content_hash: 0,
        };

        let normal_text = render_view_model(&vm, Some(1), 80, false, 0)
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");
        let detail_text = render_view_model(&vm, Some(1), 80, true, 0)
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");

        assert!(
            !normal_text.contains("new line"),
            "普通模式不应显示内嵌 diff"
        );
        assert!(
            detail_text.contains("new line"),
            "详细模式应按当前 width 渲染 diff_input"
        );
        assert!(
            detail_text.contains("  ⎿ "),
            "diff 行应带缩进前缀 `  ⎿ `，实际: {}",
            detail_text
        );
    }

    #[test]
    fn test_tool_block_detail_mode_diff_respects_terminal_width() {
        // 验证 issue 2026-06-24-diff-render-width-hardcoded-80 的核心修复：
        // 同一 diff_input 在不同终端 width 下应产生不同的渲染输出。
        // 旧实现预渲染 width=80 缓存到 VM，width 参数无效；新实现按当前 width 渲染。
        use crate::app::MessageViewModel;
        let long_line = "fn really_long_function_name(argument_one: i32, argument_two: String) -> Result<Vec<String>, Box<dyn std::error::Error>>";
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Edit".to_string(),
            tool_call_id: "tc_width".to_string(),
            display_name: "Edit".to_string(),
            args_display: Some("file.rs".to_string()),
            content: "edited".to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: Some(peri_widgets::DiffInput {
                file_path: "file.rs".to_string(),
                old_content: String::new(),
                new_content: long_line.to_string(),
                is_new_file: true,
                is_deleted_file: false,
                is_binary: false,
            }),
            content_hash: 0,
        };

        let wide_lines = render_view_model(&vm, Some(1), 200, true, 0);
        let narrow_lines = render_view_model(&vm, Some(1), 40, true, 0);

        let wide_text: String = wide_lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        let narrow_text: String = narrow_lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        // wide 渲染下整行内容应完整出现（参数名 argument_one 保留）
        assert!(
            wide_text.contains("argument_one"),
            "宽屏应完整渲染长行（包含参数 argument_one），实际: {}",
            wide_text
        );
        // narrow 渲染下长行被 truncate_to_width 截断，argument_one 不出现
        assert!(
            !narrow_text.contains("argument_one"),
            "窄屏应截断长行（不再硬编码 80），实际: {}",
            narrow_text
        );
    }

    #[test]
    fn test_tool_block_detail_mode_shows_full_long_output() {
        use crate::app::MessageViewModel;
        let content = (0..30)
            .map(|idx| format!("line {idx:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_long".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("printf long output".to_string()),
            content,
            is_error: false,
            collapsed: false,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            content_hash: 0,
        };

        let normal_text = render_view_model(&vm, Some(1), 80, false, 0)
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");
        let detail_text = render_view_model(&vm, Some(1), 80, true, 0)
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");

        assert!(normal_text.contains("line 19"), "普通模式应显示前 20 行");
        assert!(
            !normal_text.contains("line 20"),
            "普通模式应截断第 21 行后的输出"
        );
        assert!(
            normal_text.contains("... (10 more lines)"),
            "普通模式应显示隐藏行数提示"
        );
        assert!(detail_text.contains("line 29"), "详细模式应显示完整工具输出");
        assert!(
            !detail_text.contains("more lines"),
            "详细模式不应显示截断提示"
        );
    }

    #[test]
    fn test_tool_call_group_error_visible_when_collapsed() {
        use crate::app::MessageViewModel;
        use crate::ui::message_view::{ToolCategory, ToolEntry};

        let vm = MessageViewModel::ToolCallGroup {
            category: ToolCategory::Read,
            tools: vec![
                ToolEntry {
                    tool_name: "Read".to_string(),
                    display_name: "Read".to_string(),
                    args_display: Some("ok_file.txt".to_string()),
                    content: "ok content".to_string(),
                    is_error: false,
                },
                ToolEntry {
                    tool_name: "Read".to_string(),
                    display_name: "Read".to_string(),
                    args_display: Some("missing.txt".to_string()),
                    content: "Error: file not found".to_string(),
                    is_error: true,
                },
            ],
            collapsed: true,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");
        assert!(
            text.contains("Error: file not found"),
            "error from failed tool should be visible: {}",
            text
        );
        assert!(
            !text.contains("ok content"),
            "successful tool content should NOT be visible: {}",
            text
        );
    }

    #[test]
    fn test_tool_call_group_detail_mode_shows_full_content() {
        use crate::app::MessageViewModel;
        use crate::ui::message_view::{ToolCategory, ToolEntry};
        let long_content = format!("{}tail-marker", "a".repeat(220));
        let vm = MessageViewModel::ToolCallGroup {
            category: ToolCategory::Search,
            tools: vec![ToolEntry {
                tool_name: "Grep".to_string(),
                display_name: "Grep".to_string(),
                args_display: Some("needle".to_string()),
                content: long_content,
                is_error: false,
            }],
            collapsed: true,
            content_hash: 0,
        };

        let detail_text = render_view_model(&vm, Some(1), 80, true, 0)
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");

        assert!(
            detail_text.contains("tail-marker"),
            "聚合工具组详细模式应显示完整内容"
        );
    }

    #[test]
    fn test_tool_call_group_detail_mode_shows_read_args_and_summary() {
        use crate::app::MessageViewModel;
        use crate::ui::message_view::{ToolCategory, ToolEntry};
        let vm = MessageViewModel::ToolCallGroup {
            category: ToolCategory::Read,
            tools: vec![ToolEntry {
                tool_name: "Read".to_string(),
                display_name: "Read".to_string(),
                args_display: Some("D:\\code\\smart-select-product-php\\public\\index.php".to_string()),
                content: "Read 61 lines".to_string(),
                is_error: false,
            }],
            collapsed: true,
            content_hash: 0,
        };

        let detail_text = rendered_text(&render_view_model(&vm, Some(1), 80, true, 0));

        assert!(
            detail_text.contains("Read(D:\\code\\smart-select-product-php\\public\\index.php)"),
            "Read 详细模式标题应显示文件路径: {detail_text}"
        );
        assert!(
            detail_text.contains("Read 61 lines"),
            "Read 详细模式应沿用工具摘要，不应误算成内容行数: {detail_text}"
        );
    }

    #[test]
    fn test_tool_call_group_detail_mode_shows_glob_args_and_found_summary() {
        use crate::app::MessageViewModel;
        use crate::ui::message_view::{ToolCategory, ToolEntry};
        let vm = MessageViewModel::ToolCallGroup {
            category: ToolCategory::Glob,
            tools: vec![ToolEntry {
                tool_name: "Glob".to_string(),
                display_name: "Glob".to_string(),
                args_display: Some("app/admin/controller/**/*.php".to_string()),
                content: "app\\admin\\control2\napp\\admin\\control3\napp\\admin\\control4".to_string(),
                is_error: false,
            }],
            collapsed: true,
            content_hash: 0,
        };

        let detail_text = rendered_text(&render_view_model(&vm, Some(1), 80, true, 0));

        assert!(
            detail_text.contains("Glob(pattern: \"app/admin/controller/**/*.php\")"),
            "Glob 详细模式标题应显示 pattern 参数: {detail_text}"
        );
        assert!(
            detail_text.contains("Found 3 files"),
            "Glob 详细模式应显示结果数量摘要: {detail_text}"
        );
        assert!(
            detail_text.contains("app\\admin\\control2"),
            "Glob 详细模式仍应显示匹配文件: {detail_text}"
        );
    }

    #[test]
    fn test_subagent_group_error_red_title_and_summary() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::SubAgentGroup {
            agent_id: "test-agent".to_string(),
            task_preview: "do something risky".to_string(),
            total_steps: 3,
            recent_messages: Vec::new(),
            is_running: false,
            collapsed: true,
            final_result: Some("Agent failed: permission denied".to_string()),
            is_error: true,
            is_background: false,
            bg_hash: Some("abc123".to_string()),
            batch_agents: Vec::new(),
            instance_id: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let title_color = lines
            .first()
            .and_then(|l| l.spans.get(1).and_then(|s| s.style.fg));
        assert_eq!(
            title_color,
            Some(crate::ui::theme::ERROR),
            "title should be red on error"
        );
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join("");
        assert!(
            text.contains("Agent failed"),
            "error summary should be visible: {}",
            text
        );
    }

    #[test]
    fn test_render_system_reminder_user_bubble() {
        let mut vm = MessageViewModel::user("irrelevant content".to_string());
        if let MessageViewModel::UserBubble { system_reminder, .. } = &mut vm {
            *system_reminder = true;
        }
        vm.recompute_hash();
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        assert_eq!(lines.len(), 1, "系统提醒应只渲染一行");
        let text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
        assert!(text.contains("上下文已压缩"), "应显示压缩提示文字，实际: {}", text);
    }

    #[test]
    fn test_render_normal_user_bubble_unchanged() {
        let vm = MessageViewModel::user("Hello World".to_string());
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let first_text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
        assert!(first_text.contains("\u{276f}"), "普通消息应有 ❯ 前缀");
        assert!(first_text.contains("Hello"), "应包含原始内容");
    }

    #[test]
    fn test_parse_exit_code_非零退出码() {
        assert_eq!(parse_exit_code("[Exit code: 1]"), Some(1));
        assert_eq!(parse_exit_code("[Exit code: 42]"), Some(42));
        assert_eq!(parse_exit_code("[Exit code: 127]"), Some(127));
        assert_eq!(parse_exit_code("[Exit code: -1]"), Some(-1));
    }

    #[test]
    fn test_parse_exit_code_零退出码() {
        assert_eq!(parse_exit_code("[Exit code: 0]"), Some(0));
    }

    #[test]
    fn test_parse_exit_code_空输出格式() {
        assert_eq!(
            parse_exit_code("[Command completed with exit code 1]"),
            Some(1)
        );
        assert_eq!(
            parse_exit_code("[Command completed with exit code 0]"),
            Some(0)
        );
    }

    #[test]
    fn test_parse_exit_code_混合内容() {
        let content = "hello world\n[stderr]\nsome error\n[Exit code: 1]";
        assert_eq!(parse_exit_code(content), Some(1));
    }

    #[test]
    fn test_parse_exit_code_无退出码() {
        assert_eq!(parse_exit_code("just some output"), None);
        assert_eq!(parse_exit_code(""), None);
    }

    #[test]
    fn test_bash_toolblock_nonzero_exit_shows_failed() {
        use crate::app::MessageViewModel;
        // Bash 工具，is_error=false 但输出包含非零 exit code
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_bash_fail".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("git add missing.txt".to_string()),
            content: "[stderr]\nfatal: pathspec 'missing.txt' did not match any files\n[Exit code: 128]"
                .to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::BASH_BORDER,
            diff_input: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let header = &lines[0];
        // 指示器应为红色（Failed）
        let indicator_color = header.spans.first().and_then(|s| s.style.fg);
        assert_eq!(
            indicator_color,
            Some(crate::ui::theme::ERROR),
            "非零 exit code 的 Bash 工具 ● 应为红色"
        );
    }

    #[test]
    fn test_bash_toolblock_zero_exit_stays_green() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_bash_ok".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("echo hello".to_string()),
            content: "hello".to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::BASH_BORDER,
            diff_input: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let header = &lines[0];
        let indicator_color = header.spans.first().and_then(|s| s.style.fg);
        assert_eq!(
            indicator_color,
            Some(Color::Rgb(78, 186, 101)),
            "零 exit code 的 Bash 工具 ● 应为绿色"
        );
    }

    #[test]
    fn test_non_bash_tool_不受exit_code影响() {
        use crate::app::MessageViewModel;
        // Read 工具内容碰巧包含 "[Exit code: 1]"，不应被误判
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Read".to_string(),
            tool_call_id: "tc_read".to_string(),
            display_name: "Read".to_string(),
            args_display: Some("file.txt".to_string()),
            content: "some content with [Exit code: 1] in it".to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let header = &lines[0];
        let indicator_color = header.spans.first().and_then(|s| s.style.fg);
        assert_eq!(
            indicator_color,
            Some(Color::Rgb(78, 186, 101)),
            "非 Bash 工具不应受 exit code 解析影响"
        );
    }

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

use super::dim_markdown_lines;

/// 构造带前景色的 Span
fn make_colored_span(content: &str, fg: Color) -> Span<'static> {
    Span::styled(content.to_string(), Style::default().fg(fg))
}

#[test]
fn test_dim_markdown_lines_空文本() {
    let input = Text::raw("");
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    assert!(result[0].spans.is_empty() || result[0].spans[0].content.as_ref().is_empty());
}

#[test]
fn test_dim_markdown_lines_无前景色span设为dim() {
    let input = Text::from(vec![Line::from(vec![
        Span::raw("hello"),
        Span::raw(" world"),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    for span in &result[0].spans {
        assert_eq!(span.style.fg, Some(theme::DIM), "无前景色的 span 应设为 theme::DIM");
    }
}

#[test]
fn test_dim_markdown_lines_有前景色span加dim修饰() {
    let input = Text::from(vec![Line::from(vec![
        make_colored_span("keyword", Color::Red),
        make_colored_span("string", Color::Green),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].spans[0].style.fg, Some(Color::Red));
    assert!(result[0].spans[0].style.add_modifier.contains(Modifier::DIM), "有前景色的 span 应加 DIM 修饰");
    assert_eq!(result[0].spans[1].style.fg, Some(Color::Green));
    assert!(result[0].spans[1].style.add_modifier.contains(Modifier::DIM), "有前景色的 span 应加 DIM 修饰");
}

#[test]
fn test_dim_markdown_lines_多行保留结构() {
    let input = Text::from(vec![
        Line::from(vec![Span::raw("line1")]),
        Line::from(vec![Span::raw("line2")]),
        Line::from(vec![Span::raw("line3")]),
    ]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 3, "应保留原始行数");
    for line in &result {
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].style.fg, Some(theme::DIM));
    }
}

#[test]
fn test_dim_markdown_lines_混合有色无色span() {
    let input = Text::from(vec![Line::from(vec![
        Span::raw("plain"),
        make_colored_span("colored", Color::Yellow),
        Span::raw("also plain"),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].spans.len(), 3);
    assert_eq!(result[0].spans[0].style.fg, Some(theme::DIM));
    assert_eq!(result[0].spans[1].style.fg, Some(Color::Yellow));
    assert!(result[0].spans[1].style.add_modifier.contains(Modifier::DIM));
    assert_eq!(result[0].spans[2].style.fg, Some(theme::DIM));
}

#[test]
fn test_dim_markdown_lines_内容不变() {
    let input = Text::from(vec![Line::from(vec![
        Span::raw("hello "),
        make_colored_span("world", Color::Cyan),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result[0].spans[0].content.as_ref(), "hello ");
    assert_eq!(result[0].spans[1].content.as_ref(), "world");
}

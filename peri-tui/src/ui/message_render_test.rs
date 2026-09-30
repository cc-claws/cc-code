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
        let lines = render_batch_summary(&agents, &true, 80);
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
        let lines = render_batch_summary(&agents, &false, 80);
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
        let lines = render_batch_summary(&agents, &true, 80);
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
        let lines = render_batch_summary(&agents, &true, 80);
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
        let lines = render_batch_summary(&agents, &true, 80);
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
            started_at: None,
            moved_to_background: false,
        };
        vm.recompute_hash();

        let lines = render_view_model(&vm, None, 80, false, 0);
        let full_text = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.clone()))
            .collect::<Vec<_>>()
            .join("");

        assert!(full_text.contains("! git status"), "应显示命令块标题");
        assert!(!full_text.contains("exit code"), "命令块标题不应显示 exit code");
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
            started_at: None,
            moved_to_background: false,
        };
        vm.recompute_hash();

        let lines = render_view_model(&vm, None, 80, false, 0);
        let has_red_span = lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.as_ref() == "red" && span.style.fg == Some(Color::Red)
        });

        assert!(has_red_span, "ANSI 31m 应渲染为红色 span");
    }

    #[test]
    fn test_shell_command_render_strips_non_sgr_terminal_sequences() {
        let mut vm = MessageViewModel::ShellCommand {
            id: "shell-control".to_string(),
            command: "echo \x1b[<555;106;49Mcontrol".to_string(),
            cwd: ".".to_string(),
            stdin: Vec::new(),
            stdout: "ok \x1b[<555;106;49M\x1b]0;bad title\u{7}red done".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            collapsed: true,
            content_hash: 0,
            started_at: None,
            moved_to_background: false,
        };
        vm.recompute_hash();

        let lines = render_view_model(&vm, None, 80, false, 0);
        let text = rendered_text(&lines);
        assert!(!text.contains('\u{1b}'), "不应输出 ESC 控制字符: {text:?}");
        assert!(
            !text.contains("[<555;106;49M"),
            "不应输出 SGR 鼠标坐标序列: {text:?}"
        );
        assert!(
            !text.contains("bad title"),
            "OSC 标题序列内容不应进入输出: {text:?}"
        );
        assert!(text.contains("! echo control"), "命令标题普通文本应保留: {text:?}");
        assert!(text.contains("red done"), "普通文本应保留: {text:?}");
    }

    #[test]
    fn test_shell_command_render_uses_command_block_layout() {
        let mut vm = MessageViewModel::ShellCommand {
            id: "shell-block".to_string(),
            command: "restart-9router.cmd".to_string(),
            cwd: r"D:\code\9router".to_string(),
            stdin: Vec::new(),
            stdout: "[9router] Stopping old instance on 20130...\n[9router] Starting http://localhost:20130\n[9router] Startup timed out, check .9router-err.log".to_string(),
            stderr: String::new(),
            exit_code: Some(1),
            collapsed: true,
            content_hash: 0,
            started_at: None,
            moved_to_background: false,
        };
        vm.recompute_hash();

        let lines = render_view_model(&vm, None, 80, false, 0);
        let rendered_lines: Vec<String> = lines
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
            .collect();

        assert_eq!(rendered_lines[0].trim_end(), "! restart-9router.cmd");
        assert_eq!(rendered_lines[1], "  └ [9router] Stopping old instance on 20130...");
        assert_eq!(rendered_lines[2], "    [9router] Starting http://localhost:20130");
        assert_eq!(
            rendered_lines[3],
            "    [9router] Startup timed out, check .9router-err.log"
        );
        assert!(
            rendered_lines.iter().all(|line| !line.contains("exit 1") && !line.contains("D:\\code\\9router")),
            "命令块不应显示 exit code 或 cwd: {rendered_lines:?}"
        );

        let header = &lines[0];
        assert_eq!(header.spans[0].style.fg, Some(crate::ui::theme::BASH_BORDER));
        assert_eq!(header.spans[0].style.bg, Some(crate::ui::theme::USER_BG));
        assert_eq!(
            header.spans.last().unwrap().style.bg,
            Some(crate::ui::theme::USER_BG)
        );
    }

    #[test]
    fn test_shell_command_render_no_output_uses_command_block_placeholder() {
        let mut vm = MessageViewModel::ShellCommand {
            id: "shell-empty".to_string(),
            command: "sleep 30 && gh pr checks 18".to_string(),
            cwd: ".".to_string(),
            stdin: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            collapsed: true,
            content_hash: 0,
            started_at: None,
            moved_to_background: false,
        };
        vm.recompute_hash();

        let lines = render_view_model(&vm, None, 80, false, 0);
        let text = rendered_text(&lines);
        assert!(text.contains("! sleep 30 && gh pr checks 18"));
        assert!(text.contains("  └ (No output)"));
        assert!(!text.contains("(no output)"));
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
            started_at: None,
            moved_to_background: false,
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
            started_at: None,
            moved_to_background: false,
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
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
    fn test_tool_block_header_and_output_strip_terminal_control_sequences() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_control".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("echo \u{1b}[<555;106;49Munsafe".to_string()),
            content: "ok \u{1b}[<555;106;49Mline\n\u{1b}]0;bad title\u{7}done".to_string(),
            is_error: false,
            collapsed: false,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let text = rendered_text(&lines);
        assert!(!text.contains('\u{1b}'), "不应输出 ESC 控制字符: {text:?}");
        assert!(
            !text.contains("[<555;106;49M"),
            "不应输出 SGR 鼠标坐标序列: {text:?}"
        );
        assert!(
            !text.contains("bad title"),
            "OSC 标题序列内容不应进入输出: {text:?}"
        );
        assert!(text.contains("unsafe"), "工具参数普通文本应保留: {text:?}");
        assert!(text.contains("line"), "工具输出普通文本应保留: {text:?}");
        assert!(text.contains("done"), "工具输出普通文本应保留: {text:?}");
    }

    #[test]
    fn test_running_bash_toolblock_after_threshold_shows_control_b_hint() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_running".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("python wuhan_weather.py".to_string()),
            content: String::new(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: Some(std::time::Instant::now() - std::time::Duration::from_secs(39)),
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let rendered_lines: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        let text = rendered_text(&lines);
        assert!(
            text.contains(crate::ui::message_render::CONTROL_B_BACKGROUND_HINT),
            "运行超过 2 秒的 Bash ToolBlock 应显示 Ctrl+B 提示: {text:?}"
        );
        assert!(
            rendered_lines[0].contains("● Bash(python wuhan_weather.py)"),
            "Bash ToolBlock header 应保留运行中圆点和命令摘要: {rendered_lines:?}"
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line.contains("⎿ Running… (39s)")),
            "Bash 运行状态应显示 Running… 和已运行时间: {rendered_lines:?}"
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == "    (ctrl+b to run in background)"),
            "Ctrl+B 提示应作为缩进行显示，不应重复 ⎿: {rendered_lines:?}"
        );
        assert!(
            !rendered_lines
                .iter()
                .any(|line| line.contains("⎿ (ctrl+b to run in background)")),
            "Ctrl+B 提示行不应再带 ⎿ 前缀: {rendered_lines:?}"
        );
    }

    #[test]
    fn test_running_bash_toolblock_before_threshold_hides_control_b_hint() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_running_new".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("python wuhan_weather.py".to_string()),
            content: String::new(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: Some(std::time::Instant::now()),
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let text = rendered_text(&lines);
        assert!(
            !text.contains(crate::ui::message_render::CONTROL_B_BACKGROUND_HINT),
            "未超过 2 秒的 Bash ToolBlock 不应显示 Ctrl+B 提示: {text:?}"
        );
    }

    #[test]
    fn test_backgrounded_bash_shows_manage_status_and_timeout_not_ctrl_b_hint() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_backgrounded".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("cargo test -p peri-agent".to_string()),
            content: String::new(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            execution_timeout_ms: Some(600_000),
            shell_backgrounded: true,
            started_at: Some(std::time::Instant::now()),
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 100, false, 0);
        let rendered_lines: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == "  ⎿ Running in the background (↓ to manage)"),
            "后台化后应显示可管理状态: {rendered_lines:?}"
        );
        assert!(
            rendered_lines
                .iter()
                .any(|line| line == "    (timeout 10m)"),
            "应保留真实执行期限: {rendered_lines:?}"
        );
        assert!(
            !rendered_lines
                .iter()
                .any(|line| line.contains("ctrl+b to run in background")),
            "已后台化的 Bash 不应继续提示再次按 Ctrl+B: {rendered_lines:?}"
        );
    }

    #[test]
    fn test_backgrounded_bash_result_marker_keeps_running_status_until_exit() {
        use crate::app::MessageViewModel;
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_background_result".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("sleep 30".to_string()),
            content: "<background-task-started><task-id>abc-123</task-id><command>sleep 30</command><output>C:/tmp/abc-123.log</output></background-task-started>".to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            execution_timeout_ms: Some(600_000),
            shell_backgrounded: true,
            started_at: Some(std::time::Instant::now()),
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 100, false, 0);
        let text = rendered_text(&lines);
        assert!(
            text.contains("Running in the background (↓ to manage)"),
            "工具已返回后台任务句柄时仍要显示运行状态: {text:?}"
        );
        assert!(
            text.contains("(timeout 10m)"),
            "后台状态应显示执行期限: {text:?}"
        );
        assert!(
            !text.contains("ctrl+b to run in background"),
            "后台化后不能再提示 Ctrl+B: {text:?}"
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
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
        // 用非 Bash 工具验证通用截断行为（20 行）；Bash 另有 3 行摘要规则，见下一个测试
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Grep".to_string(),
            tool_call_id: "tc_long".to_string(),
            display_name: "Grep".to_string(),
            args_display: Some("pattern".to_string()),
            content,
            is_error: false,
            collapsed: false,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
    fn test_bash_non_detail_mode_shows_first_three_lines() {
        use crate::app::MessageViewModel;
        // Arrange：Bash 输出 10 行，非详细模式
        let content = (0..10)
            .map(|idx| format!("line {idx:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "bash3".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("cmd".to_string()),
            content,
            is_error: false,
            collapsed: false,
            color: crate::ui::theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
            content_hash: 0,
        };

        // Act
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

        // Assert：非详细模式仅前 3 行 + 提示；详细模式完整
        assert!(normal_text.contains("line 02"), "非详细模式应显示前 3 行");
        assert!(
            !normal_text.contains("line 03"),
            "非详细模式应截断第 4 行起，实际: {normal_text}"
        );
        assert!(
            normal_text.contains("... (7 more lines)"),
            "应显示剩余行数提示，实际: {normal_text}"
        );
        assert!(detail_text.contains("line 09"), "详细模式应显示完整输出");
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
            standalone_action: None,
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
        // 可断行的长内容：渲染层预折行后仍应完整保留尾部标记（无空白长串会被硬断，不适合本断言）
        let long_content = format!("{}tail-marker", "word ".repeat(60));
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
            standalone_action: None,
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
            standalone_action: None,
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
            standalone_action: None,
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
    fn test_render_user_bubble_link_hit_aligns_with_prefixed_line() {
        // 端到端验证：markdown → ViewModel → render_view_model_with_links 后，
        // 链接命中区的行号与 grapheme 范围必须精确指向输出行中的链接标签（含 "❯ " 前缀偏移）
        use crate::ui::markdown::parse_markdown_rich;
        use unicode_segmentation::UnicodeSegmentation;

        let text = "请看 [#222](https://github.com/cc-claws/cc-code/pull/222) 的修复";
        let doc = parse_markdown_rich(text, 80);
        let vm = MessageViewModel::UserBubble {
            content: text.to_string(),
            rendered: doc.text,
            rendered_links: doc.links,
            content_hash: 0,
            system_reminder: false,
            expanded_content: None,
        };

        let (lines, links) = render_view_model_with_links(&vm, Some(1), 80, false, 0);
        assert_eq!(links.len(), 1, "应保留 1 个链接命中区");
        assert_eq!(
            links[0].url, "https://github.com/cc-claws/cc-code/pull/222",
            "URL 必须完整保留"
        );

        let plain: String = lines[links[0].line]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let label: String = plain
            .graphemes(true)
            .skip(links[0].g_start)
            .take(links[0].g_end - links[0].g_start)
            .collect();
        assert_eq!(
            label, "#222",
            "命中区应精确指向链接标签（含前缀偏移），实际行: {plain:?}"
        );
    }

    #[test]
    fn test_render_assistant_bubble_link_hit_aligns_with_bullet_prefix() {
        use crate::ui::markdown::parse_markdown_rich;
        use unicode_segmentation::UnicodeSegmentation;

        let text = "已修复 [#222](https://github.com/cc-claws/cc-code/pull/222) 的问题";
        let doc = parse_markdown_rich(text, 78);
        let mut vm = MessageViewModel::assistant();
        if let MessageViewModel::AssistantBubble { blocks, .. } = &mut vm {
            blocks.push(ContentBlockView::Text {
                raw: text.to_string(),
                rendered: doc.text,
                rendered_links: doc.links,
                dirty: false,
                rendered_prefix_len: text.len(),
                rendered_prefix_lines: 1,
                rendered_width: 78,
                holdback_scanner: crate::ui::markdown::TableHoldbackScanner::new(),
            });
        }

        let (lines, links) = render_view_model_with_links(&vm, Some(1), 80, false, 0);
        assert_eq!(links.len(), 1, "应保留 1 个链接命中区");
        assert_eq!(
            links[0].url, "https://github.com/cc-claws/cc-code/pull/222",
            "URL 必须完整保留"
        );

        let plain: String = lines[links[0].line]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(plain.starts_with("● "), "AI 回复首行应有 ● 前缀，实际: {plain:?}");
        let label: String = plain
            .graphemes(true)
            .skip(links[0].g_start)
            .take(links[0].g_end - links[0].g_start)
            .collect();
        assert_eq!(label, "#222", "命中区应精确指向链接标签，实际行: {plain:?}");
    }

    #[test]
    fn test_parse_exit_code_nonzero_exit_code() {
        assert_eq!(parse_exit_code("[Exit code: 1]"), Some(1));
        assert_eq!(parse_exit_code("[Exit code: 42]"), Some(42));
        assert_eq!(parse_exit_code("[Exit code: 127]"), Some(127));
        assert_eq!(parse_exit_code("[Exit code: -1]"), Some(-1));
    }

    #[test]
    fn test_parse_exit_code_zero_exit_code() {
        assert_eq!(parse_exit_code("[Exit code: 0]"), Some(0));
    }

    #[test]
    fn test_parse_exit_code_empty_output_format() {
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
    fn test_parse_exit_code_mixed_content() {
        let content = "hello world\n[stderr]\nsome error\n[Exit code: 1]";
        assert_eq!(parse_exit_code(content), Some(1));
    }

    #[test]
    fn test_parse_exit_code_no_exit_code() {
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
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
    fn test_bash_nonzero_exit_result_lines_use_error_color() {
        use crate::app::MessageViewModel;
        // Bash 非零 exit code：输出行与 ⎿ 前缀须与圆点/状态口径一致，使用 ERROR 红
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_bash_fail_color".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("git add missing.txt".to_string()),
            content: "[stderr]\nfatal: pathspec 'missing.txt' did not match any files\n[Exit code: 128]"
                .to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::BASH_BORDER,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let first_result = &lines[1];
        assert_eq!(
            first_result.spans[0].style.fg,
            Some(crate::ui::theme::ERROR),
            "非零 exit code 的 Bash ⎿ 前缀应为红色"
        );
        assert_eq!(
            first_result.spans[1].style.fg,
            Some(crate::ui::theme::ERROR),
            "非零 exit code 的 Bash 输出行应为红色"
        );
    }

    #[test]
    fn test_bash_zero_exit_result_lines_use_soft_color() {
        use crate::app::MessageViewModel;
        // 零 exit code 的 Bash：输出行保持 TEXT_SOFT，不得被误标红
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_bash_ok_color".to_string(),
            display_name: "Bash".to_string(),
            args_display: Some("echo hello".to_string()),
            content: "hello".to_string(),
            is_error: false,
            collapsed: true,
            color: crate::ui::theme::BASH_BORDER,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
            content_hash: 0,
        };
        let lines = render_view_model(&vm, Some(1), 80, false, 0);
        let first_result = &lines[1];
        assert_eq!(
            first_result.spans[0].style.fg,
            Some(crate::ui::theme::DIM),
            "零 exit code 的 Bash ⎿ 前缀应保持 DIM"
        );
        assert_eq!(
            first_result.spans[1].style.fg,
            Some(crate::ui::theme::TEXT_SOFT),
            "零 exit code 的 Bash 输出行应保持 TEXT_SOFT"
        );
    }

    #[test]
    fn test_non_bash_tool_exit_code() {
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
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
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
fn test_dim_markdown_lines_empty_text() {
    let input = Text::raw("");
    let result = dim_markdown_lines(input);
    assert_eq!(result.len(), 1);
    assert!(result[0].spans.is_empty() || result[0].spans[0].content.as_ref().is_empty());
}

#[test]
fn test_dim_markdown_lines_no_fg_span_set_dim() {
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
fn test_dim_markdown_lines_with_fg_span_add_dim() {
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
fn test_dim_markdown_lines_multiline_keeps_structure() {
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
fn test_dim_markdown_lines_span() {
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
fn test_dim_markdown_lines_content_unchanged() {
    let input = Text::from(vec![Line::from(vec![
        Span::raw("hello "),
        make_colored_span("world", Color::Cyan),
    ])]);
    let result = dim_markdown_lines(input);
    assert_eq!(result[0].spans[0].content.as_ref(), "hello ");
    assert_eq!(result[0].spans[1].content.as_ref(), "world");
}

#[test]
fn test_tool_block_header_long_args_single_line_and_truncated() {
    use crate::app::MessageViewModel;
    use unicode_width::UnicodeWidthStr;

    let long_cmd = "git log --graph --oneline --decorate --all --stat --pretty=format:'%C(yellow)%h%Creset -%C(red)%d%Creset %s %Cgreen(%cr) %C(bold blue)<%an>%Creset' -n 20";
    let vm = MessageViewModel::ToolBlock {
        tool_name: "Bash".to_string(),
        tool_call_id: "tc_bash_long".to_string(),
        display_name: "Bash".to_string(),
        args_display: Some(long_cmd.to_string()),
        content: String::new(),
        is_error: false,
        collapsed: true,
        color: crate::ui::theme::SAGE,
        diff_input: None,
        execution_timeout_ms: None,
        shell_backgrounded: false,
        started_at: None,
        content_hash: 0,
    };

    let width = 80;
    let lines = render_view_model(&vm, Some(1), width, false, 0);

    assert_eq!(lines.len(), 1, "非详细模式超长参数的 ToolBlock Header 应保持单行");

    let header_line = &lines[0];
    let header_text: String = header_line.spans.iter().map(|s| s.content.clone()).collect();

    assert!(header_text.contains('…'), "超长命令应该包含截断省略号: {header_text}");
    assert!(header_text.ends_with(')'), "Header 应该保持以右括号闭合: {header_text}");

    let total_width: usize = header_line
        .spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    assert!(
        total_width <= width,
        "Header 视觉列宽 ({total_width}) 不应超过终端宽度 ({width})"
    );
    // 非详细模式：命令宽度被限制在可用宽度的 16/19（≈84%），既比原先更宽松
    // （旧实现几乎占满整行），也仍与消息区最右侧保持距离，不贴右边缘截断。
    assert!(
        total_width < width,
        "非详细模式 Header 不应占满整行顶到最右 ({total_width} vs {width})"
    );
    assert!(
        total_width * 100 >= width * 70,
        "非详细模式 Header 宽度应接近可用宽度的 84%，不应被压得过窄 ({total_width} vs {width})"
    );
}

#[test]
fn test_tool_block_header_detail_mode_wraps_full_command_aligned() {
    use crate::app::MessageViewModel;
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    let long_cmd = "git log --graph --oneline --decorate --all --stat --pretty=format:'%h %s' -n 20 --since='1 week ago' --author=someone --grep=fix";
    let vm = MessageViewModel::ToolBlock {
        tool_name: "Bash".to_string(),
        tool_call_id: "tc_bash_detail_wrap".to_string(),
        display_name: "Bash".to_string(),
        args_display: Some(long_cmd.to_string()),
        content: String::new(),
        is_error: false,
        collapsed: true,
        color: crate::ui::theme::SAGE,
        diff_input: None,
        execution_timeout_ms: None,
        shell_backgrounded: false,
        started_at: None,
        content_hash: 0,
    };

    let width = 60;
    let lines = render_view_model(&vm, Some(1), width, true, 0);
    let rendered: Vec<String> = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.clone()).collect::<String>())
        .collect();

    assert!(
        rendered.len() > 1,
        "详细模式超长命令应折成多行，实际 {} 行: {:?}",
        rendered.len(),
        rendered
    );
    // 完整命令不被截断
    let joined: String = rendered.join("");
    assert!(
        !joined.contains('…'),
        "详细模式命令不应被截断，实际: {joined}"
    );
    assert!(
        joined.contains("--grep=fix"),
        "详细模式应展示完整命令尾部，实际: {joined}"
    );
    // 续行与首行命令起始列对齐（悬挂缩进）
    let cmd_col = UnicodeWidthStr::width("● Bash") + 1;
    for (idx, line) in rendered.iter().enumerate() {
        let w: usize = line.chars().map(|c| c.width().unwrap_or(0)).sum();
        assert!(w <= width, "第 {} 行超宽 {}: {:?}", idx + 1, w, line);
    }
    for (idx, line) in rendered.iter().enumerate().skip(1) {
        let cont_indent: usize = line.chars().take_while(|c| *c == ' ').count();
        assert_eq!(
            cont_indent, cmd_col,
            "第 {} 行缩进 ({cont_indent}) 应与首行命令起始列 ({cmd_col}) 对齐: {rendered:?}",
            idx + 1
        );
    }
}

#[test]
fn test_tool_block_header_detail_mode_short_command_stays_single_line() {
    use crate::app::MessageViewModel;

    let vm = MessageViewModel::ToolBlock {
        tool_name: "Bash".to_string(),
        tool_call_id: "tc_bash_detail_short".to_string(),
        display_name: "Bash".to_string(),
        args_display: Some("echo hello".to_string()),
        content: String::new(),
        is_error: false,
        collapsed: true,
        color: crate::ui::theme::SAGE,
        diff_input: None,
        execution_timeout_ms: None,
        shell_backgrounded: false,
        started_at: None,
        content_hash: 0,
    };

    let lines = render_view_model(&vm, Some(1), 80, true, 0);
    assert_eq!(
        lines.len(),
        1,
        "详细模式下短命令仍应与 header 同行，不拆行: {:?}",
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.clone()).collect::<String>())
            .collect::<Vec<_>>()
    );
    let text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
    assert!(text.contains("Bash(echo hello)"), "实际: {text}");
}

#[test]
fn test_tool_block_header_cjk_truncation_width_aligned() {
    use crate::app::MessageViewModel;
    use unicode_width::UnicodeWidthStr;

    let long_cmd = "git commit -m \"这是一个非常长非常长的中文提交信息说明用于测试终端视觉宽度截断是否对齐\"";
    let vm = MessageViewModel::ToolBlock {
        tool_name: "Bash".to_string(),
        tool_call_id: "tc_bash_cjk".to_string(),
        display_name: "Bash".to_string(),
        args_display: Some(long_cmd.to_string()),
        content: String::new(),
        is_error: false,
        collapsed: true,
        color: crate::ui::theme::SAGE,
        diff_input: None,
        execution_timeout_ms: None,
        shell_backgrounded: false,
        started_at: None,
        content_hash: 0,
    };

    let width = 50;
    let lines = render_view_model(&vm, Some(1), width, false, 0);
    assert_eq!(lines.len(), 1, "中文超长参数同样应该只有单行 Header");

    let header_line = &lines[0];
    let total_width: usize = header_line
        .spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    assert!(
        total_width <= width,
        "中文截断后的 Header 视觉列宽 ({total_width}) 不应超过终端宽度 ({width})"
    );
}

#[test]
fn test_render_user_bubble_long_paragraph_hanging_indent() {
    let long_text = "这是一条很长的用户消息用于验证普通段落超宽折行后续行是否悬挂缩进对齐首行文字起始位置内容持续填充直到必然超过四十列终端宽度限制为止";
    let vm = MessageViewModel::user(long_text.to_string());
    let width = 40;
    let lines = render_view_model(&vm, Some(1), width, false, 0);
    assert!(lines.len() > 1, "超长段落应折成多行，实际 {} 行", lines.len());
    let first: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
    assert!(
        first.starts_with("❯ "),
        "首行应以 ❯ 前缀开头，实际 {:?}",
        first.chars().take(10).collect::<String>()
    );
    for (idx, line) in lines.iter().enumerate() {
        let text: String = line.spans.iter().map(|s| s.content.clone()).collect();
        let row_w: usize = line.spans.iter().map(|s| s.content.width()).sum();
        assert!(
            row_w <= width,
            "第 {} 行显示宽度 {} 超过终端宽度 {}",
            idx + 1,
            row_w,
            width
        );
        if idx > 0 {
            assert!(
                text.starts_with("  "),
                "第 {} 行应以 2 空格悬挂缩进对齐首行文字，实际 {:?}",
                idx + 1,
                text.chars().take(10).collect::<String>()
            );
        }
    }
}

#[test]
fn test_render_assistant_text_long_paragraph_hanging_indent() {
    let long_text = "**结论先行**：这是一段很长的AI回复内容用于验证普通段落超宽折行后续行是否悬挂缩进对齐首行文字起始位置内容持续填充直到必然超过四十列终端宽度限制为止";
    let mut vm = MessageViewModel::assistant();
    if let MessageViewModel::AssistantBubble { blocks, .. } = &mut vm {
        blocks.push(ContentBlockView::Text {
            raw: long_text.to_string(),
            rendered: crate::ui::markdown::parse_markdown(long_text, 38),
            rendered_links: Vec::new(),
            dirty: false,
            rendered_prefix_len: long_text.len(),
            rendered_prefix_lines: 1,
            rendered_width: 38,
            holdback_scanner: crate::ui::markdown::TableHoldbackScanner::new(),
        });
    }
    let width = 40;
    let lines = render_view_model(&vm, Some(1), width, false, 0);
    assert!(lines.len() > 1, "超长段落应折成多行，实际 {} 行", lines.len());
    let first: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
    assert!(
        first.starts_with("● "),
        "首行应以 ● 前缀开头，实际 {:?}",
        first.chars().take(10).collect::<String>()
    );
    for (idx, line) in lines.iter().enumerate() {
        let text: String = line.spans.iter().map(|s| s.content.clone()).collect();
        let row_w: usize = line.spans.iter().map(|s| s.content.width()).sum();
        assert!(
            row_w <= width,
            "第 {} 行显示宽度 {} 超过终端宽度 {}",
            idx + 1,
            row_w,
            width
        );
        if idx > 0 {
            assert!(
                text.starts_with("  "),
                "第 {} 行应以 2 空格悬挂缩进对齐首行文字，实际 {:?}",
                idx + 1,
                text.chars().take(10).collect::<String>()
            );
        }
    }
}

#[test]
fn test_render_user_bubble_wrapped_link_hit_across_segments() {
    use crate::ui::markdown::parse_markdown_rich;
    use unicode_segmentation::UnicodeSegmentation;

    // 窄宽度下链接标签必然折行，命中区应覆盖折行产生的多段且都落在标签文本内
    let text = "见 [verylonglinklabel](https://example.com/very/long/path) 结束";
    let width = 24usize;
    let content_width = width - 2;
    let doc = parse_markdown_rich(text, content_width);
    let vm = MessageViewModel::UserBubble {
        content: text.to_string(),
        rendered: doc.text,
        rendered_links: doc.links,
        content_hash: 0,
        system_reminder: false,
        expanded_content: None,
    };

    let (lines, links) = render_view_model_with_links(&vm, Some(1), width, false, 0);
    assert!(!links.is_empty(), "折行后应至少有一个链接命中区");
    for hit in &links {
        assert_eq!(hit.url, "https://example.com/very/long/path");
        assert!(hit.line < lines.len(), "命中区行号应在输出范围内");
        assert!(hit.g_start <= hit.g_end, "命中区 g 范围不应逆序");
        let plain: String = lines[hit.line]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let label: String = plain
            .graphemes(true)
            .skip(hit.g_start)
            .take(hit.g_end - hit.g_start)
            .collect();
        assert!(
            !label.is_empty() && "verylonglinklabel".contains(&label),
            "命中区应精确落在链接标签内，实际提取 {label:?}（整行 {plain:?}）"
        );
    }
}

#[test]
fn test_render_two_wrapped_links_do_not_cross_lines() {
    use crate::ui::markdown::parse_markdown_rich;
    use unicode_segmentation::UnicodeSegmentation;

    let text = "[firstlink](https://a.example/1) 与 [secondlink](https://b.example/2)";
    let width = 22usize;
    let content_width = width - 2;
    let doc = parse_markdown_rich(text, content_width);
    let vm = MessageViewModel::UserBubble {
        content: text.to_string(),
        rendered: doc.text,
        rendered_links: doc.links,
        content_hash: 0,
        system_reminder: false,
        expanded_content: None,
    };

    let (lines, links) = render_view_model_with_links(&vm, Some(1), width, false, 0);
    // 每个命中区提取的文本必须只属于它自己的标签，不能串到另一个链接
    for hit in &links {
        let plain: String = lines[hit.line]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        let label: String = plain
            .graphemes(true)
            .skip(hit.g_start)
            .take(hit.g_end - hit.g_start)
            .collect();
        if hit.url.contains("a.example") {
            assert!(
                "firstlink".contains(&label),
                "第一个链接的命中区不应串到第二段，实际 {label:?}"
            );
        } else {
            assert!(
                "secondlink".contains(&label),
                "第二个链接的命中区不应串到第一段，实际 {label:?}"
            );
        }
    }
}

#[test]
fn test_render_view_model_wrapper_matches_with_links_lines() {
    // 回归：render_view_model 是 render_view_model_with_links 的薄包装，行内容必须完全一致
    let vm = MessageViewModel::user("含 [链接](https://e.com) 的消息".to_string());
    let plain = render_view_model(&vm, Some(1), 80, false, 0);
    let (rich, _) = render_view_model_with_links(&vm, Some(1), 80, false, 0);
    assert_eq!(plain.len(), rich.len(), "薄包装不应改变行数");
    for (a, b) in plain.iter().zip(rich.iter()) {
        let ta: String = a.spans.iter().map(|s| s.content.as_ref()).collect();
        let tb: String = b.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(ta, tb, "薄包装不应改变行内容");
    }
}

#[test]
fn test_render_view_model_recap_prefix_is_plain_text() {
    // 无任何代码路径生成 "※ recap:" 开头的 System 消息，该前缀按普通文本渲染
    let content = "※ recap: 测试摘要";
    let vm = MessageViewModel::system(content.to_string());
    let lines = render_view_model(&vm, None, 120, false, 0);

    assert_eq!(lines.len(), 1, "应渲染为一行");
    let plain: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(plain, "· ※ recap: 测试摘要", "应作为普通系统消息渲染（前缀 · ），不做 recap 特殊着色");
}

#[test]
fn test_render_recap_line_color_hierarchy() {
    use crate::ui::main_ui::message_area::render_recap_line;

    // 正文不含后缀，后缀由 i18n 追加
    let text = "当前正在进行连续加法计算，已完成至6+6=12。请继续输入下一道算式。";
    let lc = crate::i18n::LcRegistry::new(Some("zh-CN"));
    let line = render_recap_line(text, &lc);

    assert_eq!(line.spans.len(), 5, "应包含 5 个 span（※、recap:、空格、正文、i18n 后缀）");

    // Span 0: "※ "（暗色 MUTED）
    assert_eq!(line.spans[0].content, "※ ");
    assert_eq!(line.spans[0].style.fg, Some(theme::MUTED));

    // Span 1: "recap:"（暗色粗体 MUTED）
    assert_eq!(line.spans[1].content, "recap:");
    assert_eq!(line.spans[1].style.fg, Some(theme::MUTED));
    assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));

    // Span 2: 空格
    assert_eq!(line.spans[2].content, " ");

    // Span 3: 正文（暗色斜体 MUTED）
    assert_eq!(line.spans[3].content, text);
    assert_eq!(line.spans[3].style.fg, Some(theme::MUTED));
    assert!(line.spans[3].style.add_modifier.contains(Modifier::ITALIC));

    // Span 4: i18n 后缀（暗灰弱化 DIM）
    assert_eq!(
        line.spans[4].content,
        format!("  {}", lc.tr("app-recap-hint"))
    );
    assert_eq!(line.spans[4].style.fg, Some(theme::DIM));
}

    fn line_plain_text(line: &Line<'static>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// 工具输出正文必须在渲染层预折行：任何一行的显示宽度都不超过视口宽度，
    /// 否则会触发 Paragraph::wrap 二次硬折行，续行顶格丢失 4 列悬挂缩进（图2 场景）。
    #[test]
    fn test_tool_block_long_json_wraps_with_hanging_indent() {
        let json = "{\"code\":0,\"msg\":\"success\",\"data\":{\"rmb_fee\":172.85,\"fee\":19.16,\"currency_code\":\"GBP\",\"info\":[{\"temp_name\":\"超尺寸附加费\",\"rule_name\":\"超重附加费\",\"fee_price\":12.1}]}}";
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Bash".to_string(),
            tool_call_id: "tc_json".to_string(),
            display_name: "Bash".to_string(),
            args_display: None,
            content: json.to_string(),
            is_error: false,
            collapsed: false,
            color: theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
            content_hash: 0,
        };
        let width = 80usize;
        let text_lines: Vec<String> = render_view_model(&vm, Some(1), width, false, 0)
            .iter()
            .map(line_plain_text)
            .collect();
        for l in &text_lines {
            let w = unicode_width::UnicodeWidthStr::width(l.as_str());
            assert!(w <= width, "工具输出行宽应 ≤ {width}，实际 {w}: {l:?}");
        }
        // 首行前缀应与内容同行，不被超长无空格内容挤成独立空行
        assert!(
            text_lines[1].starts_with("  ⎿ ") && text_lines[1].len() > "  ⎿ ".len(),
            "首行前缀应与内容同行: {:?}",
            text_lines[1]
        );
        // 续行必须保留 4 列悬挂缩进，不能顶格
        assert!(
            text_lines
                .iter()
                .skip(2)
                .any(|l| l.starts_with("    ") && !l.trim().is_empty()),
            "长 JSON 续行应保留 4 列悬挂缩进: {text_lines:?}"
        );
    }

    /// 错误摘要长行同样必须预折行，续行保持 4 列悬挂缩进（图1 场景）。
    #[test]
    fn test_error_summary_long_line_wraps_with_hanging_indent() {
        let content = "Tool execution failed: Grep - Invalid arguments for tool Grep:\nUnexpected parameter 'command' was provided (allowed parameters: [\"-A\", \"-B\", \"-C\", \"-i\", \"-n\", \"fixed_strings\", \"glob\", \"output_mode\", \"path\", \"pattern\", \"type\", \"whole_word\"])";
        let width = 80usize;
        let text_lines: Vec<String> = error_summary_lines(content, width, usize::MAX)
            .iter()
            .map(line_plain_text)
            .collect();
        for l in &text_lines {
            let w = unicode_width::UnicodeWidthStr::width(l.as_str());
            assert!(w <= width, "错误摘要行宽应 ≤ {width}，实际 {w}: {l:?}");
        }
        assert!(
            text_lines[0].starts_with("  ⎿ "),
            "错误摘要首行应带 ⎿ 前缀: {:?}",
            text_lines[0]
        );
        // 超长 allowed parameters 行折行后的续行应保留 4 列缩进
        assert!(
            text_lines
                .iter()
                .skip(2)
                .any(|l| l.starts_with("    ") && !l.trim().is_empty()),
            "错误摘要续行应保留 4 列悬挂缩进: {text_lines:?}"
        );
    }

    /// 返回 lines 中最长一行的显示宽度
    fn max_line_width(lines: &[Line<'static>]) -> usize {
        lines
            .iter()
            .map(|l| {
                unicode_width::UnicodeWidthStr::width(
                    l.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                        .as_str(),
                )
            })
            .max()
            .unwrap_or(0)
    }

    /// 回归：同类渲染路径（ShellCommand / SystemNote / SubAgentGroup / 批次汇总 /
    /// AskUser / CacheWarning）的长行都必须预折行，任何一行显示宽度都不超过视口宽度。
    /// 若回退修复，这些路径会重新溢出并触发 Paragraph 二次硬折行导致续行顶格。
    #[test]
    fn test_all_paths_preserve_width_no_overflow() {
        use crate::ui::message_view::{ToolCategory, ToolEntry};
        let width = 80usize;
        let long_json = "{\"code\":0,\"msg\":\"success\",\"data\":{\"rmb_fee\":172.85,\"fee\":19.16,\"currency_code\":\"GBP\",\"info\":[{\"temp_name\":\"超尺寸附加费\",\"rule_name\":\"超重附加费\",\"fee_price\":12.1}]}}";
        let long_cjk = "这是一段很长的中文说明文字用来测试折行行为是否可以在没有空格的情况下正确断开并且保持缩进对齐效果需要超过八十列宽度才可以".repeat(2);
        let long_wordy = format!("{}tail", "word ".repeat(40));

        // ShellCommand（! 本机命令）长 JSON 输出
        let vm = MessageViewModel::ShellCommand {
            id: "s".into(),
            command: "curl ...".into(),
            cwd: ".".into(),
            stdin: vec![],
            stdout: long_json.into(),
            stderr: String::new(),
            exit_code: Some(0),
            collapsed: true,
            content_hash: 0,
            started_at: None,
            moved_to_background: false,
        };
        assert!(
            max_line_width(&render_view_model(&vm, None, width, true, 0)) <= width,
            "ShellCommand 长输出应预折行"
        );

        // SystemNote 长行
        let vm = MessageViewModel::SystemNote {
            content: long_cjk.clone(),
            content_hash: 0,
        };
        assert!(
            max_line_width(&render_view_model(&vm, None, width, false, 0)) <= width,
            "SystemNote 长行应预折行"
        );

        // SubAgentGroup 展开态（嵌套 ToolBlock + final_result）
        let nested = MessageViewModel::ToolBlock {
            tool_name: "Bash".into(),
            tool_call_id: "n".into(),
            display_name: "Bash".into(),
            args_display: None,
            content: long_json.into(),
            is_error: false,
            collapsed: false,
            color: theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
            content_hash: 0,
        };
        let vm = MessageViewModel::SubAgentGroup {
            agent_id: "a".into(),
            task_preview: long_wordy.clone(),
            total_steps: 1,
            recent_messages: vec![nested],
            is_running: false,
            collapsed: false,
            final_result: Some(long_cjk.clone()),
            is_error: false,
            is_background: false,
            bg_hash: None,
            batch_agents: vec![],
            instance_id: None,
            content_hash: 0,
        };
        assert!(
            max_line_width(&render_view_model(&vm, None, width, false, 0)) <= width,
            "SubAgentGroup 展开态应预折行（含嵌套 + final_result）"
        );

        // 批次汇总 展开态
        let agent = AgentSummary {
            agent_id: "a".into(),
            task_preview: long_wordy.clone(),
            tool_count: 3,
            is_error: false,
            final_result: Some(long_cjk.clone()),
        };
        assert!(
            max_line_width(&render_batch_summary(&[agent], &false, width)) <= width,
            "批次汇总展开态应预折行"
        );

        // ToolCallGroup AskUser 长回答
        let vm = MessageViewModel::ToolCallGroup {
            category: ToolCategory::AskUser,
            tools: vec![ToolEntry {
                tool_name: "AskUserQuestion".into(),
                display_name: "Ask".into(),
                args_display: None,
                content: format!("[问: {}]\n回答: {}", long_cjk, long_cjk),
                is_error: false,
            }],
            collapsed: true,
            content_hash: 0,
            standalone_action: None,
        };
        assert!(
            max_line_width(&render_view_model(&vm, None, width, false, 0)) <= width,
            "AskUser 长回答应预折行"
        );

        // CacheWarning 长行
        let vm = MessageViewModel::CacheWarning {
            content: long_cjk.clone(),
            content_hash: 0,
        };
        assert!(
            max_line_width(&render_view_model(&vm, None, width, false, 0)) <= width,
            "CacheWarning 长行应预折行"
        );
    }

    /// 回归：带缩进的长行（美化 JSON / YAML / 缩进代码）折行后必须保留原前导缩进，
    /// 不能因 wrap_line_spans_rich 的 trim 丢失缩进而与同级别相邻行错位。
    #[test]
    fn test_wrapped_indented_line_keeps_leading_indent() {
        let blob = "jdoaPV4srHutyzpLXE0TSu7b0xNrI3aGj1eLKxrHutyzpLXE0ru2zsTayrI3aGj1eLKx7rcs6S1xXE0ru2zsTayN2ho9XiyLKx7rcs6S1xNK7ts7E2";
        let content = format!(
            "{{\n  \"args\": {{}},\n  \"data\": \"data:application/octet-stream;base64,{blob}\",\n  \"files\": {{}}\n}}"
        );
        let width = 48usize;
        // 用非 Bash 工具：本测试验证缩进保留（通用渲染逻辑），
        // Bash 另有「非详细模式仅前 3 行」规则会截断本用例
        let vm = MessageViewModel::ToolBlock {
            tool_name: "Grep".to_string(),
            tool_call_id: "indent".to_string(),
            display_name: "Grep".to_string(),
            args_display: None,
            content,
            is_error: false,
            collapsed: false,
            color: theme::SAGE,
            diff_input: None,
            execution_timeout_ms: None,
            shell_backgrounded: false,
            started_at: None,
            content_hash: 0,
        };
        let text_lines: Vec<String> = render_view_model(&vm, Some(1), width, false, 0)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // 同一缩进层级的 "args" / "data" / "files" 三行，内容起点必须一致（均为 6 空格）
        let args_line = text_lines
            .iter()
            .find(|l| l.contains("\"args\""))
            .expect("应有 args 行");
        let data_line = text_lines
            .iter()
            .find(|l| l.contains("\"data\":"))
            .expect("应有 data 行");
        let files_line = text_lines
            .iter()
            .find(|l| l.contains("\"files\""))
            .expect("应有 files 行");
        let lead = |s: &str| s.len() - s.trim_start().len();
        assert_eq!(
            lead(data_line),
            lead(args_line),
            "data 行缩进应与 args 行一致: {data_line:?} vs {args_line:?}"
        );
        assert_eq!(
            lead(files_line),
            lead(args_line),
            "files 行缩进应与 args 行一致: {files_line:?} vs {args_line:?}"
        );
        // data 行折行后的续行也必须保留同一缩进
        let data_pos = text_lines.iter().position(|l| l.contains("\"data\":")).unwrap();
        let cont = &text_lines[data_pos + 1];
        assert!(
            cont.starts_with(&" ".repeat(lead(data_line))),
            "data 续行应保留等宽缩进: {cont:?}"
        );
    }

/// 非详细模式下，超长错误摘要须限制行数（避免污染页面）。
#[test]
fn test_error_summary_limited_in_normal_mode() {
    // Arrange：10 行错误
    let content: String = (0..10)
        .map(|i| format!("error line {i:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let width = 80usize;

    // Act：非详细模式（上限 3）
    let normal: Vec<String> = error_summary_lines(&content, width, 3)
        .iter()
        .map(line_plain_text)
        .collect();

    // Assert：最多 3 行 + 1 行提示
    assert!(
        normal.len() <= 4,
        "非详细错误摘要应限行（≤3 行 + 提示），实际 {} 行: {normal:?}",
        normal.len()
    );
    assert!(
        normal.iter().any(|l| l.contains("more lines")),
        "应显示剩余行提示，实际: {normal:?}"
    );
    assert!(
        !normal.iter().any(|l| l.contains("error line 09")),
        "不应显示第 10 行，实际: {normal:?}"
    );

    // 详细模式（usize::MAX）：完整
    let detail: Vec<String> = error_summary_lines(&content, width, usize::MAX)
        .iter()
        .map(line_plain_text)
        .collect();
    assert_eq!(detail.len(), 10, "详细模式应显示全部 10 行");
}

    async fn render_headless_hitl_single() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30).await;
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let items = vec![BatchItem {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        }];
        let prompt = HitlBatchPrompt::new(items, tx);
        app.session_mgr.current_mut().agent.interaction_prompt =
            Some(InteractionPrompt::Approval(prompt));
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    async fn render_headless_hitl_multi() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30).await;
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let items = vec![
            BatchItem {
                tool_name: "Bash".to_string(),
                input: serde_json::json!({"command": "ls"}),
            },
            BatchItem {
                tool_name: "Write".to_string(),
                input: serde_json::json!({"path": "test.rs"}),
            },
        ];
        let prompt = HitlBatchPrompt::new(items, tx);
        app.session_mgr.current_mut().agent.interaction_prompt =
            Some(InteractionPrompt::Approval(prompt));
        // 通过 main_ui::render 渲染完整布局，确保面板高度正确
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_hitl_single_no_single_letter_hints() {
        let (app, handle) = render_headless_hitl_single().await;
        let snap = handle.snapshot().join("\n");
        // 不应出现单字母快捷键 y 或 n（作为独立快捷键提示）
        assert!(
            !snap.contains(":批准") || !snap.contains("y:"),
            "不应显示 y:批准 单字母快捷键"
        );
        assert!(
            !snap.contains(":拒绝") || !snap.contains("n:"),
            "不应显示 n:拒绝 单字母快捷键"
        );
        // 应显示合规快捷键
        assert!(handle.contains("↑↓"), "应显示上下键选择提示");
        assert!(!handle.contains("Space"), "审批不再使用空格循环");
        assert!(handle.contains("Enter"), "应显示 Enter 快捷键");
        for key in [
            "hitl-choice-once",
            "hitl-choice-session",
            "hitl-choice-reject",
        ] {
            assert!(
                handle.contains(&app.services.lc.tr(key)),
                "三个审批选项必须同时可见"
            );
        }
    }

    #[tokio::test]
    async fn test_hitl_multi_shows_enter_hint() {
        let (_, handle) = render_headless_hitl_multi().await;
        let snap = handle.snapshot().join("\n");
        // 多项应显示 Enter 确认
        assert!(
            snap.contains("Enter"),
            "多项应显示 Enter 快捷键，实际:\n{}",
            snap
        );
    }

    #[test]
    fn test_hitl_input_preview_strips_terminal_control_sequences() {
        let input = serde_json::json!({
            "command": "echo ok \u{1b}[<555;106;49M\u{1b}[31mred\u{1b}[0m done"
        });
        let preview = super::format_input_preview(&input, 120);
        assert!(
            !preview.contains('\u{1b}'),
            "HITL 预览不应保留 ESC 控制字符: {preview:?}"
        );
        assert!(
            !preview.contains("[<555;106;49M"),
            "HITL 预览不应保留 SGR 鼠标坐标序列: {preview:?}"
        );
        assert!(preview.contains("red"), "普通文本应保留: {preview:?}");
        assert!(preview.contains("done"), "普通文本应保留: {preview:?}");
    }

    #[test]
    fn test_hitl_input_preview_truncates_by_display_width() {
        let preview = super::format_input_preview(&serde_json::json!({"command":"执行中文命令"}), 12);
        assert!(
            unicode_width::UnicodeWidthStr::width(preview.as_str()) <= 12,
            "中文预览不能超出终端列宽"
        );
        assert!(preview.ends_with('…'));
    }

    #[tokio::test]
    async fn test_hitl_scrolled_batch_keeps_options_and_keyboard_hint_visible() {
        let (mut app, mut handle) = App::new_headless(120, 20).await;
        let (sender, _) = tokio::sync::oneshot::channel();
        let items = (0..8)
            .map(|index| BatchItem {
                tool_name: format!("Tool{index}"),
                input: serde_json::json!({"command":"custom-build"}),
            })
            .collect();
        let mut prompt = HitlBatchPrompt::new(items, sender);
        prompt.move_cursor(7);
        prompt.move_choice(2);
        app.session_mgr.current_mut().agent.interaction_prompt =
            Some(InteractionPrompt::Approval(prompt));
        handle
            .terminal
            .draw(|frame| crate::ui::main_ui::render(frame, &mut app))
            .unwrap();
        for key in [
            "hitl-choice-once",
            "hitl-choice-session",
            "hitl-choice-reject",
        ] {
            assert!(
                handle.contains(&app.services.lc.tr(key)),
                "滚动到末项仍应显示三个选项"
            );
        }
        assert!(
            handle.contains("Enter"),
            "快捷键固定在底部，不随工具列表滚走"
        );
        assert!(handle.contains("Tab"));
    }

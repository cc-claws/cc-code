
    fn make_enter() -> Input {
        Input {
            key: Key::Enter,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    async fn make_busy_app() -> App {
        let (mut app, _) = App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.loading = true;
        app
    }

    fn set_input(app: &mut App, text: &str) {
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr.current_mut().ui.textarea.insert_str(text);
    }

    fn input_text(app: &App) -> String {
        app.session_mgr.current().ui.textarea.lines().join("\n")
    }

    fn queued_count(app: &App) -> usize {
        app.session_mgr.current().messages.pending_messages.len()
    }

    fn notes_text(app: &App) -> String {
        format!("{:?}", app.session_mgr.current().messages.view_messages)
    }

    /// 按当前语言解析出的期望提示（避免把断言写死成某一种语言）。
    fn expected_note(app: &App, key: &str, command: &str) -> String {
        app.services
            .lc
            .tr_args(key, &[("command".into(), command.into())])
    }

    #[tokio::test]
    async fn test_busy_ui_command_is_rejected_and_input_preserved() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/theme");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(queued_count(&app), 0, "需空闲的 UI 命令不得入队");
        assert_eq!(input_text(&app), "/theme", "被拒绝时不得吞掉用户输入");
        let expected = expected_note(&app, "command-busy-rejected", "/theme");
        assert!(
            notes_text(&app).contains(&expected),
            "应提示运行中不可执行（{expected}），实际: {}",
            notes_text(&app)
        );
    }

    #[tokio::test]
    async fn test_busy_forwarding_command_is_queued() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/recap");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(queued_count(&app), 1, "/recap 应入队延后执行");
        assert!(input_text(&app).is_empty(), "入队后应清空输入框");
    }

    #[tokio::test]
    async fn test_busy_agent_command_is_queued() {
        let mut app = make_busy_app().await;
        app.session_mgr
            .current_mut()
            .commands
            .update_agent_commands(vec!["compact".to_string()]);
        set_input(&mut app, "/compact");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(queued_count(&app), 1, "ACP 命令应入队延后执行");
    }

    #[tokio::test]
    async fn test_busy_unknown_command_is_rejected_not_sent_to_model() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/no-such-command");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(
            queued_count(&app),
            0,
            "未知命令不得入队（否则会被当成普通消息发给模型）"
        );
        assert_eq!(input_text(&app), "/no-such-command", "输入应保留");
        let expected = expected_note(&app, "command-unknown", "/no-such-command");
        assert!(
            notes_text(&app).contains(&expected),
            "应提示未知命令（{expected}），实际: {}",
            notes_text(&app)
        );
    }

    #[tokio::test]
    async fn test_unknown_command_note_renders_immediately() {
        let (mut app, _handle) = App::new_headless(120, 30).await;
        let before = app.session_mgr.current().messages.render_cache.read().total_lines;
        set_input(&mut app, "/no-such-command");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        let after = app.session_mgr.current().messages.render_cache.read().total_lines;
        assert!(
            after > before,
            "未知命令提示应立即重绘（重绘前 {before} 行，重绘后 {after} 行）"
        );
    }

    #[tokio::test]
    async fn test_busy_shell_input_is_rejected_and_input_preserved() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "!ls");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(queued_count(&app), 0, "运行中的 shell 输入不得入队");
        assert_eq!(input_text(&app), "!ls", "输入应保留");
        let expected = expected_note(&app, "command-busy-rejected", "!ls");
        assert!(
            notes_text(&app).contains(&expected),
            "应提示运行中不可执行（{expected}），实际: {}",
            notes_text(&app)
        );
    }

    #[tokio::test]
    async fn test_busy_plain_message_is_still_queued() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "这段等本轮结束后再发");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(queued_count(&app), 1, "普通消息应照常入队");
    }

    #[tokio::test]
    async fn test_busy_deferred_local_command_runs_as_command_on_flush() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/streaming");
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(queued_count(&app), 1, "可延后的本地命令应入队");

        // 队列消费必须重新按命令派发，而不是把 "/streaming" 当普通文本发给模型
        let message = app
            .session_mgr
            .current_mut()
            .messages
            .pending_messages
            .remove(0);
        app.submit_queued_message(message);
        assert!(
            notes_text(&app).contains("streaming"),
            "延后的本地命令应在消费时真正执行，实际: {}",
            notes_text(&app)
        );
    }

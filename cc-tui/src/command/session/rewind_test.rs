    use cc_agent::messages::BaseMessage;

    async fn headless_app() -> App {
        App::new_headless(80, 24).await.0
    }

    fn push_human_turn(app: &mut App, text: &str) {
        app.session_mgr
            .current_mut()
            .agent
            .origin_messages
            .push(BaseMessage::human(text.to_string()));
    }

    #[tokio::test]
    async fn test_rewind_command_opens_picker() {
        let mut app = headless_app().await;
        push_human_turn(&mut app, "帮我重构一下这个模块");
        RewindCommand.execute(&mut app, "");
        assert!(
            matches!(
                app.session_mgr.current().agent.interaction_prompt,
                Some(crate::app::InteractionPrompt::Rewind(_))
            ),
            "/rewind 应打开回滚选择器"
        );
    }

    #[tokio::test]
    async fn test_rewind_command_without_history_shows_note() {
        let mut app = headless_app().await;
        RewindCommand.execute(&mut app, "");
        assert!(
            app.session_mgr.current().agent.interaction_prompt.is_none(),
            "无历史时不应打开回滚选择器"
        );
        let text = format!(
            "{:?}",
            app.session_mgr.current_mut().messages.view_messages
        );
        assert!(
            text.contains("没有可回退的对话轮次")
                || text.contains("No conversation turns available to rewind"),
            "无历史时应提示没有可回退轮次，实际: {text}"
        );
    }

    #[tokio::test]
    async fn test_rewind_alias_undo_dispatches_to_picker() {
        let mut app = headless_app().await;
        push_human_turn(&mut app, "第一轮提问");
        let registry = crate::command::default_registry();
        assert!(
            registry.dispatch(&mut app, "/undo"),
            "/undo 应命中 rewind 命令"
        );
        assert!(
            matches!(
                app.session_mgr.current().agent.interaction_prompt,
                Some(crate::app::InteractionPrompt::Rewind(_))
            ),
            "/undo 应打开回滚选择器"
        );
    }

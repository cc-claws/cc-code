
    fn make_enter() -> Input {
        Input {
            key: Key::Enter,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    fn make_arrow(key: Key) -> Input {
        Input {
            key,
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

    /// 运行中候选浮层必须可导航：↑/↓ 移动 `hint_cursor`，而不是去滚消息区。
    ///
    /// 回归的是「面板照常渲染、按键却全被 `!loading` 挡掉」的不对称——用户看到
    /// 一个能看不能选的列表。对齐 CC `useTypeahead.tsx`：候选存在即接管 ↑↓。
    #[tokio::test]
    async fn test_busy_hint_overlay_navigates_with_arrows() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/");
        let count = app.hint_candidates_count();
        assert!(count > 1, "前提：`/` 应有多个候选，实际 {count}");

        handle_normal_keys(&mut app, make_arrow(Key::Down)).expect("按键处理成功");
        assert_eq!(
            app.session_mgr.current().ui.hint_cursor,
            Some(0),
            "运行中按 ↓ 应选中第一个候选"
        );

        handle_normal_keys(&mut app, make_arrow(Key::Down)).expect("按键处理成功");
        assert_eq!(
            app.session_mgr.current().ui.hint_cursor,
            Some(1),
            "运行中按 ↓ 应移到下一个候选"
        );

        handle_normal_keys(&mut app, make_arrow(Key::Up)).expect("按键处理成功");
        assert_eq!(
            app.session_mgr.current().ui.hint_cursor,
            Some(0),
            "运行中按 ↑ 应移回上一个候选"
        );
    }

    /// 运行中 Tab 必须走补全导航，不能落进通用输入分支插入字面 Tab 字符。
    ///
    /// 改前 `if !loading => handle_tab(app)` 守卫失败后没有其他 Tab 分支接住，
    /// 会一路落到 `textarea.input(input)`。
    #[tokio::test]
    async fn test_busy_tab_navigates_hint_without_inserting_literal_tab() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/");
        let before = input_text(&app);

        handle_normal_keys(&mut app, make_arrow(Key::Tab)).expect("按键处理成功");

        assert_eq!(
            input_text(&app),
            before,
            "Tab 不得把字面 Tab 字符写进输入框"
        );
        assert_eq!(
            app.session_mgr.current().ui.hint_cursor,
            Some(0),
            "运行中 Tab 应选中第一个候选"
        );
    }

    /// 运行中「先选中再 Enter」补全，补全不提交；再按一次 Enter 才走运行中输入路径。
    ///
    /// 第二下 Enter 落到 `handle_busy_input`：可延后者入队、需空闲的本地命令被拒绝并
    /// 保留输入——两条都算「离开了补全路径」。这里不写死其中一条，因为首个候选是
    /// 哪条命令取决于注册表排序。
    #[tokio::test]
    async fn test_busy_enter_completes_selected_hint_then_busy_input_on_second_enter() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/");

        // 选中候选
        handle_normal_keys(&mut app, make_arrow(Key::Down)).expect("按键处理成功");
        assert!(
            app.session_mgr.current().ui.hint_cursor.is_some(),
            "按 ↓ 后应有选中项"
        );

        // 第一下 Enter：补全文本，不入队
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        assert_eq!(queued_count(&app), 0, "补全本身不得把消息送出去");
        let completed = input_text(&app);
        assert!(
            completed.starts_with('/') && completed.ends_with(' '),
            "补全后应是 `/命令名 ` 形态，实际: {completed:?}"
        );
        assert_eq!(
            app.session_mgr.current().ui.hint_cursor,
            None,
            "补全后应清空选中态"
        );

        // 第二下 Enter：走 handle_busy_input（入队或被拒绝并保留输入）
        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");
        let queued = queued_count(&app);
        // 拒绝提示带 `$command` 插值，`tr()` 取不到带参版本，改用前缀匹配。
        let note_prefix = app
            .services
            .lc
            .tr("command-busy-rejected")
            .split('{')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let rejected = !note_prefix.is_empty() && notes_text(&app).contains(&note_prefix);
        assert!(
            queued == 1 || rejected,
            "补全后再按 Enter 应走运行中输入路径（入队或拒绝），实际 queued={queued}, notes={}",
            notes_text(&app)
        );
        assert_eq!(
            input_text(&app),
            if queued == 1 {
                String::new()
            } else {
                completed.clone()
            },
            "入队应清空输入；被拒绝应保留补全后的文本"
        );
    }

    /// 运行中未选中候选时，Enter 保持 #533 语义：直接入队/拒绝，不被补全劫持。
    #[tokio::test]
    async fn test_busy_enter_without_selection_keeps_queue_semantics() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/recap");

        handle_normal_keys(&mut app, make_enter()).expect("按键处理成功");

        assert_eq!(queued_count(&app), 1, "/recap 应直接入队，不被补全劫持");
    }

    /// Esc 先关浮层（保留输入文本），不中断 agent；再按一次才中断。
    ///
    /// 对齐 CC `useRegisterOverlay('autocomplete', ...)`：autocomplete 是 overlay，
    /// ESC 属于它，不该连带中断正在跑的轮次。
    #[tokio::test]
    async fn test_busy_esc_dismisses_hint_before_interrupting() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/");

        handle_normal_keys(&mut app, make_arrow(Key::Esc)).expect("按键处理成功");

        assert_eq!(app.hint_candidates_count(), 0, "Esc 应关掉候选浮层");
        assert_eq!(input_text(&app), "/", "关闭浮层不得清掉用户输入");
        assert!(
            app.session_mgr.current().ui.loading,
            "Esc 关浮层时不得中断正在跑的轮次"
        );

        // 文本不变时浮层不该自己弹回来
        assert_eq!(app.hint_candidates_count(), 0, "文本未变不应重新弹出");

        // 再按一次 Esc：此时没有浮层，走中断分支
        handle_normal_keys(&mut app, make_arrow(Key::Esc)).expect("按键处理成功");
        assert!(
            !app.session_mgr.current().ui.loading,
            "浮层已关时再按 Esc 应中断轮次"
        );
    }

    /// 编辑输入文本后，Esc 的抑制位失效，浮层重新出现。
    #[tokio::test]
    async fn test_hint_reappears_after_editing_dismissed_input() {
        let mut app = make_busy_app().await;
        set_input(&mut app, "/");
        handle_normal_keys(&mut app, make_arrow(Key::Esc)).expect("按键处理成功");
        assert_eq!(app.hint_candidates_count(), 0, "Esc 后应无候选");

        handle_normal_keys(
            &mut app,
            Input {
                key: Key::Char('m'),
                ctrl: false,
                alt: false,
                shift: false,
            },
        )
        .expect("按键处理成功");

        assert!(
            app.hint_candidates_count() > 0,
            "继续编辑后浮层应重新出现（对齐 CC `dismissedForInputRef`）"
        );
    }

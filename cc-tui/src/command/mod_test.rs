

    // ── StubCommand ──

    struct StubCommand {
        n: &'static str,
        called: Arc<AtomicBool>,
        last_args: Arc<parking_lot::Mutex<String>>,
        aliases_vec: Vec<&'static str>,
    }

    impl Command for StubCommand {
        fn name(&self) -> &str {
            self.n
        }
        fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
            "stub".to_string()
        }
        fn aliases(&self) -> Vec<&str> {
            self.aliases_vec.clone()
        }
        fn execute(&self, _app: &mut App, args: &str) {
            self.called.store(true, Ordering::Relaxed);
            *self.last_args.lock() = args.to_string();
        }
    }

    fn make_lc() -> crate::i18n::LcRegistry {
        crate::i18n::LcRegistry::default()
    }

    fn make_stub(
        name: &'static str,
    ) -> (
        StubCommand,
        Arc<AtomicBool>,
        Arc<parking_lot::Mutex<String>>,
    ) {
        make_stub_with_aliases(name, vec![])
    }

    fn make_stub_with_aliases(
        name: &'static str,
        aliases: Vec<&'static str>,
    ) -> (
        StubCommand,
        Arc<AtomicBool>,
        Arc<parking_lot::Mutex<String>>,
    ) {
        let called = Arc::new(AtomicBool::new(false));
        let last_args = Arc::new(parking_lot::Mutex::new(String::new()));
        (
            StubCommand {
                n: name,
                called: called.clone(),
                last_args: last_args.clone(),
                aliases_vec: aliases,
            },
            called,
            last_args,
        )
    }

    async fn headless_app() -> App {
        App::new_headless(80, 24).await.0
    }

    // ── dispatch 精确匹配 ──

    #[tokio::test]
    async fn test_dispatch_exact_match() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app().await;
        assert!(
            r.dispatch(&mut app, "/model"),
            "exact match should return true"
        );
        assert!(called.load(Ordering::Relaxed), "command should be called");
    }

    #[tokio::test]
    async fn test_dispatch_no_match() {
        let mut r = CommandRegistry::new();
        let (stub, _, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app().await;
        assert!(
            !r.dispatch(&mut app, "/unknown"),
            "unknown command should return false"
        );
    }

    // ── 前缀唯一匹配 ──

    #[tokio::test]
    async fn test_dispatch_prefix_unique() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app().await;
        assert!(
            r.dispatch(&mut app, "/mo"),
            "unique prefix should return true"
        );
        assert!(
            called.load(Ordering::Relaxed),
            "command should be called via prefix"
        );
    }

    #[tokio::test]
    async fn test_dispatch_prefix_ambiguous() {
        let mut r = CommandRegistry::new();
        let (stub1, called1, _) = make_stub("model");
        let (stub2, called2, _) = make_stub("mock");
        r.register(Box::new(stub1));
        r.register(Box::new(stub2));
        let mut app = headless_app().await;
        assert!(
            !r.dispatch(&mut app, "/m"),
            "ambiguous prefix should return false"
        );
        assert!(!called1.load(Ordering::Relaxed));
        assert!(!called2.load(Ordering::Relaxed));
    }

    // ── 参数传递 ──

    #[tokio::test]
    async fn test_dispatch_with_args() {
        let mut r = CommandRegistry::new();
        let (stub, _, last_args) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app().await;
        r.dispatch(&mut app, "/model opus");
        assert_eq!(*last_args.lock(), "opus", "args should be passed correctly");
    }

    // ── 辅助方法（纯逻辑，无需 App）──

    #[test]
    fn test_match_prefix_returns_matching() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("model");
        let (s2, _, _) = make_stub("mock");
        let (s3, _, _) = make_stub("clear");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        r.register(Box::new(s3));
        let matches = r.match_prefix("mo", &make_lc());
        assert_eq!(matches.len(), 2, "should match 'model' and 'mock'");
    }

    #[test]
    fn test_list_returns_all() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("a");
        let (s2, _, _) = make_stub("b");
        let (s3, _, _) = make_stub("c");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        r.register(Box::new(s3));
        assert_eq!(r.list(&make_lc()).len(), 3, "list should return all 3 commands");
    }

    #[tokio::test]
    async fn test_dispatch_empty_prefix() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("model");
        let (s2, _, _) = make_stub("clear");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        let mut app = headless_app().await;
        // "/" → empty name, all commands match → ambiguous → false
        assert!(
            !r.dispatch(&mut app, "/"),
            "empty prefix should return false when ambiguous"
        );
    }

    // ── 别名匹配 ──

    #[tokio::test]
    async fn test_alias_exact_match() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub_with_aliases("clear", vec!["reset", "new"]);
        r.register(Box::new(stub));
        let mut app = headless_app().await;
        assert!(
            r.dispatch(&mut app, "/reset"),
            "alias exact match should return true"
        );
        assert!(called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_alias_no_match() {
        let mut r = CommandRegistry::new();
        let (stub, _, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app().await;
        assert!(
            !r.dispatch(&mut app, "/reset"),
            "no alias should return false"
        );
    }

    #[tokio::test]
    async fn test_name_priority_over_alias() {
        let mut r = CommandRegistry::new();
        let (s1, called1, _) = make_stub("reset");
        let (s2, called2, _) = make_stub_with_aliases("clear", vec!["reset"]);
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        let mut app = headless_app().await;
        assert!(r.dispatch(&mut app, "/reset"));
        assert!(called1.load(Ordering::Relaxed), "name exact should win");
        assert!(!called2.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_alias_prefix_match() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub_with_aliases("clear", vec!["reset"]);
        r.register(Box::new(stub));
        let mut app = headless_app().await;
        assert!(
            r.dispatch(&mut app, "/res"),
            "alias prefix unique match should return true"
        );
        assert!(called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_alias_prefix_ambiguous() {
        let mut r = CommandRegistry::new();
        let (s1, called1, _) = make_stub_with_aliases("clear", vec!["reset"]);
        let (s2, called2, _) = make_stub("real");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        let mut app = headless_app().await;
        assert!(
            !r.dispatch(&mut app, "/re"),
            "ambiguous alias prefix should return false"
        );
        assert!(!called1.load(Ordering::Relaxed));
        assert!(!called2.load(Ordering::Relaxed));
    }

    #[test]
    fn test_match_prefix_covers_aliases() {
        let mut r = CommandRegistry::new();
        let (s, _, _) = make_stub_with_aliases("clear", vec!["reset"]);
        r.register(Box::new(s));
        let matches = r.match_prefix("res", &make_lc());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, "clear");
    }

    #[test]
    fn test_list_includes_aliases() {
        let mut r = CommandRegistry::new();
        let (s, _, _) = make_stub_with_aliases("clear", vec!["reset", "new"]);
        r.register(Box::new(s));
        let list = r.list(&make_lc());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "clear");
        assert_eq!(list[0].2, vec!["reset".to_string(), "new".to_string()]);
    }

    #[test]
    fn test_no_alias_backward_compat() {
        let mut r = CommandRegistry::new();
        let (s, _, _) = make_stub("model");
        r.register(Box::new(s));
        let list = r.list(&make_lc());
        assert_eq!(list[0].2, Vec::<String>::new());
        let matches = r.match_prefix("mo", &make_lc());
        assert_eq!(matches.len(), 1);
    }

    // ── 回归：default_registry 必须包含 passthrough 命令及其别名 ──
    // 历史 bug：commit/review 只在 ACP 层注册，TUI registry 遗漏，
    // 导致用户输入 /commit /review 时报"未知命令"。此测试防止再次遗漏。
    #[test]
    fn test_default_registry_includes_passthrough_commands() {
        let r = crate::command::default_registry();
        let list = r.list(&make_lc());
        let names: Vec<&str> = list.iter().map(|(n, _, _)| n.as_str()).collect();
        // init / commit / review 均为 Passthrough，TUI 必须注册才能透传给 ACP；recap 为 Immediate
        assert!(names.contains(&"init"), "default_registry 应包含 init");
        assert!(names.contains(&"commit"), "default_registry 应包含 commit");
        assert!(names.contains(&"review"), "default_registry 应包含 review");
        assert!(names.contains(&"recap"), "default_registry 应包含 recap");
        // 别名也必须暴露给补全/Hints
        let commit_entry = list.iter().find(|(n, _, _)| *n == "commit").unwrap();
        assert!(
            commit_entry.2.iter().any(|a| a == "ci"),
            "commit 应含 ci 别名"
        );
        let review_entry = list.iter().find(|(n, _, _)| *n == "review").unwrap();
        assert!(
            review_entry.2.iter().any(|a| a == "pr"),
            "review 应含 pr 别名"
        );
        let recap_entry = list.iter().find(|(n, _, _)| *n == "recap").unwrap();
        assert!(
            recap_entry.2.iter().any(|a| a == "away"),
            "recap 应含 away 别名"
        );
        assert!(
            recap_entry.2.iter().any(|a| a == "catchup"),
            "recap 应含 catchup 别名"
        );
        // rewind 走 TUI registry（打开回滚选择器），不再是 ACP 独占
        assert!(names.contains(&"rewind"), "default_registry 应包含 rewind");
        let rewind_entry = list.iter().find(|(n, _, _)| *n == "rewind").unwrap();
        assert!(
            rewind_entry.2.iter().any(|a| a == "undo"),
            "rewind 应含 undo 别名"
        );
        // ps 命令：无别名
        assert!(names.contains(&"ps"), "default_registry 应包含 ps");
        let ps_entry = list.iter().find(|(n, _, _)| *n == "ps").unwrap();
        assert_eq!(ps_entry.2, Vec::<String>::new(), "ps 命令不设别名");
    }

    // ── find：纯判定，必须与 dispatch 的匹配优先级一致 ──

    #[test]
    fn test_find_follows_dispatch_precedence() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("reset");
        let (s2, _, _) = make_stub_with_aliases("clear", vec!["reset"]);
        let (s3, _, _) = make_stub("model");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        r.register(Box::new(s3));
        // 精确名优先于别名
        assert_eq!(r.find("reset").map(|c| c.name()), Some("reset"));
        // 别名次之
        assert_eq!(r.find("model").map(|c| c.name()), Some("model"));
        assert_eq!(r.find("mo").map(|c| c.name()), Some("model"));
        assert!(r.find("nope").is_none());
    }

    #[test]
    fn test_find_returns_none_for_ambiguous_prefix() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("model");
        let (s2, _, _) = make_stub("mock");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        assert!(r.find("m").is_none(), "歧义前缀不应命中任何命令");
        assert_eq!(r.prefix_names("m"), vec!["model", "mock"]);
    }

    /// 回归：判定（`find`）与执行（`dispatch`）必须对同一输入给出一致的结论。
    #[tokio::test]
    async fn test_find_agrees_with_dispatch() {
        let mut r = CommandRegistry::new();
        let (s1, called1, _) = make_stub("model");
        let (s2, called2, _) = make_stub_with_aliases("clear", vec!["reset"]);
        // 再放一个同前缀命令，让 "/m" 成为歧义前缀
        let (s3, _, _) = make_stub("mock");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        r.register(Box::new(s3));
        let mut app = headless_app().await;
        for (input, expected) in [
            ("/model", true),
            ("/reset", true),
            ("/nope", false),
            ("/m", false),
        ] {
            let name = input.trim_start_matches('/');
            let found = r.find(name).is_some();
            assert_eq!(found, expected, "find({name}) 判定与预期不符");
            assert_eq!(
                r.dispatch(&mut app, input),
                found,
                "dispatch({input}) 与 find 判定不一致"
            );
        }
        assert!(called1.load(Ordering::Relaxed));
        assert!(called2.load(Ordering::Relaxed));
    }

    // ── blocked_while_busy：运行中能否延后执行 ──

    /// 回归：内建命令的「运行中阻塞」分类是有意为之的白名单。
    ///
    /// 只有 `execute` 仅做 `submit_message` 转发的命令才允许运行中入队（`false`）；
    /// 需要 UI 稳定态的命令（开面板 / 弹选择器 / 改本地状态 / 退出 / 追加通知）必须
    /// 阻塞，否则运行中输入会被静默当成普通消息发给模型。新增命令若漏了 override，
    /// 本测试会失败提醒。
    #[test]
    fn test_builtin_commands_blocked_while_busy_classification() {
        let r = crate::command::default_registry();
        let lc = make_lc();
        let mut deferrable: Vec<String> = Vec::new();
        for (name, _, _) in r.list(&lc) {
            let cmd = r.find(&name).expect("注册表里的命令必须能被 find 命中");
            if !cmd.blocked_while_busy() {
                deferrable.push(name);
            }
        }
        deferrable.sort();
        assert_eq!(
            deferrable,
            vec!["commit", "init", "recap", "review", "streaming"],
            "运行中可延后执行的命令白名单发生变化，请确认新命令的 execute 是否只做 submit_message"
        );
    }

    #[tokio::test]
    async fn test_ps_command_executes_while_busy_and_opens_panel() {
        use crate::app::panel_manager::PanelKind;
        let mut app = headless_app().await;
        // 模拟 Agent 处于运行中状态
        app.session_mgr.current_mut().ui.loading = true;
        app.session_mgr
            .current_mut()
            .ui
            .textarea
            .insert_str("/ps");

        // Act: 运行中执行 /ps 命令
        app.handle_busy_input("/ps".to_string());

        // Assert: 立即打开后台任务面板，不进排队队列，不被拒绝
        assert!(
            app.global_panels.is_active(PanelKind::BackgroundTasks),
            "运行中执行 /ps 必须立即打开 BackgroundTasksPanel"
        );
        assert!(
            app.session_mgr
                .current()
                .ui
                .textarea
                .lines()
                .join("")
                .is_empty(),
            "执行完成后输入框应已被清空"
        );
        assert!(
            app.session_mgr
                .current()
                .messages
                .pending_messages
                .is_empty(),
            "/ps 不得被排队延后"
        );
    }

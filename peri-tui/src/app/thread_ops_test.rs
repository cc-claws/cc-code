    fn make_thread(cwd: &str) -> ThreadMeta {
        ThreadMeta::new(cwd)
    }

    #[test]
    fn filter_keeps_matching_cwd() {
        let cwd = "/Users/alice/project";
        let threads = vec![
            make_thread(cwd),
            make_thread("/Users/alice/other"),
            make_thread(cwd),
        ];
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_returns_empty_when_no_match() {
        let cwd = "/Users/alice/project";
        let threads = vec![
            make_thread("/Users/alice/other"),
            make_thread("/Users/bob/project"),
        ];
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_keeps_all_when_all_match() {
        let cwd = "/Users/alice/project";
        let threads = vec![make_thread(cwd), make_thread(cwd), make_thread(cwd)];
        let filtered: Vec<_> = threads.into_iter().filter(|t| t.cwd == cwd).collect();
        assert_eq!(filtered.len(), 3);
    }

    #[tokio::test]
    async fn scroll_up_from_follow_starts_at_bottom() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.scrollbar_max_offset = 30;
        app.session_mgr.current_mut().ui.scroll_offset = u16::MAX;
        app.session_mgr.current_mut().ui.scroll_follow = true;

        app.scroll_up();

        assert_eq!(
            app.session_mgr.current().ui.scroll_offset,
            27,
            "follow 模式下首次滚轮上滚应从底部向上移动，而不是被下一帧吸回底部"
        );
        assert!(
            !app.session_mgr.current().ui.scroll_follow,
            "用户主动上滚后应退出 follow 模式"
        );
    }

    #[tokio::test]
    async fn scroll_down_to_bottom_restores_follow() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.scrollbar_max_offset = 30;
        app.session_mgr.current_mut().ui.scroll_offset = 28;
        app.session_mgr.current_mut().ui.scroll_follow = false;

        app.scroll_down();

        assert_eq!(app.session_mgr.current().ui.scroll_offset, 30);
        assert!(
            app.session_mgr.current().ui.scroll_follow,
            "滚动到底部后应恢复 follow 模式，保持新消息自动跟随"
        );
    }

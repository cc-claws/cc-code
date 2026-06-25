    use super::App;

    fn make_thread(cwd: &str) -> ThreadMeta {
        ThreadMeta::new(cwd)
    }

    fn make_app() -> App {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(App::new())
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

    #[test]
    fn test_scroll_up_from_follow_starts_at_bottom() {
        let mut app = make_app();
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.scroll_follow = true;
        ui.scrollbar_min_offset = 10;
        ui.scrollbar_max_offset = 20;
        app.scroll_up();
        let ui = &app.session_mgr.current().ui;
        assert_eq!(ui.scroll_offset, 17);
        assert!(!ui.scroll_follow);
    }

    #[test]
    fn test_scroll_down_to_bottom_restores_follow() {
        let mut app = make_app();
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.scroll_follow = false;
        ui.scroll_offset = 18;
        ui.scrollbar_min_offset = 10;
        ui.scrollbar_max_offset = 20;
        app.scroll_down();
        let ui = &app.session_mgr.current().ui;
        assert_eq!(ui.scroll_offset, 20);
        assert!(ui.scroll_follow);
    }

    #[test]
    fn test_scroll_up_does_not_cross_native_scrollback_boundary() {
        let mut app = make_app();
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.scroll_follow = false;
        ui.scroll_offset = 11;
        ui.scrollbar_min_offset = 10;
        ui.scrollbar_max_offset = 20;
        app.scroll_up();
        assert_eq!(app.session_mgr.current().ui.scroll_offset, 10);
    }

    #[test]
    fn test_scroll_to_top_stops_at_native_scrollback_boundary() {
        let mut app = make_app();
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.scrollbar_min_offset = 10;
        ui.scrollbar_max_offset = 20;
        app.scroll_to_top();
        let ui = &app.session_mgr.current().ui;
        assert_eq!(ui.scroll_offset, 10);
        assert!(!ui.scroll_follow);
    }

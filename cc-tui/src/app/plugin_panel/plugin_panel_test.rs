    fn make_entry(id: &str, name: &str, enabled: bool) -> PluginEntry {
        PluginEntry {
            id: id.into(),
            name: name.into(),
            plugin_type: PluginItemType::Plugin,
            marketplace: "test".into(),
            enabled,
            scope: InstallScope::User,
            version: "1.0.0".into(),
            install_path: std::path::PathBuf::new(),
            project_path: None,
            load_error: None,
            description: String::new(),
            author: None,
            commands: vec![],
            skills: vec![],
            agents: vec![],
            mcp_servers: vec![],
        }
    }

    #[test]
    fn test_plugin_panel_new() {
        let panel = PluginPanel::new(vec![]);
        assert_eq!(panel.cursor(), 0);
        assert_eq!(panel.view, PluginPanelView::Installed);
        assert!(panel.confirm_delete.is_none());
    }

    #[test]
    fn test_plugin_panel_move_cursor() {
        let mut panel = PluginPanel::new(vec![
            make_entry("a@test", "a", true),
            make_entry("b@test", "b", true),
            make_entry("c@test", "c", true),
        ]);
        // 上移不越界
        for _ in 0..5 {
            panel.installed_list.move_cursor(-1);
        }
        assert_eq!(panel.cursor(), 0);
        // 下移到末尾
        for _ in 0..5 {
            panel.installed_list.move_cursor(1);
        }
        assert_eq!(panel.cursor(), 2);
    }

    #[test]
    fn test_plugin_panel_tab_cycles_views() {
        let mut panel = PluginPanel::new(vec![]);
        panel.view.next();
        assert_eq!(panel.view, PluginPanelView::Discover);
        panel.view.next();
        assert_eq!(panel.view, PluginPanelView::Marketplaces);
        panel.view.next();
        assert_eq!(panel.view, PluginPanelView::Errors);
        panel.view.next();
        assert_eq!(panel.view, PluginPanelView::Installed);
    }

    #[test]
    fn test_plugin_panel_shift_tab_cycles_back() {
        let mut panel = PluginPanel::new(vec![]);
        panel.view.prev();
        assert_eq!(panel.view, PluginPanelView::Errors);
        panel.view.prev();
        assert_eq!(panel.view, PluginPanelView::Marketplaces);
    }

    #[test]
    fn test_plugin_panel_request_cancel_delete() {
        let mut panel = PluginPanel::new(vec![make_entry("my-plugin@test", "my-plugin", true)]);
        // 请求删除
        if let Some(entry) = panel.selected_entry() {
            panel.confirm_delete = Some(entry.id.clone());
        }
        assert_eq!(panel.confirm_delete, Some("my-plugin@test".into()));
        // 取消删除
        panel.confirm_delete = None;
        assert!(panel.confirm_delete.is_none());
    }

    #[test]
    fn test_plugin_panel_toggle_enabled() {
        let mut panel = PluginPanel::new(vec![make_entry("p@test", "p", true)]);
        assert!(panel.entries[0].enabled);
        panel.entries[0].enabled = !panel.entries[0].enabled;
        assert!(!panel.entries[0].enabled);
        panel.entries[0].enabled = !panel.entries[0].enabled;
        assert!(panel.entries[0].enabled);
    }

    #[test]
    fn test_plugin_panel_errors_view() {
        let mut entry = make_entry("bad@t", "bad-plugin", true);
        entry.load_error = Some("missing manifest".into());
        let panel = PluginPanel::new(vec![make_entry("good@t", "good-plugin", true), entry]);
        // Installed 视图：2 条
        assert_eq!(panel.current_list_len(), 2);
        // Errors 视图过滤：只有 load_error 的条目
        assert_eq!(panel.visible_indices().len(), 2);
        let error_count = panel.entries.iter().filter(|e| e.load_error.is_some()).count();
        assert_eq!(error_count, 1);
    }

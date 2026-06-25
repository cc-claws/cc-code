    #[test]
    fn test_memory_panel_new_entries() {
        let cwd = if cfg!(windows) {
            "C:\\test\\project"
        } else {
            "/test/project"
        };
        let home = if cfg!(windows) {
            "C:\\Users\\user"
        } else {
            "/home/user"
        };
        let panel = MemoryPanel::new(cwd, Some(PathBuf::from(home)));
        assert_eq!(panel.entries.len(), 2);
        assert_eq!(panel.entries[0].label, "项目说明");
        assert_eq!(panel.entries[1].label, "用户全局");
        assert_eq!(panel.entries[0].path, PathBuf::from(cwd).join("CLAUDE.md"));
        assert_eq!(
            panel.entries[1].path,
            PathBuf::from(home).join(".claude").join("CLAUDE.md")
        );
    }

    #[test]
    fn test_memory_panel_cursor_navigation() {
        let mut panel = MemoryPanel::new("/test", None);
        assert_eq!(panel.cursor(), 0);
        panel.list.move_cursor(1);
        assert_eq!(panel.cursor(), 1);
        panel.list.move_cursor(1); // 不再下移
        assert_eq!(panel.cursor(), 1);
        panel.list.move_cursor(-1);
        assert_eq!(panel.cursor(), 0);
        panel.list.move_cursor(-1); // 不再上移
        assert_eq!(panel.cursor(), 0);
    }

    #[test]
    fn test_memory_panel_refresh_exists() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_memory_panel_exists.md");
        std::fs::write(&temp_file, "test").ok();

        let mut panel = MemoryPanel::new("/test", None);
        // 手动设置一个条目的路径到临时文件
        panel.entries[0].path = temp_file.clone();
        panel.refresh_exists();
        assert!(panel.entries[0].exists);
        assert!(!panel.entries[1].exists);

        std::fs::remove_file(&temp_file).ok();
    }

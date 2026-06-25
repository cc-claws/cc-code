    fn make_hook(event: HookEvent, matcher: Option<&str>) -> RegisteredHook {
        let hook_type: HookType = serde_json::from_value(serde_json::json!({
            "type": "command",
            "command": "echo test"
        }))
        .unwrap();
        RegisteredHook {
            hook: hook_type,
            event,
            matcher: matcher.map(String::from),
            plugin_name: "test".to_string(),
            plugin_id: "test".to_string(),
            plugin_root: PathBuf::from("/tmp"),
            plugin_data_dir: PathBuf::from("/tmp"),
            plugin_options: HashMap::new(),
        }
    }

    #[test]
    fn test_cursor_line_basic() {
        // 3 entries, cursor on 0 → header(3) + 0 = 3
        let hooks = vec![
            make_hook(HookEvent::PreToolUse, None),
            make_hook(HookEvent::PostToolUse, None),
            make_hook(HookEvent::Stop, None),
        ];
        let panel = HooksPanel::new(hooks);
        assert_eq!(panel.cursor_line(), 3); // header=3, cursor=0 → line 3
    }

    #[test]
    fn test_cursor_line_middle() {
        // 3 entries, cursor on 1 → header(3) + 1 = 4
        let hooks = vec![
            make_hook(HookEvent::PreToolUse, None),
            make_hook(HookEvent::PostToolUse, None),
            make_hook(HookEvent::Stop, None),
        ];
        let mut panel = HooksPanel::new(hooks);
        panel.list.move_cursor(1);
        assert_eq!(panel.cursor_line(), 4); // header=3, cursor=1 → line 4
    }

    #[test]
    fn test_expanded_lines_with_matcher() {
        let hook_type: HookType = serde_json::from_value(serde_json::json!({
            "type": "command",
            "command": "echo test"
        }))
        .unwrap();
        let hook = RegisteredHook {
            hook: hook_type,
            event: HookEvent::PreToolUse,
            matcher: Some("Bash".to_string()),
            plugin_name: "test".to_string(),
            plugin_id: "test".to_string(),
            plugin_root: PathBuf::from("/tmp"),
            plugin_data_dir: PathBuf::from("/tmp"),
            plugin_options: HashMap::new(),
        };
        let panel = HooksPanel::new(vec![hook]);
        // 1 event header + 1 detail(type+summary=1, matcher=1, plugin=1 → 3) + 1 empty = 5
        assert_eq!(panel.expanded_lines(), 5);
    }

    #[test]
    fn test_expanded_lines_without_matcher() {
        let hooks = vec![make_hook(HookEvent::PreToolUse, None)];
        let panel = HooksPanel::new(hooks);
        // 1 event header + 1 detail(type+summary=1, no matcher, plugin=1 → 2) + 1 empty = 4
        assert_eq!(panel.expanded_lines(), 4);
    }

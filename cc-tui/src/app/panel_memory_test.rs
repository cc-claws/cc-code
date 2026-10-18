    /// #587 回归：从外部编辑器返回后必须请求一次物理清屏 + 全量重绘。
    ///
    /// ratatui 的 `Terminal` 是差值绘制：离开再重入 alternate screen 后屏幕已是空白，
    /// back buffer 却仍假设上一帧在屏上，下一帧只 flush 差异 cell 就会留下大片残缺，
    /// 只能靠 `terminal.clear()`（`draw_app` 消费 `force_terminal_clear_redraw`）纠正。
    #[tokio::test]
    async fn test_memory_panel_open_editor_requests_terminal_clear_redraw() {
        use crate::app::memory_panel::MemoryPanel;
        use crate::app::panel_manager::PanelState;
        use std::io::IsTerminal;

        let (mut app, _handle) = App::new_headless(100, 30).await;

        // 临时目录当 cwd，避免触碰仓库里真实的 CLAUDE.md
        let dir = std::env::temp_dir().join(format!("cc-memory-editor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("创建临时目录");
        let path = dir.join("CLAUDE.md");
        std::fs::write(&path, "# fixture\n").expect("写临时文件");

        let mut panel = MemoryPanel::new(
            dir.to_str().expect("临时目录路径可转 UTF-8"),
            None,
            &crate::i18n::LcRegistry::default(),
        );
        panel.entries[0].path = path.clone();
        panel.refresh_exists();
        app.open_panel(PanelState::Memory(panel));

        // 起点必须干净，否则下面的断言可能被旧值满足
        assert!(
            !app.session_mgr.current_mut().ui.take_terminal_clear_redraw(),
            "初始不应有清屏请求"
        );

        // EDITOR 指向立即退出的命令，测试不进入真实编辑器
        std::env::set_var("EDITOR", "true");
        let result = app.memory_panel_open_editor();
        std::env::remove_var("EDITOR");
        // 测试进程不保留 raw mode（有 tty 时恢复阶段会真的把它打开）
        let _ = ratatui::crossterm::terminal::disable_raw_mode();

        // 无 tty（CI / headless）时恢复阶段的 enable_raw_mode 必然失败并提前返回，
        // 这正是把置位放到挂起之后的原因：屏幕状态已不可信就必须重绘。
        if std::io::stdin().is_terminal() {
            result.expect("有 tty 时应能完整挂起并恢复 TUI");
        }

        assert!(
            app.session_mgr.current().ui.force_terminal_clear_redraw,
            "离开 alternate screen 后必须请求清屏重绘，否则返回后残屏（#587）"
        );
        assert!(
            app.session_mgr.current_mut().ui.take_terminal_clear_redraw(),
            "清屏请求应能被 draw_app 消费一次"
        );
        assert!(
            !app.session_mgr.current_mut().ui.take_terminal_clear_redraw(),
            "消费后必须归零，避免每帧清屏闪烁"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

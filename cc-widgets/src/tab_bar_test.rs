    #[test]
    fn tab_state_navigation() {
        let mut state = TabState::new(vec!["A".into(), "B".into(), "C".into()]);
        state.next();
        assert_eq!(state.active(), 1);
        state.prev();
        assert_eq!(state.active(), 0);
        state.prev();
        assert_eq!(state.active(), 2); // wraps around
    }

    #[test]
    fn tab_state_indicator() {
        let mut state = TabState::new(vec!["A".into(), "B".into(), "C".into()]);
        state.set_indicator(1, Some('✓'));
        assert_eq!(state.indicator(1), Some('✓'));
        assert_eq!(state.indicator(0), None);
    }

    #[test]
    fn tab_bar_render() {
        let backend = TestBackend::new(30, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = TabState::new(vec!["Tab1".into(), "Tab2".into()]);
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 30, 3);
                f.render_stateful_widget(TabBar::new(), area, &mut state);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let row: String = (0..30)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(
            row.contains("Tab1"),
            "Expected Tab1 in output, got: {:?}",
            row
        );
        assert!(
            row.contains("Tab2"),
            "Expected Tab2 in output, got: {:?}",
            row
        );
    }

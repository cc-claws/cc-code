    #[test]
    fn radio_state_select() {
        let mut state = RadioState::new();
        state.select(2);
        assert_eq!(state.selected(), Some(2));
    }

    #[test]
    fn radio_state_cursor_clamp() {
        let mut state = RadioState::new();
        state.move_cursor(5, 3);
        assert_eq!(state.cursor(), 2);
        state.move_cursor(-10, 3);
        assert_eq!(state.cursor(), 0);
    }

    #[test]
    fn radio_group_render() {
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = RadioState::new();
        state.select(0);
        let options = vec![
            RadioOption::new("Option A"),
            RadioOption::new("Option B").description("desc"),
        ];
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 30, 5);
                f.render_stateful_widget(RadioGroup::new(options), area, &mut state);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let row0: String = (0..30)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(
            row0.contains("●"),
            "Expected filled marker, got: {:?}",
            row0
        );
        let row1: String = (0..30)
            .map(|x| buf.cell((x, 1)).unwrap().symbol().to_string())
            .collect();
        assert!(row1.contains("○"), "Expected empty marker, got: {:?}", row1);
    }

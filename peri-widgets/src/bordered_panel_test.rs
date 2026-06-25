    #[test]
    fn render_returns_inner_area() {
        let backend = TestBackend::new(10, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 10, 6);
        let mut inner = Rect::default();
        terminal
            .draw(|f| {
                inner = BorderedPanel::new("Title")
                    .border_style(Style::default())
                    .render(f, area);
            })
            .unwrap();
        // inner width = 10 (no left/right borders)
        assert_eq!(inner.width, 10);
        // inner height = 6 - 2 (top + bottom borders) = 4
        assert_eq!(inner.height, 4);
    }

    #[test]
    fn render_with_empty_title() {
        let backend = TestBackend::new(10, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 10, 6);
        let mut inner = Rect::default();
        terminal
            .draw(|f| {
                inner = BorderedPanel::new("")
                    .border_style(Style::default())
                    .render(f, area);
            })
            .unwrap();
        assert_eq!(inner.width, 10);
        assert_eq!(inner.height, 4);
    }

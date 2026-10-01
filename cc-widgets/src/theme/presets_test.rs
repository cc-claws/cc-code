    #[test]
    fn dark_theme_returns_correct_colors() {
        let theme = DarkTheme;
        assert_eq!(theme.accent(), Color::Rgb(215, 119, 87));
    }

    #[test]
    fn dark_theme_trait_object_usable() {
        let theme: &dyn Theme = &DarkTheme;
        let _accent = theme.accent();
        let _success = theme.success();
        let _warning = theme.warning();
        let _error = theme.error();
        let _thinking = theme.thinking();
        let _text = theme.text();
        let _muted = theme.muted();
        let _dim = theme.dim();
        let _border = theme.border();
        let _border_active = theme.border_active();
        let _popup_bg = theme.popup_bg();
        let _cursor_bg = theme.cursor_bg();
        let _loading = theme.loading();
    }

    #[test]
    fn dark_theme_cloneable() {
        let theme = DarkTheme;
        let _cloned = theme.clone();
    }

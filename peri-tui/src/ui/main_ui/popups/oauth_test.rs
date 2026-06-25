    #[tokio::test]
    async fn test_render_oauth_popup_shows_url() {
        let (mut app, mut handle) = crate::app::App::new_headless(80, 30).await;
        let (tx, _rx) = tokio::sync::oneshot::channel();
        app.global_ui.oauth_prompt = Some(crate::app::OAuthPrompt::new(
            "test-server".into(),
            "http://auth.example.com/authorize".into(),
            tx,
        ));
        handle
            .terminal
            .draw(|f| render_oauth_popup(f, &mut app, ratatui::layout::Rect::new(0, 0, 80, 9)))
            .unwrap();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("example.com"),
            "OAuth popup should show authorization URL domain"
        );
    }

    #[tokio::test]
    async fn test_render_oauth_popup_shows_error() {
        let (mut app, mut handle) = crate::app::App::new_headless(80, 30).await;
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut prompt =
            crate::app::OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.error_message = Some("parse error".to_string());
        app.global_ui.oauth_prompt = Some(prompt);
        handle
            .terminal
            .draw(|f| render_oauth_popup(f, &mut app, ratatui::layout::Rect::new(0, 0, 80, 9)))
            .unwrap();
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("parse error"),
            "OAuth popup should show error message"
        );
    }

    #[test]
    fn test_oauth_prompt_new() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let prompt = OAuthPrompt::new("test-server".into(), "http://example.com/auth".into(), tx);
        assert!(prompt.input.is_empty());
        assert_eq!(prompt.cursor, 0);
        assert!(prompt.error_message.is_none());
        assert_eq!(prompt.server_name, "test-server");
    }

    #[test]
    fn test_oauth_prompt_submit_valid_url() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = "http://localhost:12345/callback?code=abc&state=xyz".to_string();
        assert!(prompt.submit());
        let result = rx.blocking_recv().unwrap();
        assert_eq!(result.code, "abc");
        assert_eq!(result.state, "xyz");
    }

    #[test]
    fn test_oauth_prompt_submit_full_url() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = "http://localhost:9999/callback?code=test_code&state=test_state".to_string();
        assert!(prompt.submit());
        let result = rx.blocking_recv().unwrap();
        assert_eq!(result.code, "test_code");
        assert_eq!(result.state, "test_state");
    }

    #[test]
    fn test_oauth_prompt_submit_invalid_url() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = "not a valid url".to_string();
        assert!(!prompt.submit());
        assert!(prompt.error_message.is_some());
    }

    #[test]
    fn test_oauth_prompt_submit_empty() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut prompt = OAuthPrompt::new("srv".into(), "http://auth.example.com".into(), tx);
        prompt.input = String::new();
        assert!(!prompt.submit());
        assert!(prompt.error_message.is_some());
    }

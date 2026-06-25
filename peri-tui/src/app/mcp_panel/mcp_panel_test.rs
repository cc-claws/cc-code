

    fn make_server_info(name: &str, status: ClientStatus) -> ServerInfo {
        ServerInfo {
            name: name.to_string(),
            transport_type: "stdio".to_string(),
            status,
            tool_count: 0,
            resource_count: 0,
            oauth_status: Default::default(),
            source: None,
            url: None,
            plugin_source: None,
        }
    }

    #[tokio::test]
    async fn test_mcp_panel_new() {
        let panel = McpPanel::new(vec![]);
        assert_eq!(panel.cursor(), 0);
        assert!(matches!(panel.view, McpPanelView::ServerList));
        assert!(panel.confirm_delete.is_none());

        let servers = vec![
            make_server_info("a", ClientStatus::Connected),
            make_server_info("b", ClientStatus::Failed("err".into())),
            make_server_info("c", ClientStatus::Connected),
        ];
        let panel = McpPanel::new(servers);
        assert_eq!(panel.servers.len(), 3);
    }

    #[tokio::test]
    async fn test_mcp_panel_move_cursor() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        let servers = vec![
            make_server_info("a", ClientStatus::Connected),
            make_server_info("b", ClientStatus::Connected),
            make_server_info("c", ClientStatus::Connected),
        ];
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Mcp(McpPanel::new(
                servers,
            )));

        for _ in 0..5 {
            app.mcp_panel_move_up();
        }
        assert_eq!(app.global_panels.get::<McpPanel>().unwrap().cursor(), 0);

        for _ in 0..5 {
            app.mcp_panel_move_down();
        }
        assert_eq!(app.global_panels.get::<McpPanel>().unwrap().cursor(), 2);
    }

    #[tokio::test]
    async fn test_mcp_panel_close() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Mcp(McpPanel::new(
                vec![],
            )));
        assert!(app.global_panels.is_active(crate::app::PanelKind::Mcp));
        app.mcp_panel_close();
        assert!(!app.global_panels.is_active(crate::app::PanelKind::Mcp));
    }

    #[tokio::test]
    async fn test_mcp_panel_request_cancel_delete() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        let servers = vec![make_server_info("test-srv", ClientStatus::Connected)];
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Mcp(McpPanel::new(
                servers,
            )));

        app.mcp_panel_request_delete();
        assert_eq!(
            app.global_panels.get::<McpPanel>().unwrap().confirm_delete,
            Some("test-srv".to_string())
        );

        app.mcp_panel_cancel_delete();
        assert!(app
            .global_panels
            .get::<McpPanel>()
            .unwrap()
            .confirm_delete
            .is_none());
    }

    #[tokio::test]
    async fn test_mcp_panel_enter_builds_actions() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        let mut srv = make_server_info("http-srv", ClientStatus::Connected);
        srv.transport_type = "http".to_string();
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Mcp(McpPanel::new(
                vec![srv],
            )));

        app.mcp_panel_enter();
        match &app.global_panels.get::<McpPanel>().unwrap().view {
            McpPanelView::ServerDetail { actions, .. } => {
                assert!(actions.contains(&DetailAction::ReAuthenticate));
                assert!(actions.contains(&DetailAction::ClearAuth));
                assert!(actions.contains(&DetailAction::Reconnect));
                assert!(actions.contains(&DetailAction::Disable));
            }
            _ => panic!("应进入 ServerDetail 视图"),
        }
    }

    #[tokio::test]
    async fn test_mcp_panel_back_restores_cursor() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        let servers = vec![
            make_server_info("a", ClientStatus::Connected),
            make_server_info("b", ClientStatus::Connected),
        ];
        app.global_panels
            .open(crate::app::panel_manager::PanelState::Mcp(McpPanel::new(
                servers,
            )));
        app.global_panels
            .get_mut::<McpPanel>()
            .unwrap()
            .server_list
            .move_cursor_to(1);
        app.mcp_panel_enter();
        app.mcp_panel_back();
        assert_eq!(app.global_panels.get::<McpPanel>().unwrap().cursor(), 1);
    }

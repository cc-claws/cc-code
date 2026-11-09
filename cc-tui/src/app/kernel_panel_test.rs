use super::*;
use cc_acp::transport::{mpsc::mpsc_transport_pair, types::IncomingMessage, AcpTransport};
use ratatui::{
    backend::TestBackend,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    Terminal,
};

use crate::{
    command::{kernel::KernelCommand, Command},
    event::{handle_event, Action},
};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[tokio::test]
async fn no_argument_opens_picker_at_current_session_kernel_and_routes_keys() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    app.services
        .peri_config
        .get_or_insert_with(Default::default)
        .config
        .agent_kernel = Some("cc".to_string());
    app.session_mgr.current_mut().metadata.kernel_id = Some("pi".to_string());
    let messages = app.session_mgr.current().messages.view_messages.len();

    KernelCommand.execute(&mut app, "");
    let panel = app
        .session_mgr
        .current()
        .session_panels
        .get::<KernelPanel>()
        .unwrap();
    assert_eq!(panel.list.items(), &["cc", "dsh", "pi"]);
    assert_eq!(panel.selected(), Some("pi"));
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        messages
    );

    assert!(matches!(
        handle_event(&mut app, key(KeyCode::Up)).await.unwrap(),
        Some(Action::Redraw)
    ));
    assert_eq!(
        app.session_mgr
            .current()
            .session_panels
            .get::<KernelPanel>()
            .unwrap()
            .selected(),
        Some("dsh")
    );
    assert_eq!(
        app.current_kernel_id(),
        "pi",
        "navigation must not switch kernels"
    );

    for _ in 0..3 {
        handle_event(&mut app, key(KeyCode::Up)).await.unwrap();
    }
    assert_eq!(
        app.session_mgr
            .current()
            .session_panels
            .get::<KernelPanel>()
            .unwrap()
            .selected(),
        Some("cc")
    );
    for _ in 0..4 {
        handle_event(&mut app, key(KeyCode::Down)).await.unwrap();
    }
    assert_eq!(
        app.session_mgr
            .current()
            .session_panels
            .get::<KernelPanel>()
            .unwrap()
            .selected(),
        Some("pi")
    );

    handle_event(&mut app, Event::Paste("ignored".to_string()))
        .await
        .unwrap();
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines(),
        &[String::new()]
    );
    handle_event(&mut app, key(KeyCode::Esc)).await.unwrap();
    assert!(!app.session_mgr.current().session_panels.is_any_open());
    assert_eq!(app.current_kernel_id(), "pi");
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        messages
    );
}

#[tokio::test]
async fn enter_uses_selected_kernel_and_waits_for_backend_acknowledgement() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    app.session_mgr.current_mut().metadata.kernel_id = Some("cc".to_string());
    let (client, server) = mpsc_transport_pair();
    let (client, _notifications) = crate::acp_client::AcpTuiClient::new(client);
    app.acp_client = Some(client);
    let messages = app.session_mgr.current().messages.view_messages.len();

    KernelCommand.execute(&mut app, "");
    handle_event(&mut app, key(KeyCode::Down)).await.unwrap();
    handle_event(&mut app, key(KeyCode::Enter)).await.unwrap();
    assert!(!app.session_mgr.current().session_panels.is_any_open());
    let request = tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
        .await
        .unwrap()
        .unwrap();
    let IncomingMessage::Request { id, method, params } = request else {
        panic!("expected a config request");
    };
    assert_eq!(method, "session/set_config_option");
    assert_eq!(params["configId"], "agent_kernel");
    assert_eq!(params["value"], "dsh");
    assert_eq!(app.current_kernel_id(), "cc");
    server
        .send_response(id, Ok(serde_json::json!({})))
        .await
        .unwrap();

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = app
                .services
                .bg_event_rx
                .as_mut()
                .unwrap()
                .recv()
                .await
                .unwrap();
            if matches!(
                event,
                super::super::AgentEvent::KernelCommandCompleted { .. }
            ) {
                break event;
            }
        }
    })
    .await
    .unwrap();
    app.handle_agent_event(event);
    assert_eq!(app.current_kernel_id(), "dsh");
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        messages
    );
    assert!(app
        .session_mgr
        .current()
        .ui
        .toast
        .as_ref()
        .unwrap()
        .0
        .contains("dsh"));
}

#[tokio::test]
async fn render_marks_current_kernel_independently_of_cursor() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    app.services
        .peri_config
        .get_or_insert_with(Default::default)
        .config
        .agent_kernel = Some("cc".to_string());
    app.session_mgr.current_mut().metadata.kernel_id = Some("dsh".to_string());
    let mut panel = KernelPanel::new(app.current_kernel_id());
    panel.list.move_cursor_to(0);
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            crate::ui::main_ui::panels::kernel::render_kernel_panel(frame, &panel, &mut app, area);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let lines: Vec<String> = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect();
    assert!(lines
        .iter()
        .any(|line| line.contains("2. dsh") && line.contains('✔')));
    assert!(lines
        .iter()
        .any(|line| line.contains("1. cc") && !line.contains('✔')));
    assert!(lines.iter().any(|line| line.contains("3. pi")));
    assert!(lines
        .iter()
        .any(|line| line.contains("Current kernel: dsh")));
}

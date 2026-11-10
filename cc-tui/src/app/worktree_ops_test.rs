use super::*;
use cc_agent::{messages::BaseMessage, thread::ThreadMeta};

fn tree(path: &Path, branch: &str) -> Worktree {
    Worktree {
        path: path.to_string_lossy().into_owned(),
        head: "abc123".into(),
        branch: Some(branch.into()),
        locked: false,
        prunable: false,
        bare: false,
    }
}

#[test]
fn worktree_selector_requires_registered_available_directory() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let mut list = WorktreeList {
        repository_root: source.path().to_string_lossy().into_owned(),
        worktrees: vec![tree(source.path(), "main"), tree(target.path(), "feature")],
    };
    assert_eq!(
        resolve_worktree(&list, "2").unwrap().branch.as_deref(),
        Some("feature")
    );
    assert_eq!(
        resolve_worktree(&list, "feature").unwrap().path,
        target.path().to_string_lossy()
    );
    assert!(resolve_worktree(&list, &target.path().to_string_lossy()).is_ok());
    assert!(resolve_worktree(&list, "missing").is_err());
    list.worktrees[1].prunable = true;
    assert!(resolve_worktree(&list, "2").is_err());
    list.worktrees[1].prunable = false;
    list.worktrees[1].bare = true;
    assert!(resolve_worktree(&list, "2").is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_activation_rebuilds_project_context_without_changing_process_cwd() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let target = tempfile::tempdir().unwrap();
    let cwd = app.services.cwd.clone();
    let process_cwd = std::env::current_dir().unwrap();
    let session_id = app.session_mgr.current().metadata.session_id;
    app.session_mgr
        .current_mut()
        .metadata
        .worktree_request_pending = true;
    app.handle_worktree_completed(
        session_id,
        cwd,
        Ok(WorktreeOutcome::Activate {
            worktree: tree(target.path(), "feature"),
            thread_id: None,
            created: true,
        }),
    );
    assert!(same_directory(
        &app.services.cwd,
        &target.path().to_string_lossy()
    ));
    assert_eq!(
        app.session_mgr.current().messages.pipeline.cwd(),
        app.services.cwd
    );
    assert_ne!(app.session_mgr.current().metadata.session_id, session_id);
    assert!(!app.session_mgr.current().metadata.worktree_request_pending);
    assert_eq!(std::env::current_dir().unwrap(), process_cwd);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_switch_restores_target_history() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let target = tempfile::tempdir().unwrap();
    let cwd = target.path().to_string_lossy().into_owned();
    let id = app
        .services
        .thread_store
        .create_thread(ThreadMeta::new(&cwd))
        .await
        .unwrap();
    app.services
        .thread_store
        .append_message(&id, BaseMessage::human("target history"))
        .await
        .unwrap();
    let source_cwd = app.services.cwd.clone();
    let session_id = app.session_mgr.current().metadata.session_id;
    app.handle_worktree_completed(
        session_id,
        source_cwd,
        Ok(WorktreeOutcome::Activate {
            worktree: tree(target.path(), "feature"),
            thread_id: Some(id.clone()),
            created: false,
        }),
    );
    assert_eq!(
        app.session_mgr.current().current_thread_id.as_deref(),
        Some(id.as_str())
    );
    assert_eq!(app.services.cwd, cwd);
    assert!(!app.session_mgr.current().messages.view_messages.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_stale_or_busy_completion_keeps_source_context_and_checkout() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let target = tempfile::tempdir().unwrap();
    let source = app.services.cwd.clone();
    let session_id = app.session_mgr.current().metadata.session_id;
    app.handle_worktree_completed(
        uuid::Uuid::now_v7(),
        source.clone(),
        Ok(WorktreeOutcome::Activate {
            worktree: tree(target.path(), "feature"),
            thread_id: None,
            created: true,
        }),
    );
    assert_eq!(app.services.cwd, source);
    app.session_mgr.current_mut().ui.loading = true;
    app.session_mgr
        .current_mut()
        .metadata
        .worktree_request_pending = true;
    app.handle_worktree_completed(
        session_id,
        source.clone(),
        Ok(WorktreeOutcome::Activate {
            worktree: tree(target.path(), "feature"),
            thread_id: None,
            created: true,
        }),
    );
    assert_eq!(app.services.cwd, source);
    assert!(target.path().is_dir());
    assert!(!app.session_mgr.current().metadata.worktree_request_pending);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_rpc_waits_for_acknowledgement_and_activates_created_checkout() {
    use cc_acp::transport::{mpsc::mpsc_transport_pair, types::IncomingMessage, AcpTransport};
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let target = tempfile::tempdir().unwrap();
    let original = app.services.cwd.clone();
    let (client, server) = mpsc_transport_pair();
    let (client, _notifications) = crate::acp_client::AcpTuiClient::new(client);
    app.acp_client = Some(client);
    app.request_worktree(WorktreeRequest::Create {
        branch: Some("feat/demo".into()),
        base_ref: Some("main".into()),
    });
    assert!(app.session_mgr.current().metadata.worktree_request_pending);
    assert!(!app.can_switch_thread_context());
    assert_eq!(app.services.cwd, original);
    let IncomingMessage::Request { id, method, params } =
        tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("expected request")
    };
    assert_eq!(method, "workspace/worktree/create");
    assert_eq!(params["cwd"], original);
    assert_eq!(params["branch"], "feat/demo");
    assert_eq!(params["baseRef"], "main");
    server
        .send_response(
            id,
            Ok(serde_json::to_value(tree(target.path(), "feat/demo")).unwrap()),
        )
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
            if matches!(event, AgentEvent::WorktreeCompleted { .. }) {
                break event;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(app.services.cwd, original);
    app.handle_agent_event(event);
    assert!(same_directory(
        &app.services.cwd,
        &target.path().to_string_lossy()
    ));
}

#[tokio::test]
async fn worktree_failure_releases_pending_state_and_preserves_context() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let cwd = app.services.cwd.clone();
    let session_id = app.session_mgr.current().metadata.session_id;
    app.session_mgr
        .current_mut()
        .metadata
        .worktree_request_pending = true;
    app.handle_worktree_completed(session_id, cwd.clone(), Err("invalid reference".into()));
    assert_eq!(app.services.cwd, cwd);
    assert!(!app.session_mgr.current().metadata.worktree_request_pending);
}

#[tokio::test]
async fn worktree_input_arriving_during_creation_stays_in_source_session() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let cwd = app.services.cwd.clone();
    let session_id = app.session_mgr.current().metadata.session_id;
    let target = tempfile::tempdir().unwrap();
    app.session_mgr
        .current_mut()
        .metadata
        .worktree_request_pending = true;
    app.submit_message("keep this input in the source checkout".to_string());
    assert_eq!(app.session_mgr.current().messages.pending_messages.len(), 1);
    app.handle_worktree_completed(
        session_id,
        cwd.clone(),
        Ok(WorktreeOutcome::Activate {
            worktree: tree(target.path(), "feature"),
            thread_id: None,
            created: true,
        }),
    );
    assert_eq!(app.services.cwd, cwd);
    assert_eq!(app.session_mgr.current().messages.pending_messages.len(), 1);
    assert!(target.path().is_dir());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_picker_keyboard_switches_through_acp() {
    use cc_acp::transport::{mpsc::mpsc_transport_pair, types::IncomingMessage, AcpTransport};
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let source_cwd = source.path().to_string_lossy().into_owned();
    assert!(app.new_thread_in_cwd(source_cwd.clone()));
    let list = WorktreeList {
        repository_root: source_cwd.clone(),
        worktrees: vec![tree(source.path(), "main"), tree(target.path(), "feature")],
    };
    let session_id = app.session_mgr.current().metadata.session_id;
    app.handle_worktree_completed(
        session_id,
        source_cwd,
        Ok(WorktreeOutcome::Browse(list.clone())),
    );
    for key in [KeyCode::Down, KeyCode::Down] {
        crate::event::handle_event(&mut app, Event::Key(KeyEvent::new(key, KeyModifiers::NONE)))
            .await
            .unwrap();
    }
    assert_eq!(
        app.session_mgr
            .current()
            .session_panels
            .get::<WorktreePanel>()
            .unwrap()
            .list
            .cursor(),
        2
    );
    let (client, server) = mpsc_transport_pair();
    let (client, _notifications) = crate::acp_client::AcpTuiClient::new(client);
    app.acp_client = Some(client);
    crate::event::handle_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    assert!(!app.session_mgr.current().session_panels.is_any_open());
    let IncomingMessage::Request { id, method, .. } =
        tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("expected worktree list request")
    };
    assert_eq!(method, "workspace/worktrees");
    server
        .send_response(id, Ok(serde_json::to_value(list).unwrap()))
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
            if matches!(event, AgentEvent::WorktreeCompleted { .. }) {
                break event;
            }
        }
    })
    .await
    .unwrap();
    app.handle_agent_event(event);
    assert!(same_directory(
        &app.services.cwd,
        &target.path().to_string_lossy()
    ));
}

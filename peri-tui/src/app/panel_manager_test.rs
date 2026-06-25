use super::*;

#[test]
fn test_panel_kind_scope() {
    assert_eq!(PanelKind::Model.scope(), PanelScope::Session);
    assert_eq!(PanelKind::Login.scope(), PanelScope::Session);
    assert_eq!(PanelKind::Agent.scope(), PanelScope::Session);
    assert_eq!(PanelKind::Hooks.scope(), PanelScope::Session);
    assert_eq!(PanelKind::Config.scope(), PanelScope::Session);
    assert_eq!(PanelKind::ThreadBrowser.scope(), PanelScope::Session);
    assert_eq!(PanelKind::Mcp.scope(), PanelScope::Global);
    assert_eq!(PanelKind::Plugin.scope(), PanelScope::Global);
    assert_eq!(PanelKind::Cron.scope(), PanelScope::Global);
    assert_eq!(PanelKind::Status.scope(), PanelScope::Global);
    assert_eq!(PanelKind::Memory.scope(), PanelScope::Global);
}

#[test]
fn test_panel_kind_priority_unique() {
    use std::collections::HashSet;
    let priorities: HashSet<u8> = [
        PanelKind::Agent,
        PanelKind::Hooks,
        PanelKind::Model,
        PanelKind::Login,
        PanelKind::Config,
        PanelKind::ThreadBrowser,
        PanelKind::Mcp,
        PanelKind::Plugin,
        PanelKind::Cron,
        PanelKind::Status,
        PanelKind::Memory,
    ]
    .iter()
    .map(|k| k.priority())
    .collect();
    assert_eq!(
        priorities.len(),
        11,
        "All 11 PanelKind variants must have unique priorities"
    );
    assert!(
        priorities.iter().all(|&p| p <= 10),
        "Priorities should be in range 0-10"
    );
}

#[test]
fn test_panel_state_kind_roundtrip() {
    // CronPanel::new(tasks) 简单构造
    let state = PanelState::Cron(CronPanel::new(vec![]));
    assert_eq!(state.kind(), PanelKind::Cron);
}

#[test]
fn test_panel_manager_new_is_empty() {
    let mgr = PanelManager::new();
    assert!(!mgr.is_any_open());
    assert_eq!(mgr.active_kind(), None);
}

#[test]
fn test_panel_manager_open_close() {
    let mut mgr = PanelManager::new();
    mgr.open(PanelState::Cron(CronPanel::new(vec![])));
    assert!(mgr.is_active(PanelKind::Cron));
    assert!(mgr.is_any_open());
    mgr.close();
    assert!(!mgr.is_any_open());
    assert!(!mgr.is_active(PanelKind::Cron));
}

#[test]
fn test_event_result_variants() {
    let _ = EventResult::Consumed;
    let _ = EventResult::NotConsumed;
    let _ = EventResult::ClosePanel;
    let _ = EventResult::OpenPanel(PanelKind::Model);
    let _ = EventResult::OpenThread(String::new());
}

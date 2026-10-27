use super::*;
use cc_agent::agent::events::CompactFileInfo;

#[tokio::test]
async fn test_auto_compact_started_sets_compact_manual_false() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.handle_compact_started(false);
    assert!(
        !app.session_mgr.current().agent.compact_manual,
        "auto-compact 不应将 compact_manual 标记为 true"
    );
    assert!(
        app.session_mgr.current().ui.loading,
        "auto-compact 开始时应进入 loading 态"
    );
}

#[tokio::test]
async fn test_auto_compact_completed_keeps_existing_view_messages() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.apply_pipeline_action(PipelineAction::add_message(MessageViewModel::system(
        "历史消息".to_string(),
    )));
    app.handle_compact_started(false);
    let before = app.session_mgr.current().messages.view_messages.len();
    app.handle_compact_completed("summary".to_string(), vec![], vec![], 0, vec![]);
    let vms = &app.session_mgr.current().messages.view_messages;
    assert_eq!(
        vms.len(),
        before + 1,
        "自动 compact 后已显示历史必须保留，只追加一条压缩通知"
    );
    assert_eq!(
        app.session_mgr.current().messages.round_start_vm_idx,
        vms.len(),
        "自动 compact 后重建起点应推到视图末尾，后续只重建新增部分"
    );
}

#[tokio::test]
async fn test_manual_compact_completed_clears_view_messages() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.apply_pipeline_action(PipelineAction::add_message(MessageViewModel::system(
        "历史消息".to_string(),
    )));
    app.handle_compact_started(true);
    app.handle_compact_completed("summary".to_string(), vec![], vec![], 0, vec![]);
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        1,
        "手动 compact 后视图应重置为只剩压缩通知"
    );
    assert_eq!(
        app.session_mgr.current().messages.round_start_vm_idx,
        0,
        "手动 compact 后重建起点归零"
    );
}

#[tokio::test]
async fn test_auto_compact_completed_preserves_loading_true() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.handle_compact_started(false);
    app.handle_compact_completed(
        "summary".to_string(),
        vec![CompactFileInfo {
            path: "src/main.rs".to_string(),
            lines: 10,
        }],
        vec!["skill-a".to_string()],
        0,
        vec![],
    );
    assert!(
        app.session_mgr.current().ui.loading,
        "auto-compact 完成后应保留 loading 态（agent 继续执行）"
    );
    assert!(
        !app.session_mgr.current().agent.compact_manual,
        "auto-compact 完成后 compact_manual 仍应为 false"
    );
}

#[tokio::test]
async fn test_manual_compact_started_sets_compact_manual_true() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.handle_compact_started(true);
    assert!(
        app.session_mgr.current().agent.compact_manual,
        "手动 /compact 命令必须标记 compact_manual 为 true"
    );
    assert!(
        app.session_mgr.current().ui.loading,
        "手动 compact 开始时应进入 loading 态"
    );
}

#[tokio::test]
async fn test_manual_compact_completed_resets_loading_false() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.handle_compact_started(true);
    app.handle_compact_completed("summary".to_string(), vec![], vec![], 0, vec![]);
    assert!(
        !app.session_mgr.current().ui.loading,
        "手动 compact 完成后必须结束 loading 态"
    );
    assert!(
        !app.session_mgr.current().agent.compact_manual,
        "手动 compact 完成后必须复位 compact_manual"
    );
}

#[tokio::test]
async fn test_auto_compact_error_preserves_loading_true() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.handle_compact_started(false);
    app.handle_compact_error("model context overflow".to_string());
    assert!(
        app.session_mgr.current().ui.loading,
        "auto-compact 失败后应保持 loading 态供 agent 错误/终止流程处理"
    );
    assert!(
        !app.session_mgr.current().agent.compact_manual,
        "auto-compact 失败后 compact_manual 仍为 false"
    );
}

#[tokio::test]
async fn test_manual_compact_error_resets_loading_false() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.handle_compact_started(true);
    app.handle_compact_error("manual compact failed".to_string());
    assert!(
        !app.session_mgr.current().ui.loading,
        "手动 compact 失败后必须结束 loading 态"
    );
    assert!(
        !app.session_mgr.current().agent.compact_manual,
        "手动 compact 失败后必须复位 compact_manual"
    );
}

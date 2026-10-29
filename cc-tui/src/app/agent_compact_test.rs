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

/// 视图历史超限时裁掉最老的一段，并插一行占位；重建起点随之平移。
#[tokio::test]
async fn test_trim_view_history_drops_oldest_with_placeholder() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    for i in 0..10 {
        app.apply_pipeline_action(PipelineAction::add_message(MessageViewModel::system(
            format!("历史 {i}"),
        )));
    }
    app.session_mgr.current_mut().messages.round_start_vm_idx = 10;

    app.trim_view_history(4);

    let vms = &app.session_mgr.current().messages.view_messages;
    assert_eq!(vms.len(), 5, "应保留最近 4 条 + 1 行占位");
    let placeholder = format!("{:?}", vms[0]);
    assert!(
        placeholder.contains("已省略") && placeholder.contains('6'),
        "首行应是省略提示（裁掉 6 条），实际: {placeholder}"
    );
    assert_eq!(
        app.session_mgr.current().messages.round_start_vm_idx,
        5,
        "重建起点应平移到新的视图末尾"
    );
}

/// 未超限时不做任何改动。
#[tokio::test]
async fn test_trim_view_history_noop_within_limit() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.apply_pipeline_action(PipelineAction::add_message(MessageViewModel::system(
        "历史".to_string(),
    )));
    app.trim_view_history(10);
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        1,
        "未超限不应裁剪"
    );
}

/// 压缩进行中不再往消息流插「正在进行」提示，改由 spinner 承担；
/// 完成后 spinner 文案必须还原成普通动词。
#[tokio::test]
async fn test_compact_started_uses_spinner_instead_of_view_message() {
    // Arrange
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let before = app.session_mgr.current().messages.view_messages.len();
    let expected = app.services.lc.tr("app-compact-started");
    // Act
    app.handle_compact_started(false);
    // Assert
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        before,
        "压缩开始不应写入消息流"
    );
    assert_eq!(
        app.session_mgr.current().spinner_state.verb(),
        expected,
        "spinner 应显示压缩文案"
    );
    app.handle_compact_completed("summary".to_string(), vec![], vec![], 0, vec![]);
    assert_ne!(
        app.session_mgr.current().spinner_state.verb(),
        expected,
        "压缩完成后 spinner 不应停留在压缩文案"
    );
}

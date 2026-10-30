use super::*;

fn make_message_state() -> (MessageState, mpsc::Receiver<RenderEvent>) {
    let (sender, receiver) = mpsc::channel(1);
    let state = MessageState::new(
        ".".into(),
        sender,
        Arc::new(RwLock::new(RenderCache::new())),
        Arc::new(Notify::new()),
    );
    (state, receiver)
}

#[test]
fn test_render_mode_retry_precedes_final_snapshot_and_invalidates_layout() {
    let (mut state, mut receiver) = make_message_state();
    state.send_render_snapshot(RenderEvent::Rebuild(Vec::new()));
    let original_epoch = state.layout_epoch();
    assert!(state
        .send_render_event(RenderEvent::ToggleHookRows(false))
        .is_err());
    assert!(state
        .send_render_event(RenderEvent::ToggleDetail(true))
        .is_err());
    state.send_render_snapshot(RenderEvent::Clear);
    assert_eq!(state.layout_epoch(), original_epoch, "未入队不能确认新布局");
    receiver.try_recv().expect("移除旧内容");
    assert!(state.retry_render_rebuild());
    assert!(matches!(
        receiver.try_recv().expect("先同步 hook 开关"),
        RenderEvent::ToggleHookRows(false)
    ));
    assert_eq!(state.layout_epoch(), original_epoch + 1);
    assert!(state.retry_render_rebuild());
    assert!(matches!(
        receiver.try_recv().expect("再同步详细模式"),
        RenderEvent::ToggleDetail(true)
    ));
    assert!(state.retry_render_rebuild());
    assert!(matches!(
        receiver.try_recv().expect("最后应用内容快照"),
        RenderEvent::Clear
    ));
    assert!(!state.retry_render_rebuild(), "发送成功后不能重复追发");
}

#[test]
fn test_render_snapshot_does_not_overtake_pending_mode_when_queue_becomes_available() {
    let (mut state, mut receiver) = make_message_state();
    state.send_render_snapshot(RenderEvent::Clear);
    assert!(state
        .send_render_event(RenderEvent::ToggleDetail(true))
        .is_err());
    receiver.try_recv().expect("渲染线程腾出队列容量");
    state.send_render_snapshot(RenderEvent::RebuildWithAnchor {
        messages: vec![MessageViewModel::system("带锚点的内容".into())],
        anchor: crate::ui::render_thread::ViewportAnchor {
            message_index: 0,
            ..Default::default()
        },
        scroll_revision: 0,
    });
    assert!(receiver.try_recv().is_err(), "快照不能越过待发送模式");
    assert!(state.retry_render_rebuild());
    assert!(matches!(
        receiver.try_recv().expect("模式必须先于锚点入队"),
        RenderEvent::ToggleDetail(true)
    ));
    assert!(state.retry_render_rebuild());
    assert!(matches!(
        receiver.try_recv().expect("随后应用带锚点的快照"),
        RenderEvent::RebuildWithAnchor { anchor, messages, .. } if anchor.message_index == 0 && messages.len() == 1
    ));
}

#[test]
fn test_render_mode_retry_coalesces_latest_values_with_bounded_storage() {
    let (mut state, mut receiver) = make_message_state();
    state.send_render_snapshot(RenderEvent::Clear);
    for index in 0..1000 {
        assert!(state
            .send_render_event(RenderEvent::ToggleHookRows(index % 2 == 0))
            .is_err());
    }
    assert_eq!(state.pending_render_modes.borrow().len(), 1);
    receiver.try_recv().expect("移除旧内容");
    assert!(state.retry_render_rebuild());
    assert!(matches!(
        receiver.try_recv().expect("只发送最后的开关值"),
        RenderEvent::ToggleHookRows(false)
    ));
    assert!(!state.retry_render_rebuild());
}

#[test]
fn test_render_mode_success_replaces_pending_value_and_closed_channel_releases_it() {
    let (mut state, mut receiver) = make_message_state();
    state.send_render_snapshot(RenderEvent::Clear);
    assert!(state
        .send_render_event(RenderEvent::ToggleDiff(false))
        .is_err());
    receiver.try_recv().expect("移除旧内容");
    assert!(state
        .send_render_event(RenderEvent::ToggleDiff(true))
        .is_ok());
    assert!(state.pending_render_modes.borrow().is_empty());
    receiver.try_recv().expect("移除已发送的新状态");
    assert!(!state.retry_render_rebuild(), "不能重放旧的关闭状态");
    state.send_render_snapshot(RenderEvent::Clear);
    assert!(state
        .send_render_event(RenderEvent::SetDetailStrategy("virtual".into()))
        .is_err());
    drop(receiver);
    assert!(!state.retry_render_rebuild());
    assert!(state.pending_render_modes.borrow().is_empty());
}

#[test]
fn test_render_mode_queue_retry_retains_atomic_viewport_and_coalesces_by_payload() {
    let (mut state, mut receiver) = make_message_state();
    state.send_render_snapshot(RenderEvent::Clear);
    for revision in 1..=3 {
        let event = RenderEvent::WithViewport {
            viewport: crate::ui::render_thread::RenderViewport {
                top: 100 - revision as usize,
                follow_tail: false,
                scroll_revision: revision,
                ..Default::default()
            },
            event: Box::new(RenderEvent::ToggleDetail(revision % 2 == 0)),
        };
        assert!(state.send_render_event(event).is_err());
    }
    assert_eq!(
        state.pending_render_modes.borrow().len(),
        1,
        "包装后仍按模式类型合并"
    );
    receiver.try_recv().expect("释放队列容量");
    assert!(state.retry_render_rebuild());
    match receiver.try_recv().expect("重试完整请求") {
        RenderEvent::WithViewport { viewport, event } => {
            assert_eq!(viewport.top, 97);
            assert_eq!(viewport.scroll_revision, 3);
            assert!(matches!(*event, RenderEvent::ToggleDetail(false)));
        }
        _ => panic!("队列满不能拆散重排与阅读位置"),
    }
}

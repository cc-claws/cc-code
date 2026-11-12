use super::*;
use crate::ui::message_view::{ToolCategory, ToolEntry};
use crate::ui::render_layout::Viewport;

fn make_hidden_read() -> MessageViewModel {
    let mut message = MessageViewModel::ToolCallGroup {
        thinking: None,
        category: ToolCategory::Read,
        tools: vec![ToolEntry {
            tool_name: "Read".into(),
            display_name: "Read".into(),
            args_display: None,
            content: "success".into(),
            is_error: false,
            file_inspect: false,
        }],
        collapsed: true,
        content_hash: 0,
        standalone_action: None,
    };
    message.recompute_hash();
    message
}

async fn make_barrier(tx: &mpsc::Sender<RenderEvent>, width: u16) {
    let (reply, ready) = tokio::sync::oneshot::channel();
    tx.send(RenderEvent::ResizeAndWait(width, reply))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), ready)
        .await
        .unwrap()
        .unwrap();
}

async fn make_auto_history() -> (mpsc::Sender<RenderEvent>, Arc<RwLock<RenderCache>>) {
    let (tx, cache, _) = spawn_render_thread(80);
    tx.send(RenderEvent::Viewport {
        top: 0,
        height: 30,
        follow_tail: true,
        anchor: None,
        scroll_revision: 0,
    })
    .await
    .unwrap();
    let mut messages = vec![
        MessageViewModel::user("a".repeat(7_800)),
        MessageViewModel::user("b".repeat(4_056)),
    ];
    messages.extend((0..298).map(|_| make_hidden_read()));
    tx.send(RenderEvent::Rebuild(messages)).await.unwrap();
    make_barrier(&tx, 80).await;
    assert_eq!(
        cache
            .read()
            .virtual_layout
            .as_ref()
            .unwrap()
            .message_visual_offsets[1],
        3
    );
    tx.send(RenderEvent::Viewport {
        top: 3,
        height: 30,
        follow_tail: false,
        anchor: Some(ViewportAnchor {
            message_index: 1,
            ..Default::default()
        }),
        scroll_revision: 1,
    })
    .await
    .unwrap();
    make_barrier(&tx, 80).await;
    assert!(
        cache
            .read()
            .virtual_layout
            .as_ref()
            .unwrap()
            .diagnostics
            .estimated_segments
            >= 1,
        "尾窗不能预先实测第一条长消息"
    );
    (tx, cache)
}

fn make_steps(
    base_anchor: ViewportAnchor,
    deltas: &[isize],
    prefix_rows: usize,
    base_prefix_visible: usize,
    trailing_rows: usize,
    event: RenderEvent,
) -> RenderEvent {
    let steps: Arc<[(u64, isize)]> = deltas
        .iter()
        .enumerate()
        .map(|(i, delta)| (i as u64 + 2, *delta))
        .collect();
    RenderEvent::WithRelativeScroll {
        viewport: Viewport {
            top: 0,
            height: 30,
            follow_tail: false,
            anchor: None,
            scroll_revision: deltas.len() as u64 + 1,
        },
        base_anchor,
        prefix_rows,
        base_prefix_visible,
        trailing_rows,
        full_height: 30,
        steps,
        event: Box::new(event),
    }
}

#[tokio::test]
async fn test_relative_steps_preserve_bursts_and_real_boundary_reversals() {
    for (deltas, index, line, follow, trailing) in [
        (vec![-3, -3], 0, 95, false, 0),
        (vec![-3, -3, 3], 0, 98, false, 0),
        (vec![-10_000, 3], 0, 3, false, 0),
        (vec![10_000, -3], 1, 19, false, 0),
        (vec![10_000], 1, 22, true, 0),
        (vec![10_000], 1, 27, true, 5),
    ] {
        let (tx, cache) = make_auto_history().await;
        tx.send(make_steps(
            ViewportAnchor {
                message_index: 1,
                ..Default::default()
            },
            &deltas,
            0,
            0,
            trailing,
            RenderEvent::Resize(80),
        ))
        .await
        .unwrap();
        make_barrier(&tx, 80).await;
        let resolved = cache
            .read()
            .scroll_anchor
            .expect("每组步骤必须发布精确锚点");
        assert_eq!(
            (
                resolved.position.message_index,
                resolved.position.logical_line
            ),
            (index, line),
            "步骤={deltas:?}"
        );
        assert_eq!(resolved.follow_tail, follow, "只有向下抵达真实底边才能跟底");
        assert_eq!(resolved.position.relative_rows, 0);
    }
}

#[tokio::test]
async fn test_relative_steps_keep_welcome_prefix_and_reverse_at_true_top() {
    for (deltas, prefix_rows, base_prefix_visible, expected_visible, expected_line) in [
        (vec![-3, 3], 10, 0, 0, 0),
        (vec![-3, -3, 3], 10, 10, 7, 0),
        (vec![-3, -3, 3], 0, 0, 0, 3),
    ] {
        let (tx, cache) = make_auto_history().await;
        tx.send(make_steps(
            ViewportAnchor::default(),
            &deltas,
            prefix_rows,
            base_prefix_visible,
            0,
            RenderEvent::Resize(80),
        ))
        .await
        .unwrap();
        make_barrier(&tx, 80).await;
        let resolved = cache.read().scroll_anchor.unwrap();
        assert_eq!(resolved.prefix_visible, expected_visible);
        assert_eq!(resolved.position.message_index, 0);
        assert_eq!(
            resolved.position.logical_line, expected_line,
            "跨欢迎卡边界后不能额外前进三行"
        );
    }
}

#[tokio::test]
async fn test_relative_steps_replayed_after_mode_or_width_change_are_consumed_once() {
    let (tx, cache, _) = spawn_render_thread(80);
    tx.send(RenderEvent::SetDetailStrategy("virtual".into()))
        .await
        .unwrap();
    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::system_with_detail(
            "a\nb\nc".into(),
            (0..101).map(|i| format!("detail {i}\n")).collect(),
        ),
        MessageViewModel::user("b".repeat(4_056)),
    ]))
    .await
    .unwrap();
    make_barrier(&tx, 80).await;
    let base = ViewportAnchor {
        message_index: 1,
        ..Default::default()
    };
    tx.send(make_steps(
        base,
        &[-3],
        0,
        0,
        0,
        RenderEvent::ToggleDetail(true),
    ))
    .await
    .unwrap();
    make_barrier(&tx, 80).await;
    let expected = cache.read().scroll_anchor.unwrap().position;
    assert_eq!((expected.message_index, expected.logical_line), (0, 1));
    tx.send(make_steps(base, &[-3], 0, 0, 0, RenderEvent::Resize(79)))
        .await
        .unwrap();
    make_barrier(&tx, 79).await;
    assert_eq!(cache.read().scroll_anchor.unwrap().position, expected);
    tx.send(RenderEvent::WithViewport {
        viewport: Viewport {
            top: 0,
            height: 30,
            follow_tail: false,
            anchor: Some(ViewportAnchor {
                relative_rows: -3,
                ..base
            }),
            scroll_revision: 2,
        },
        event: Box::new(RenderEvent::Resize(80)),
    })
    .await
    .unwrap();
    make_barrier(&tx, 80).await;
    assert_eq!(
        cache.read().scroll_anchor.unwrap().position,
        expected,
        "旧 WithViewport 不能重新解释已消费步骤"
    );
}

#[tokio::test]
async fn test_relative_steps_survive_a_strategy_change_before_the_ui_acknowledges_it() {
    let (tx, cache) = make_auto_history().await;
    tx.send(RenderEvent::SetDetailStrategy("cache".into()))
        .await
        .unwrap();
    make_barrier(&tx, 80).await;
    assert!(cache.read().virtual_layout.is_none());
    // UI 仍持有旧虚拟窗口取样的步骤，经典 worker 也要执行原始位移。
    tx.send(make_steps(
        ViewportAnchor {
            message_index: 1,
            ..Default::default()
        },
        &[-3, -3, 3],
        0,
        0,
        0,
        RenderEvent::Resize(80),
    ))
    .await
    .unwrap();
    make_barrier(&tx, 80).await;
    let resolved = cache.read().scroll_anchor.unwrap();
    assert_eq!(
        (
            resolved.position.message_index,
            resolved.position.logical_line
        ),
        (0, 98)
    );
}

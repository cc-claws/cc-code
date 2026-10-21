use super::*;
use crate::{
    app::MessageViewModel,
    ui::{
        main_ui::message_area::render_messages,
        render_thread::{RenderEvent, VirtualLayoutSnapshot, WrappedLineInfo},
    },
};
use ratatui::crossterm::event::MouseEvent;
use ratatui::{layout::Rect, text::Line};
use std::sync::Arc;

fn make_window(app: &mut App, start: usize) {
    let mut cache = app.session_mgr.current().messages.render_cache.write();
    cache.width = 79;
    cache.total_lines = 1_000;
    cache.lines = (start..start + 20)
        .map(|row| Line::from(format!("Window {row}")))
        .collect();
    cache.wrap_map = (start..start + 20)
        .enumerate()
        .map(|(local, row)| {
            let plain_text = format!("Window {row}");
            WrappedLineInfo {
                line_idx: local,
                visual_row_start: row,
                visual_row_end: row + 1,
                char_widths: vec![1; plain_text.len()],
                plain_text,
            }
        })
        .collect();
    cache.virtual_layout = Some(Arc::new(VirtualLayoutSnapshot {
        history_epoch: 4,
        window_start: start,
        message_versions: vec![55; 8].into(),
        line_origins: (start..start + 20).map(|line| (7, line)).collect(),
        ..Default::default()
    }));
}

fn make_mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[tokio::test]
async fn test_history_replacement_cancels_copy_before_renderer_publication() {
    let (mut app, _) = App::new_headless(80, 10).await;
    app.session_mgr
        .current_mut()
        .ui
        .text_selection
        .start_drag(0, 0);
    let generation = app.session_mgr.current().ui.text_selection.generation;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.session_mgr.current_mut().ui.pending_text_copy = Some((generation, receiver));
    app.rebuild_vms_from_all(
        &[cc_agent::messages::BaseMessage::human("替换后的历史")],
        None,
    );
    assert!(
        sender.is_closed(),
        "历史排队重建前必须取消旧复制，不能等worker发布新epoch"
    );
    assert_ne!(
        app.session_mgr.current().ui.text_selection.generation,
        generation
    );
    assert!(app
        .session_mgr
        .current()
        .ui
        .presented_selection_snapshot
        .is_none());
}

#[tokio::test]
async fn test_subagent_viewport_switch_cancels_pending_copy_immediately() {
    let (mut app, _) = App::new_headless(80, 10).await;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.session_mgr.current_mut().ui.pending_text_copy = Some((0, receiver));
    app.switch_subagent_viewport(None);
    assert!(sender.is_closed(), "切视图必须同步取消旧来源的异步复制");
    assert!(app
        .session_mgr
        .current()
        .ui
        .presented_selection_snapshot
        .is_none());
}

async fn wait_for_real_viewport(
    app: &App,
    top: usize,
    height: usize,
    width: u16,
    previous_version: u64,
) {
    let session = app.session_mgr.current();
    let cache = Arc::clone(&session.messages.render_cache);
    let notify = Arc::clone(&session.messages.render_notify);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let notification = notify.notified();
            let ready = {
                let cache = cache.read();
                cache.version > previous_version
                    && cache.width == width
                    && cache.virtual_layout.as_ref().is_some_and(|layout| {
                        layout.requested_top == top && layout.requested_height == height
                    })
            };
            if ready {
                break;
            }
            notification.await;
        }
    })
    .await
    .expect("真实渲染任务应发布所请求的窗口");
}

#[tokio::test]
async fn test_real_renderer_scroll_event_publishes_and_draws_requested_window() {
    // new_headless 内部使用生产 spawn_render_thread，不替换缓存、不拦截渲染通道。
    let (mut app, mut handle) = App::new_headless(80, 10).await;
    let messages: Vec<_> = (0..200)
        .map(|index| MessageViewModel::user(format!("render-message-{index:04}")))
        .collect();
    let sender = app.session_mgr.current().messages.render_tx.clone();
    app.session_mgr.current_mut().messages.view_messages = messages.clone();
    sender
        .send(RenderEvent::SetDetailStrategy("virtual".to_owned()))
        .await
        .expect("启用真实虚拟渲染任务");
    sender
        .send(RenderEvent::Rebuild(messages))
        .await
        .expect("发送真实用户消息");
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("首次绘制上报真实宽度和高度");
    // 初次空缓存尚无滚动条，先发布80列，再由下一帧实际布局请求79列。
    wait_for_real_viewport(&app, 0, 10, 80, 0).await;
    let initial_version = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .version;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("绘制真实尾部窗口");
    let tail_top = app.session_mgr.current().ui.scroll_offset;
    wait_for_real_viewport(&app, tail_top, 10, 79, initial_version).await;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("宽度收敛后绘制真实尾窗");
    assert!(
        handle.contains("render-message-0199"),
        "真实渲染尾窗必须绘出最后一条用户消息"
    );
    let old_window_start = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .virtual_layout
        .as_ref()
        .expect("真实虚拟窗口")
        .window_start;
    for _ in 0..45 {
        handle_event(&mut app, make_mouse(MouseEventKind::ScrollUp, 10, 5))
            .await
            .expect("真实鼠标滚轮入口");
    }
    let requested_top = app.session_mgr.current().ui.scroll_offset;
    assert!(
        requested_top < old_window_start,
        "滚动目标必须离开已物化尾窗"
    );
    let previous_version = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .version;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("新窗口未发布前保留正文");
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        requested_top,
        "旧尾窗的校正不得覆盖最新滚轮目标"
    );
    assert!(
        handle.contains("render-message-0199"),
        "等待真实任务物化新窗时不能清空正文"
    );
    wait_for_real_viewport(&app, requested_top, 10, 79, previous_version).await;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("发布后绘制真实目标窗口");
    let expected_message = {
        let session = app.session_mgr.current();
        let cache = session.messages.render_cache.read();
        assert!(
            cache
                .virtual_layout
                .as_ref()
                .expect("新虚拟窗口")
                .window_start
                < old_window_start,
            "真实任务必须切换窗口，而非继续复用尾部假数据"
        );
        let top = session.ui.presented_scroll_offset;
        let visible_body = cache
            .wrap_map
            .iter()
            .find(|line| {
                line.visual_row_start >= top
                    && line.visual_row_end <= top + 10
                    && line.plain_text.contains("render-message-")
            })
            .expect("实际可见窗口应包含消息正文");
        cache
            .virtual_layout
            .as_ref()
            .expect("窗口来源映射")
            .line_origins[visible_body.line_idx]
            .0
    };
    assert!(expected_message < 199);
    assert!(
        handle.contains(&format!("render-message-{expected_message:04}")),
        "目标源消息必须真正绘到终端buffer"
    );
    assert!(
        !handle.contains("render-message-0199"),
        "切窗完成后不能继续显示旧尾窗"
    );
}

#[tokio::test]
async fn test_virtual_old_publish_does_not_override_new_scroll_request() {
    let (mut app, mut handle) = App::new_headless(80, 10).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    let (sender, _receiver) = tokio::sync::mpsc::channel(16);
    app.session_mgr.current_mut().messages.render_tx = sender;
    make_window(&mut app, 100);
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        cache.scroll_anchor = Some(115);
        let layout = Arc::make_mut(cache.virtual_layout.as_mut().expect("旧窗口"));
        layout.requested_top = 115;
        layout.requested_height = 10;
    }
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_offset = 115;
    ui.presented_scroll_offset = 115;
    ui.scroll_follow = false;
    ui.scrollbar_max_offset = 990;
    handle_event(&mut app, make_mouse(MouseEventKind::ScrollUp, 10, 5))
        .await
        .expect("新滚轮事件");
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 112);
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("处理尚未消费的旧发布");
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        112,
        "旧请求115的anchor不能吃掉新请求112"
    );
}

#[tokio::test]
async fn test_virtual_viewport_preserves_display_and_copies_across_eviction() {
    let (mut app, mut handle) = App::new_headless(80, 10).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
    app.session_mgr.current_mut().messages.render_tx = sender;
    make_window(&mut app, 100);
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_offset = 800;
    ui.presented_scroll_offset = 102;
    ui.scroll_follow = false;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("绘制等待中的虚拟窗口");
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        800,
        "保留用户请求，不取消待物化目标"
    );
    assert_eq!(
        app.session_mgr.current().ui.presented_scroll_offset,
        102,
        "缺页期间保留实际显示内容"
    );
    assert_eq!(
        handle.terminal.backend().buffer()[(0, 0)].symbol(),
        "W",
        "缺页期间不得清空正文"
    );
    assert!(
        matches!(
            receiver.try_recv(),
            Ok(RenderEvent::Viewport {
                top: 800,
                height: 10,
                anchor: None,
                ..
            })
        ),
        "真实绘制应上报请求视口"
    );
    make_window(&mut app, 500);
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Down(MouseButton::Left), 0, 2),
    )
    .await
    .expect("消息区按下");
    let start = app
        .session_mgr
        .current()
        .ui
        .text_selection
        .anchor_start
        .expect("实际显示内容应可命中");
    assert_eq!(
        start.logical_line, 104,
        "鼠标必须解释为实际显示位置，而非尚未加载的目标"
    );
    make_window(&mut app, 800);
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("绘制新窗口");
    assert!(
        matches!(receiver.try_recv(), Ok(RenderEvent::Viewport {top:800,anchor:Some(anchor),..}) if anchor.message_index==7 && anchor.logical_line==800),
        "目标位置未变也需补报精确锚点"
    );
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Drag(MouseButton::Left), 5, 3),
    )
    .await
    .expect("跨窗口拖拽");
    let end = app
        .session_mgr
        .current()
        .ui
        .text_selection
        .anchor_end
        .expect("新窗口终点");
    assert_eq!(end.logical_line, 803);
    assert_eq!(end.grapheme_offset, 5);
    assert_eq!(
        app.session_mgr.current().ui.text_selection.anchor_start,
        Some(start),
        "淘汰旧窗口不应移动起点"
    );
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Up(MouseButton::Left), 5, 3),
    )
    .await
    .expect("释放跨窗口选区");
    let event = receiver.try_recv().expect("应向真实渲染任务请求分段复制");
    assert!(
        matches!(event, RenderEvent::CopySelection {start:copy_start,end:copy_end,..} if copy_start==start && copy_end==end),
        "复制必须携带稳定源坐标，不能退回窗口局部索引"
    );
    assert!(
        app.session_mgr.current().ui.pending_text_copy.is_some(),
        "复制异步等待不能阻塞输入"
    );
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        Arc::make_mut(cache.virtual_layout.as_mut().expect("虚拟快照")).history_epoch += 1;
    }
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("布局代次变化后绘制");
    assert!(
        app.session_mgr
            .current()
            .ui
            .text_selection
            .anchor_start
            .is_none(),
        "宽度/主题/模式变化不得继续解释旧选区"
    );
    assert!(
        app.session_mgr.current().ui.pending_text_copy.is_none(),
        "失效选区必须取消待复制结果"
    );
}

#[tokio::test]
async fn test_virtual_copy_completion_does_not_replace_new_selection() {
    let (mut app, _) = App::new_headless(80, 10).await;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let selection = &mut app.session_mgr.current_mut().ui.text_selection;
    selection.start_drag(0, 0);
    let old_generation = selection.generation;
    selection.start_drag(10, 0);
    app.session_mgr.current_mut().ui.pending_text_copy = Some((old_generation, receiver));
    sender
        .send(Some("旧选区".to_string()))
        .expect("发送旧复制结果");
    assert!(!poll_message_selection_copy(&mut app));
    assert_eq!(
        app.session_mgr.current().ui.text_selection.start,
        Some((10, 0)),
        "过期完成不能替换新选区"
    );
    assert!(app
        .session_mgr
        .current()
        .ui
        .text_selection
        .selected_text
        .is_none());
}

#[tokio::test]
async fn test_virtual_copy_completion_rejects_changed_endpoint_version() {
    let (mut app, _) = App::new_headless(80, 10).await;
    make_window(&mut app, 100);
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.text_selection.start_drag(100, 0);
    let start = crate::ui::render_thread::SelectionAnchor {
        history_epoch: 4,
        message_index: 7,
        message_version: 55,
        logical_line: 100,
        grapheme_offset: 0,
    };
    ui.text_selection.anchor_start = Some(start);
    ui.text_selection.anchor_end = Some(crate::ui::render_thread::SelectionAnchor {
        grapheme_offset: 10,
        ..start
    });
    let (sender, receiver) = tokio::sync::oneshot::channel();
    ui.pending_text_copy = Some((ui.text_selection.generation, receiver));
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        let layout = Arc::make_mut(cache.virtual_layout.as_mut().expect("虚拟布局"));
        Arc::make_mut(&mut layout.message_versions)[7] = 56;
    }
    sender
        .send(Some("旧内容不应复制".to_owned()))
        .expect("返回旧快照结果");
    assert!(poll_message_selection_copy(&mut app));
    let ui = &app.session_mgr.current().ui;
    assert!(
        ui.text_selection.anchor_start.is_none(),
        "相同epoch内的流式内容版本变化也必须作废旧结果"
    );
    assert!(
        ui.text_selection.selected_text.is_none(),
        "不能复制旧内容或等待下一次绘制才作废"
    );
}

#[tokio::test]
async fn test_virtual_scrollbar_drag_freezes_scale_while_heights_change() {
    let (mut app, mut handle) = App::new_headless(80, 30).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    make_window(&mut app, 100);
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_offset = 500;
    ui.scroll_follow = false;
    ui.scrollbar_hover = true;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("绘制初始滚动条");
    let metrics = app
        .session_mgr
        .current()
        .ui
        .message_scrollbar_metrics
        .expect("滚动条");
    let row = metrics.thumb_area.y;
    assert!(handle_message_scrollbar_down(
        &mut app,
        row,
        metrics.bar_area.x
    ));
    let (_, start, travel) = app
        .session_mgr
        .current()
        .ui
        .message_scrollbar_drag_origin
        .expect("拖拽起点");
    app.session_mgr
        .current()
        .messages
        .render_cache
        .write()
        .total_lines = 2_000;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("部分高度从估算校正到实测");
    assert!(handle_message_scrollbar_drag(&mut app, row + 1));
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        start + metrics.max_offset / travel as usize,
        "同一次拖拽不可因高度实测而改变比例"
    );
    assert!(handle_message_scrollbar_drag(&mut app, row));
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        start,
        "回到按下位置仍恢复原精确偏移"
    );
}

#[tokio::test]
async fn test_virtual_plain_click_does_not_export_empty_selection() {
    let (mut app, mut handle) = App::new_headless(80, 10).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
    app.session_mgr.current_mut().messages.render_tx = sender;
    make_window(&mut app, 100);
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_offset = 100;
    ui.scroll_follow = false;
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), frame.area()))
        .expect("绘制可点击窗口");
    let _ = receiver.try_recv().expect("视口上报");
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Down(MouseButton::Left), 1, 2),
    )
    .await
    .expect("按下正文");
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Up(MouseButton::Left), 1, 2),
    )
    .await
    .expect("原地释放");
    assert!(
        receiver.try_recv().is_err(),
        "纯点击不能请求空导出或覆盖用户剪贴板"
    );
    assert!(app.session_mgr.current().ui.pending_text_copy.is_none());
    assert!(!app.session_mgr.current().ui.text_selection.is_active());
}

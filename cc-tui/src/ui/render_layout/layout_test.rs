use super::*;
use crate::ui::render_thread::{spawn_render_thread, RenderEvent};

#[tokio::test]
async fn test_virtual_bash_inspect_group_height_matches_real_renderer() {
    use crate::ui::message_view::{ToolCategory, ToolEntry};
    for content in [
        String::new(),
        "[Command completed with exit code 0]".into(),
        "{\"items\":[\"a\",\"b\",\"c\"],\"ok\":true}".into(),
        cc_middlewares::middleware::format_command_output(
            &"查找上下文\n".repeat(20),
            "grep: missing file",
            2,
        ),
    ] {
        let mut group = MessageViewModel::ToolCallGroup {
            thinking: None,
            category: ToolCategory::Inspect,
            tools: vec![ToolEntry {
                tool_name: "Bash".into(),
                display_name: "Bash".into(),
                args_display: Some(format!("grep needle {}/tail.txt", "很长的目录/".repeat(20))),
                content,
                is_error: false,
                file_inspect: true,
            }],
            collapsed: true,
            standalone_action: Some("ran 1 shell command".into()),
            content_hash: 0,
        };
        group.recompute_hash();
        let mut renderer = VirtualRenderer::default();
        for detail in [false, true, false] {
            let messages = vec![group.clone()];
            let (reference, wrap) = make_reference(&messages, detail);
            let cache = make_complete(&mut renderer, messages, detail).await;
            assert_eq!(
                cache.lines, reference,
                "单行命令头、组内错误摘要和 JSON 输出不得被估算高度裁剪"
            );
            assert_eq!(
                cache.total_lines,
                wrap.last().map_or(0, |line| line.visual_row_end),
                "组高度必须按实际折行数修正"
            );
            assert_eq!(
                renderer.metadata[0].measured,
                Some(cache.total_lines),
                "切换展开态后高度索引必须与实际渲染行数一致"
            );
        }
    }
}

#[tokio::test]
async fn test_virtual_streaming_output_json_and_cjk_height_matches_real_renderer() {
    use crate::ui::message_view::{ToolOutputPreview, ToolPhase};
    for tail in [
        "{\"items\":[\"a\",\"b\",\"c\",\"d\"],\"ok\":true}".to_owned(),
        "很长的构建日志需要折行以避免输出被裁剪。".repeat(40),
    ] {
        let mut vm = make_tool(0, 0);
        if let MessageViewModel::ToolBlock {
            phase,
            live_output,
            started_at,
            ..
        } = &mut vm
        {
            *phase = ToolPhase::Running;
            *live_output = Some(ToolOutputPreview::new(tail.clone(), 1, tail.len(), 3000));
            *started_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
        }
        vm.recompute_hash();
        let mut renderer = VirtualRenderer::default();
        for detail in [false, true, false] {
            let source = vec![vm.clone()];
            let (reference, wrap) = make_reference(&source, detail);
            let cache = make_complete(&mut renderer, source, detail).await;
            assert_eq!(
                cache.lines[1..],
                reference[1..],
                "虚拟视口应发布完整真实行，不能按原始行数裁剪JSON或长行"
            );
            assert_eq!(
                cache.total_lines,
                wrap.last().map_or(0, |line| line.visual_row_end)
            );
            assert_eq!(
                renderer.metadata[0].measured,
                Some(cache.total_lines),
                "高度索引应被实际装饰器结果修正"
            );
        }
    }
}

fn make_mode(detail: bool) -> LayoutMode {
    LayoutMode {
        width: 80,
        detail,
        diff: false,
        hook_rows: true,
        theme: crate::ui::theme::fingerprint(),
    }
}
fn make_tool(index: usize, count: usize) -> MessageViewModel {
    let mut vm = MessageViewModel::tool_block_with_id(
        format!("call-{index}"),
        "Bash".into(),
        "Bash".into(),
        Some("run".into()),
        false,
    );
    if let MessageViewModel::ToolBlock { content, .. } = &mut vm {
        *content = (0..count)
            .map(|n| format!("output {index} line {n}\n"))
            .collect();
    }
    vm.recompute_hash();
    vm
}
fn make_reference(
    messages: &[MessageViewModel],
    detail: bool,
) -> (Vec<Line<'static>>, Vec<WrappedLineInfo>) {
    let mut lines = Vec::new();
    for (index, mut vm) in messages.iter().cloned().enumerate() {
        for line in RenderTask::render_one(&mut vm, index + 1, 80, false, true, detail).0 {
            if super::super::render_thread::is_blank_line(&line)
                && lines
                    .last()
                    .is_some_and(super::super::render_thread::is_blank_line)
            {
                continue;
            }
            lines.push(line);
        }
    }
    while lines
        .last()
        .is_some_and(super::super::render_thread::is_blank_line)
    {
        lines.pop();
    }
    let wrap = RenderTask::build_wrap_map(&lines, 80).1;
    (lines, wrap)
}
async fn make_complete(
    renderer: &mut VirtualRenderer,
    messages: Vec<MessageViewModel>,
    detail: bool,
) -> RenderCache {
    renderer.sync_messages(&messages);
    let mut source: Vec<_> = messages.into_iter().map(Arc::new).collect();
    renderer
        .render(
            &mut source,
            make_mode(detail),
            Viewport {
                scroll_revision: 0,
                height: 100_000,
                ..Viewport::default()
            },
            0,
        )
        .await
}

#[test]
fn test_height_index_point_update_and_zero_height_location() {
    let mut index = HeightIndex::default();
    index.reset(vec![2, 0, 4, 1]);
    assert_eq!(index.total(), 7);
    assert_eq!(index.prefix(2), 2);
    assert_eq!(index.locate(2), 2);
    index.set(0, 5);
    assert_eq!(index.total(), 10);
    assert_eq!(index.locate(5), 2);
    assert_eq!(index.locate(9), 3);
}

#[tokio::test]
async fn test_virtual_full_window_matches_published_renderer() {
    for detail in [false, true] {
        let messages = vec![
            MessageViewModel::user("中文 e\u{301} 👩‍💻\n\n下一段".into()),
            make_tool(1, 12),
            MessageViewModel::system_with_detail("摘要".into(), "详细\n\n正文".into()),
            make_tool(3, 4),
        ];
        let reference = make_reference(&messages, detail);
        let mut renderer = VirtualRenderer::default();
        let result = make_complete(&mut renderer, messages, detail).await;
        assert_eq!(
            result.lines, reference.0,
            "全量窗口的文本、样式和间距必须与发布版一致"
        );
        assert_eq!(
            result
                .wrap_map
                .iter()
                .map(|w| (&w.plain_text, w.visual_row_start, w.visual_row_end))
                .collect::<Vec<_>>(),
            reference
                .1
                .iter()
                .map(|w| (&w.plain_text, w.visual_row_start, w.visual_row_end))
                .collect::<Vec<_>>(),
            "换行必须等价"
        );
    }
}

#[tokio::test]
async fn test_virtual_append_restores_previous_tail_separator() {
    let mut renderer = VirtualRenderer::default();
    let first = vec![make_tool(1, 4)];
    let _ = make_complete(&mut renderer, first.clone(), true).await;
    let mut appended = first;
    appended.push(MessageViewModel::user("后续".into()));
    let reference = make_reference(&appended, true).0;
    let result = make_complete(&mut renderer, appended, true).await;
    assert_eq!(
        result.lines, reference,
        "旧尾条不再是尾条时必须恢复分隔空行"
    );
}

#[tokio::test]
async fn test_virtual_viewport_does_not_materialize_entire_history() {
    let messages: Vec<_> = (0..1000).map(|i| make_tool(i, 20)).collect();
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut source: Vec<_> = messages.into_iter().map(Arc::new).collect();
    let result = renderer
        .render(&mut source, make_mode(true), Viewport::default(), 0)
        .await;
    let stats = &result
        .virtual_layout
        .as_ref()
        .expect("应有虚拟元数据")
        .diagnostics;
    assert!(stats.resident_segments < 10, "首帧只能渲染视口附近");
    assert!(stats.estimated_segments > 990);
    assert!(result.lines.len() < 100, "共享表不得含全量历史");
    assert_eq!(
        result.wrap_map.last().map(|w| w.visual_row_end),
        Some(result.total_lines),
        "跟随尾部必须覆盖实际最后一行"
    );
}

#[tokio::test]
async fn test_virtual_metadata_is_shared_on_unchanged_viewport() {
    let messages = vec![make_tool(0, 60), make_tool(1, 60)];
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut source: Vec<_> = messages.into_iter().map(Arc::new).collect();
    let first = renderer
        .render(&mut source, make_mode(true), Viewport::default(), 0)
        .await;
    let second = renderer
        .render(&mut source, make_mode(true), Viewport::default(), 0)
        .await;
    let a = first.virtual_layout.expect("应有元数据");
    let b = second.virtual_layout.expect("应有元数据");
    assert!(
        Arc::ptr_eq(&a.message_visual_offsets, &b.message_visual_offsets),
        "高度未变不能重新克隆全历史前缀"
    );
    assert!(Arc::ptr_eq(&a.message_versions, &b.message_versions));
    assert_eq!(a.history_epoch, b.history_epoch, "滚动不改变历史代际");
}

#[tokio::test]
async fn test_virtual_copy_exports_evicted_region_from_frozen_source() {
    let messages = vec![make_tool(0, 20), MessageViewModel::user("尾部中文".into())];
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut source: Vec<_> = messages.into_iter().map(Arc::new).collect();
    let _ = renderer
        .render(&mut source, make_mode(true), Viewport::default(), 0)
        .await;
    let start = SelectionAnchor {
        history_epoch: renderer.history_epoch,
        message_index: 0,
        message_version: source[0].content_hash(),
        logical_line: 0,
        grapheme_offset: 0,
    };
    let end = SelectionAnchor {
        history_epoch: renderer.history_epoch,
        message_index: 1,
        message_version: source[1].content_hash(),
        logical_line: 0,
        grapheme_offset: 99,
    };
    renderer.release_products();
    let (tx, rx) = tokio::sync::oneshot::channel();
    renderer.export_selection(&source, end, start, tx);
    source[1] = Arc::new(MessageViewModel::user("新内容不应污染旧快照".into()));
    let text = rx.await.expect("应返回导出结果").expect("版本合法");
    assert!(text.contains("output 0 line 19"));
    assert!(text.contains("尾部中文"));
    assert!(!text.contains("污染旧快照"));
    assert_eq!(renderer.bytes, 0, "导出不得把全选区装回缓存");
}

#[tokio::test]
async fn test_resize_preserves_following_detail_control_event() {
    let (tx, cache, notify) = spawn_render_thread(80);
    tx.send(RenderEvent::Rebuild(vec![make_tool(0, 30)]))
        .await
        .expect("应入队");
    notify.notified().await;
    tx.try_send(RenderEvent::Resize(70)).expect("应入队");
    tx.try_send(RenderEvent::ToggleDetail(true))
        .expect("应入队");
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    let output = cache.read();
    assert_eq!(output.width, 70);
    assert!(
        output
            .lines
            .iter()
            .any(|line| line.to_string().contains("line 29")),
        "resize合并不得吞详情事件"
    );
}

fn make_hidden_group() -> MessageViewModel {
    use crate::ui::message_view::{ToolCategory, ToolEntry};
    let mut message = MessageViewModel::ToolCallGroup {
        thinking: None,
        category: ToolCategory::Read,
        tools: vec![ToolEntry {
            tool_name: "Read".into(),
            display_name: "Read".into(),
            args_display: Some("file.txt".into()),
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

#[tokio::test]
async fn test_virtual_hidden_tail_matches_reference_without_following() {
    let messages = vec![
        make_hidden_group(),
        MessageViewModel::user("中文尾条".into()),
        make_hidden_group(),
        make_hidden_group(),
    ];
    let reference = make_reference(&messages, false);
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut sources: Vec<_> = messages.into_iter().map(Arc::new).collect();
    let output = renderer
        .render(
            &mut sources,
            make_mode(false),
            Viewport {
                scroll_revision: 0,
                height: 1000,
                follow_tail: false,
                ..Viewport::default()
            },
            0,
        )
        .await;
    assert_eq!(
        output.lines, reference.0,
        "隐藏尾组不能留下多余空行或吞掉正文"
    );
    assert_eq!(
        output.total_lines,
        reference.1.last().map_or(0, |line| line.visual_row_end)
    );
}

#[tokio::test]
async fn test_virtual_detail_toggle_reuses_both_variants_with_shared_budget() {
    let messages = vec![make_tool(0, 100)];
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut sources: Vec<_> = messages.into_iter().map(Arc::new).collect();
    let detail = renderer
        .render(&mut sources, make_mode(true), Viewport::default(), 0)
        .await;
    let key = renderer.key(0, &sources, 0);
    let original = Arc::clone(&renderer.residents[&key].payload);
    let _ = renderer
        .render(&mut sources, make_mode(false), Viewport::default(), 0)
        .await;
    let hot = renderer
        .render(&mut sources, make_mode(true), Viewport::default(), 0)
        .await;
    assert!(
        Arc::ptr_eq(&original, &renderer.residents[&key].payload),
        "热切换不得重新渲染仍驻留的详情"
    );
    assert_eq!(detail.lines, hot.lines);
    assert!(renderer.residents.keys().any(|key| !key.detail));
    assert!(
        renderer.bytes <= DETAIL_CACHE_BUDGET,
        "两种模式必须共用而非各占一份预算"
    );
}

#[tokio::test]
async fn test_virtual_lru_budget_and_copy_after_actual_eviction() {
    let messages: Vec<_> = (0..160).map(|index| make_tool(index, 400)).collect();
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut sources: Vec<_> = messages.into_iter().map(Arc::new).collect();
    for index in 0..sources.len() {
        let top = renderer.heights.prefix(index);
        let output = renderer
            .render(
                &mut sources,
                make_mode(true),
                Viewport {
                    scroll_revision: 0,
                    top,
                    height: 12,
                    follow_tail: false,
                    anchor: Some(ViewportAnchor {
                        message_index: index,
                        logical_line: 0,
                        row_in_line: 0,
                    }),
                },
                0,
            )
            .await;
        assert!(output.lines.len() < 450, "共享成品不得随累计访问历史增长");
        assert!(
            renderer.bytes <= DETAIL_CACHE_BUDGET,
            "普通消息驻留必须保持预算"
        );
    }
    assert!(renderer.evictions > 0, "测试必须真正超过预算并发生淘汰");
    assert!(
        !renderer.residents.keys().any(|key| key.index == 0),
        "最早的消息应已淘汰"
    );
    let before = renderer.bytes;
    let anchor = SelectionAnchor {
        history_epoch: renderer.history_epoch,
        message_index: 0,
        message_version: sources[0].content_hash(),
        logical_line: 0,
        grapheme_offset: 0,
    };
    let (send, receive) = tokio::sync::oneshot::channel();
    renderer.export_selection(
        &sources,
        anchor,
        SelectionAnchor {
            logical_line: 10_000,
            grapheme_offset: usize::MAX,
            ..anchor
        },
        send,
    );
    let text = receive
        .await
        .expect("导出通道")
        .expect("淘汰不应使源选区失效");
    assert!(
        text.contains("output 0 line 399"),
        "淘汰后仍应复制完整原始选区"
    );
    assert_eq!(renderer.bytes, before, "复制不能把被淘汰的历史装回渲染缓存");
}

#[tokio::test]
async fn test_virtual_reading_head_does_not_materialize_offscreen_tail() {
    let messages = vec![make_tool(0, 40), make_tool(1, 10_000)];
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut sources: Vec<_> = messages.into_iter().map(Arc::new).collect();
    let output = renderer
        .render(
            &mut sources,
            make_mode(true),
            Viewport {
                scroll_revision: 0,
                height: 8,
                follow_tail: false,
                ..Viewport::default()
            },
            0,
        )
        .await;
    assert!(output.lines.len() < 60, "头窗不应包含离屏长消息");
    assert!(
        renderer.residents.keys().all(|key| key.index == 0),
        "不能为了确定尾高度而顺带渲染巨型尾条"
    );
    assert!(
        renderer.metadata[1].measured.is_none(),
        "离屏高度应继续估算"
    );
    assert!(
        renderer.metadata[1].summary.height(160, true, false) >= 10_000,
        "宽终端不能让固定换行数量减少"
    );
}

#[tokio::test]
async fn test_virtual_theme_generation_invalidates_preparsed_text_without_hash_change() {
    use crate::ui::message_view::ContentBlockView;
    let message = cc_agent::messages::BaseMessage::ai("**当前正文**");
    let vm = MessageViewModel::from_base_message_with_cwd(&message, &[], None);
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(std::slice::from_ref(&vm));
    let mut sources = vec![Arc::new(vm)];
    let first = renderer
        .render(&mut sources, make_mode(true), Viewport::default(), 0)
        .await;
    let version = sources[0].content_hash();
    if let MessageViewModel::AssistantBubble { blocks, .. } = Arc::make_mut(&mut sources[0]) {
        for block in blocks {
            if let ContentBlockView::Text { rendered, .. } = block {
                *rendered = ratatui::text::Text::from("过期主题快照");
            }
        }
    }
    assert_eq!(
        sources[0].content_hash(),
        version,
        "预解析结果不参与内容版本"
    );
    let mut next_mode = make_mode(true);
    next_mode.theme = next_mode.theme.wrapping_add(1);
    let refreshed = renderer
        .render(&mut sources, next_mode, Viewport::default(), 0)
        .await;
    assert_eq!(
        refreshed.lines, first.lines,
        "主题代次变化必须按raw惰性重建，不能复用旧Arc中的预解析样式"
    );
}
#[tokio::test]
async fn test_virtual_image_attachment_copy_omits_prefix_and_keeps_row_count() {
    let messages = vec![MessageViewModel::user("[Image #1] [Image #2]".into())];
    let mut renderer = VirtualRenderer::default();
    renderer.sync_messages(&messages);
    let mut source: Vec<_> = messages.into_iter().map(Arc::new).collect();
    let output = renderer
        .render(&mut source, make_mode(false), Viewport::default(), 0)
        .await;
    assert_eq!(
        output.total_lines, 3,
        "虚拟布局必须计算两个不可点击的附件行"
    );
    let start = SelectionAnchor {
        history_epoch: renderer.history_epoch,
        message_index: 0,
        message_version: source[0].content_hash(),
        logical_line: 1,
        grapheme_offset: 0,
    };
    let end = SelectionAnchor {
        logical_line: 2,
        grapheme_offset: 15,
        ..start
    };
    renderer.release_products();
    let (tx, rx) = tokio::sync::oneshot::channel();
    renderer.export_selection(&source, start, end, tx);
    assert_eq!(
        rx.await.expect("导出应完成").as_deref(),
        Some("[Image #1]\n[Image #2]"),
        "逐段恢复后复制仍必须排除附件前缀"
    );
}

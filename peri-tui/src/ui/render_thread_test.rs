use super::*;

/// 等待 RenderThread 处理完事件：yield 让出执行权给后台 task
async fn wait_render() {
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
}

#[test]
fn test_wrapped_line_info_supports_more_than_u16_rows() {
    let info = WrappedLineInfo {
        line_idx: 0,
        visual_row_start: u16::MAX as usize + 1,
        visual_row_end: u16::MAX as usize + 2,
        plain_text: String::new(),
        char_widths: Vec::new(),
    };

    assert_eq!(info.visual_row_start, u16::MAX as usize + 1);
    assert_eq!(info.visual_row_end, u16::MAX as usize + 2);
}

#[tokio::test]
async fn test_rebuild_increments_version() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    assert_eq!(cache.read().version, 0);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .await
    .unwrap();

    wait_render().await;

    let c = cache.read();
    assert!(c.version > 0, "version should increment after Rebuild");
    assert!(
        !c.lines.is_empty(),
        "lines should not be empty after Rebuild"
    );
}

#[tokio::test]
async fn test_rebuild_hash_diff_skips_unchanged() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 第一次 Rebuild：渲染两条消息
    let user1 = MessageViewModel::user("First".to_string());
    let user2 = MessageViewModel::user("Second".to_string());
    tx.send(RenderEvent::Rebuild(vec![user1.clone(), user2.clone()]))
        .await
        .unwrap();
    wait_render().await;

    let v1 = cache.read().version;
    let lines_v1 = cache.read().lines.len();

    // 第二次 Rebuild：相同内容，hash diff 应跳过渲染
    tx.send(RenderEvent::Rebuild(vec![user1, user2]))
        .await
        .unwrap();
    wait_render().await;

    let c = cache.read();
    // version 仍应递增（即使内容不变）
    assert!(c.version > v1, "version should still increment");
    // 行数不变
    assert_eq!(c.lines.len(), lines_v1, "lines count should be the same");
}

#[tokio::test]
async fn test_rebuild_no_trailing_blank() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    let c = cache.read();
    let last_is_empty = c.lines.last().is_some_and(|l| {
        l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
    });
    assert!(!last_is_empty, "should not have trailing blank line");
}

#[tokio::test]
async fn test_rebuild_multiple_messages_have_gaps() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::user("First message".to_string()),
        MessageViewModel::user("Second message".to_string()),
    ]))
    .await
    .unwrap();
    wait_render().await;

    let c = cache.read();
    // 找 "Second message" 的行，检查前一行是否为空行
    let mut second_msg_idx = None;
    for (i, line) in c.lines.iter().enumerate() {
        for span in &line.spans {
            if span.content.contains("Second message") {
                second_msg_idx = Some(i);
                break;
            }
        }
        if second_msg_idx.is_some() {
            break;
        }
    }
    let idx = second_msg_idx.expect("should find second user message");
    assert!(idx > 0, "second message should not be the first line");
    let prev_is_empty = c.lines[idx - 1].spans.is_empty()
        || (c.lines[idx - 1].spans.len() == 1 && c.lines[idx - 1].spans[0].content.is_empty());
    assert!(
        prev_is_empty,
        "should have blank line before second user message, but line {} is: {:?}",
        idx - 1,
        c.lines[idx - 1]
    );
}

#[tokio::test]
async fn test_rebuild_with_anchor_sets_scroll_anchor() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::RebuildWithAnchor {
        messages: vec![
            MessageViewModel::user("First".to_string()),
            MessageViewModel::user("Second".to_string()),
        ],
        anchor_message_idx: 1,
    })
    .await
    .unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.scroll_anchor.is_some(), "scroll_anchor should be set");
}

#[tokio::test]
async fn test_clear_resets_cache() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    tx.send(RenderEvent::Clear).await.unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.lines.is_empty(), "lines should be empty after Clear");
    assert_eq!(c.total_lines, 0);
}

#[tokio::test]
async fn test_resize_rebuilds_with_new_width() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    let user = MessageViewModel::user("Hello world".to_string());
    tx.send(RenderEvent::Rebuild(vec![user.clone()]))
        .await
        .unwrap();
    wait_render().await;

    let v1 = cache.read().version;
    let total_v1 = cache.read().total_lines;

    // Resize
    tx.send(RenderEvent::Resize(40)).await.unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.version > v1, "version should increment after Resize");
    // 窄宽度可能导致更多 wrap 行
    assert!(c.total_lines >= total_v1);
}

#[test]
fn test_build_wrap_map_empty() {
    let (total, result) = RenderTask::build_wrap_map(&[], 80);
    assert!(result.is_empty());
    assert_eq!(total, 0);
}

#[test]
fn test_build_wrap_map_single_short_line() {
    let lines = vec![Line::from("Hello")];
    let (total, result) = RenderTask::build_wrap_map(&lines, 80);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].visual_row_start, 0);
    assert_eq!(result[0].visual_row_end, 1);
    assert_eq!(result[0].plain_text, "Hello");
    assert_eq!(total, 1);
}

#[test]
fn test_build_wrap_map_single_long_line_wraps() {
    let long_text: String = "A".repeat(200);
    let lines: Vec<Line<'static>> = vec![Line::from(long_text)];
    let (total, result) = RenderTask::build_wrap_map(&lines, 40);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].visual_row_start, 0);
    assert_eq!(result[0].visual_row_end, 5);
    assert_eq!(total, 5);
}

#[test]
fn test_build_wrap_map_cjk_char_width() {
    let lines = vec![Line::from("你好世界")];
    let (total, result) = RenderTask::build_wrap_map(&lines, 80);
    assert_eq!(result[0].char_widths, vec![2, 2, 2, 2]);
    assert_eq!(result[0].visual_row_end - result[0].visual_row_start, 1);
    assert_eq!(total, 1);
}

#[test]
fn test_build_wrap_map_multi_line_visual_rows() {
    let first_line: String = "A".repeat(80);
    let second_line = Line::from("short");
    let lines: Vec<Line<'static>> = vec![Line::from(first_line), second_line];
    let (total, result) = RenderTask::build_wrap_map(&lines, 40);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].visual_row_start, 0);
    assert_eq!(result[0].visual_row_end, 2);
    assert_eq!(result[1].visual_row_start, 2);
    assert_eq!(result[1].visual_row_end, 3);
    assert_eq!(total, 3);
}

#[test]
fn test_build_wrap_map_empty_line() {
    let lines = vec![Line::from("")];
    let (total, result) = RenderTask::build_wrap_map(&lines, 80);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].visual_row_end - result[0].visual_row_start, 1);
    assert_eq!(total, 1);
}

// ─── 有界通道背压安全测试 ──────────────────────────────────────────────────

/// 填满通道后发送 Resize，验证 try_send 立即返回（不阻塞）
#[tokio::test]
async fn test_resize_try_send_when_channel_full() {
    let (tx, _cache, _notify) = spawn_render_thread(80);

    // 先发送一个 Rebuild 建立初始状态
    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    // 填满通道（不消费）
    for i in 0..128 {
        tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(format!(
            "Filler {i}"
        ))]))
        .await
        .unwrap();
    }

    // try_send Resize 应该返回 Err(Full)，不阻塞
    let result = tx.try_send(RenderEvent::Resize(40));
    assert!(
        result.is_err(),
        "try_send 在通道满时应返回错误，实际: {result:?}"
    );
    // 不验证 Resize 是否到达——通道满时丢弃 Resize 是预期行为
    // 渲染线程消费后会处理下一个 Resize（如果有）
}

/// 验证有界通道在大量事件下不会 panic 或死锁
#[tokio::test]
async fn test_bounded_channel_handles_high_volume() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 渲染线程会持续消费，所以很难真正填满。
    // 验证在大量事件下不会 panic 或死锁即可。
    for i in 0..200 {
        // blocking_send 在 async test 中会阻塞当前线程，
        // 但渲染线程在后台持续消费，所以不会真正卡住
        tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(format!(
            "Message {i}"
        ))]))
        .await
        .unwrap();
    }
    wait_render().await;

    let c = cache.read();
    assert!(c.version > 0, "渲染线程应处理了至少一个事件");
    assert!(!c.lines.is_empty(), "最终应有渲染结果");
}

/// 验证 drop sender 后渲染线程正常退出，不死锁
#[tokio::test]
async fn test_drop_sender_exits_cleanly() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Before drop".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    let version_before = cache.read().version;

    // Drop sender —— 模拟 ChatSession drop
    drop(tx);

    // 给渲染线程时间退出
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // cache 仍然可读（Arc<RwLock> 仍持有）
    let c = cache.read();
    assert_eq!(c.version, version_before, "drop 后不应有新事件处理");
}

/// 验证多个快速连续的 Resize 事件被合并为一个最终宽度
#[tokio::test]
async fn test_resize_coalesce_under_pressure() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 先建立初始内容
    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello world this is a longer message for wrapping".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    let width_80 = cache.read().total_lines;

    // 快速连续发送多个 Resize（模拟拖动窗口边缘）
    for w in [60, 50, 40, 30, 20] {
        tx.send(RenderEvent::Resize(w)).await.unwrap();
    }
    wait_render().await;

    let c = cache.read();
    // 最终宽度应为最后一个 Resize 的值（20）
    assert_eq!(c.width, 20, "最终宽度应为最后一个 Resize 值");
    // 窄宽度应有更多行（wrap 更多）
    assert!(
        c.total_lines >= width_80,
        "窄宽度应产生更多视觉行: {} >= {}",
        c.total_lines,
        width_80
    );
}

// ─── 增量 wrap_map 测试 ──────────────────────────────────────────────────

/// 辅助：构建 V2 的全量 wrap_map，返回 (total_lines, wrap_map)
fn full_wrap(vms: &[MessageViewModel], width: u16) -> (usize, Vec<super::WrappedLineInfo>) {
    let mut all_lines: Vec<Line<'static>> = Vec::new();
    for vm in vms {
        let mut lines = super::RenderTask::render_one(&mut vm.clone(), 0, width as usize, false, false);
        all_lines.append(&mut lines);
    }
    // dedup 连续空行
    let mut deduped: Vec<Line<'static>> = Vec::new();
    let mut prev_empty = false;
    for line in all_lines {
        let is_empty =
            line.spans.is_empty() || (line.spans.len() == 1 && line.spans[0].content.is_empty());
        if is_empty && prev_empty {
            continue;
        }
        prev_empty = is_empty;
        deduped.push(line);
    }
    while deduped.last().is_some_and(|l| {
        l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
    }) {
        deduped.pop();
    }
    super::RenderTask::build_wrap_map(&deduped, width)
}

/// 验证 message_offsets 在 deduped 索引空间中正确定位
#[tokio::test]
async fn test_message_offsets_match_deduped_space() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 三条消息
    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::user("First".to_string()),
        MessageViewModel::user("Second".to_string()),
        MessageViewModel::user("Third".to_string()),
    ]))
    .await
    .unwrap();
    wait_render().await;

    let c = cache.read();
    // offsets 应在 deduped 索引空间中
    // 每条消息至少一行内容 + 一行空行分隔（最后一条的尾部空行被 dedup 移除）
    assert!(c.message_offsets.len() == 3, "应有 3 个 offsets");
    // 第一个 offset 应为 0
    assert_eq!(c.message_offsets[0], 0, "第一条消息应从 0 开始");
    // offsets 中的值应 <= lines.len()
    for &off in &c.message_offsets {
        assert!(
            off <= c.lines.len(),
            "offset {off} 应 <= lines.len() {}",
            c.lines.len()
        );
    }
    // offsets 应单调递增
    for w in c.message_offsets.windows(2) {
        assert!(w[0] <= w[1], "offsets 应单调递增");
    }
}

/// 核心测试：增量 wrap_map 结果与全量计算完全一致
#[tokio::test]
async fn test_incremental_wrap_map_matches_full() {
    let (tx, cache, _notify) = spawn_render_thread(40);

    // 第一次 Rebuild：3 条长消息（需要 wrap）
    let long_text: String = "Hello world ".repeat(10);
    let vms = vec![
        MessageViewModel::user(long_text.clone()),
        MessageViewModel::user("Short".to_string()),
        MessageViewModel::user(long_text.clone()),
    ];
    tx.send(RenderEvent::Rebuild(vms.clone())).await.unwrap();
    wait_render().await;

    let (expected_total, expected_wrap) = full_wrap(&vms, 40);
    {
        let c = cache.read();
        assert_eq!(c.total_lines, expected_total, "total_lines 应一致");
        assert_eq!(c.wrap_map.len(), expected_wrap.len(), "wrap_map 长度应一致");
        for (i, (got, exp)) in c.wrap_map.iter().zip(expected_wrap.iter()).enumerate() {
            assert_eq!(
                got.visual_row_start, exp.visual_row_start,
                "wrap_map[{i}].visual_row_start 不一致"
            );
            assert_eq!(
                got.visual_row_end, exp.visual_row_end,
                "wrap_map[{i}].visual_row_end 不一致"
            );
        }
    }

    // 第二次 Rebuild：改变最后一条消息（prefix_stable_len = 2）
    let vms2 = vec![
        MessageViewModel::user(long_text.clone()),
        MessageViewModel::user("Short".to_string()),
        MessageViewModel::user("Changed content".to_string()),
    ];
    tx.send(RenderEvent::Rebuild(vms2.clone())).await.unwrap();
    wait_render().await;

    let (expected_total2, expected_wrap2) = full_wrap(&vms2, 40);
    let c2 = cache.read();
    assert_eq!(c2.total_lines, expected_total2, "增量 total_lines 应一致");
    assert_eq!(
        c2.wrap_map.len(),
        expected_wrap2.len(),
        "增量 wrap_map 长度应一致"
    );
    for (i, (got, exp)) in c2.wrap_map.iter().zip(expected_wrap2.iter()).enumerate() {
        assert_eq!(
            got.visual_row_start, exp.visual_row_start,
            "增量 wrap_map[{i}].visual_row_start 不一致"
        );
        assert_eq!(
            got.visual_row_end, exp.visual_row_end,
            "增量 wrap_map[{i}].visual_row_end 不一致"
        );
    }
}

/// 所有 VM 不变时 wrap_map 完全复用
#[tokio::test]
async fn test_incremental_wrap_map_all_stable() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    let vms = vec![
        MessageViewModel::user("Hello".to_string()),
        MessageViewModel::user("World".to_string()),
    ];
    tx.send(RenderEvent::Rebuild(vms.clone())).await.unwrap();
    wait_render().await;

    let v1 = cache.read().version;
    let total_v1 = cache.read().total_lines;
    let wrap_len_v1 = cache.read().wrap_map.len();

    // 完全相同的 Rebuild
    tx.send(RenderEvent::Rebuild(vms)).await.unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.version > v1, "version 应递增");
    assert_eq!(c.total_lines, total_v1, "total_lines 应不变");
    assert_eq!(c.wrap_map.len(), wrap_len_v1, "wrap_map 长度应不变");
}

/// 无稳定前缀时走全量路径
#[tokio::test]
async fn test_incremental_wrap_map_prefix_stable_len_zero() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 第一次 Rebuild
    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "First".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    // 第二次 Rebuild：完全不同的消息（prefix_stable_len = 0）
    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Completely different".to_string(),
    )]))
    .await
    .unwrap();
    wait_render().await;

    let c = cache.read();
    // 应正常渲染，不 panic
    assert!(!c.lines.is_empty());
    assert!(c.total_lines > 0);
}

/// ToggleDetail 切换后 AssistantBubble 的 Text 内容不会丢失
#[tokio::test]
async fn test_toggle_detail_preserves_assistant_text() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 构建含 Text block 的 AssistantBubble
    let mut assistant = MessageViewModel::assistant();
    assistant.append_chunk("Agent reply here");
    // 设为非流式，确保 rebuild 时走 ensure_rendered_flush 路径
    if let MessageViewModel::AssistantBubble {
        ref mut is_streaming,
        ..
    } = assistant
    {
        *is_streaming = false;
    }

    // 步骤 1：Rebuild
    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::user("Hello".to_string()),
        assistant.clone(),
    ]))
    .await
    .unwrap();
    wait_render().await;

    // 步骤 2：记录初始行数
    let initial_count = {
        let c = cache.read();
        c.lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("Agent reply here"))
            })
            .count()
    };
    assert!(
        initial_count > 0,
        "初始 Rebuild 后应能找到 'Agent reply here'"
    );

    // 步骤 3：ToggleDetail(true)
    tx.send(RenderEvent::ToggleDetail(true))
        .await
        .unwrap();
    wait_render().await;

    {
        let c = cache.read();
        let count = c
            .lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("Agent reply here"))
            })
            .count();
        assert!(
            count > 0,
            "ToggleDetail(true) 后 'Agent reply here' 不应丢失"
        );
    }

    // 步骤 4：ToggleDetail(false)
    tx.send(RenderEvent::ToggleDetail(false))
        .await
        .unwrap();
    wait_render().await;

    {
        let c = cache.read();
        let count = c
            .lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("Agent reply here"))
            })
            .count();
        assert!(
            count > 0,
            "ToggleDetail(false) 后 'Agent reply here' 不应丢失"
        );
    }
}

/// 模拟真实场景：reconcile 路径创建的 VM（from_base_message_with_cwd）
/// 多轮对话 + ToggleDetail 切换后，上一轮 agent 回复内容不应丢失
#[tokio::test]
async fn test_toggle_detail_reconcile_path_preserves_content() {
    use crate::ui::message_view::MessageViewModel;
    use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};

    let (tx, cache, _notify) = spawn_render_thread(80);

    // ── 构建第一轮对话的 BaseMessage ──
    let user1 = BaseMessage::human("你好");
    let ai1 = BaseMessage::ai(MessageContent::blocks(vec![
        ContentBlock::text("这是第一轮 agent 的回复内容，包含一些文字。"),
        ContentBlock::Reasoning {
            text: "这是思考过程的内容，比较长的一段 reasoning 文本。".to_string(),
            signature: None,
        },
    ]));

    // 通过 reconcile 路径（from_base_message_with_cwd）创建 VM
    let vm_user1 = MessageViewModel::from_base_message_with_cwd(&user1, &[], None);
    let vm_ai1 = MessageViewModel::from_base_message_with_cwd(
        &ai1,
        &[],
        None,
    );

    // 步骤 1：初始 Rebuild（模拟 agent 完成后的 render_rebuild）
    tx.send(RenderEvent::Rebuild(vec![vm_user1.clone(), vm_ai1]))
        .await
        .unwrap();
    wait_render().await;

    let initial_text_count = {
        let c = cache.read();
        c.lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("第一轮 agent 的回复内容"))
            })
            .count()
    };
    assert!(
        initial_text_count > 0,
        "初始 Rebuild 后应能找到第一轮回复内容，实际 lines 总数: {}",
        cache.read().lines.len()
    );

    // 步骤 2：构建第二轮（新增 UserBubble）
    let vm_user2 = MessageViewModel::user("第二个问题".to_string());

    // 模拟第二轮 RebuildAll：第一轮保留为 prefix，第二轮追加
    let vm_ai1_clone = MessageViewModel::from_base_message_with_cwd(&ai1, &[], None);
    tx.send(RenderEvent::Rebuild(vec![vm_user1.clone(), vm_ai1_clone, vm_user2]))
        .await
        .unwrap();
    wait_render().await;

    let round2_text_count = {
        let c = cache.read();
        c.lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("第一轮 agent 的回复内容"))
            })
            .count()
    };
    assert!(
        round2_text_count > 0,
        "第二轮 Rebuild 后第一轮回复内容应保留"
    );

    // 步骤 3：ToggleDetail(true) — 切到详细模式
    tx.send(RenderEvent::ToggleDetail(true))
        .await
        .unwrap();
    wait_render().await;

    let detail_text_count = {
        let c = cache.read();
        let text_lines: Vec<_> = c
            .lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("第一轮 agent 的回复内容"))
            })
            .collect();
        text_lines.len()
    };
    assert!(
        detail_text_count > 0,
        "ToggleDetail(true) 后第一轮 Text 内容不应丢失，实际 lines 总数: {}",
        cache.read().lines.len()
    );

    // detail 模式下 reasoning 也应该可见
    let detail_reasoning_count = {
        let c = cache.read();
        c.lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("思考过程"))
            })
            .count()
    };
    assert!(
        detail_reasoning_count > 0,
        "ToggleDetail(true) 后 reasoning 内容应可见"
    );

    // 步骤 4：ToggleDetail(false) — 切回普通模式
    tx.send(RenderEvent::ToggleDetail(false))
        .await
        .unwrap();
    wait_render().await;

    // ★ 核心断言：切回普通模式后，第一轮的 Text 内容必须保留
    let normal_text_count = {
        let c = cache.read();
        c.lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("第一轮 agent 的回复内容"))
            })
            .count()
    };
    assert!(
        normal_text_count > 0,
        "ToggleDetail(false) 后第一轮 Text 内容不应丢失！实际 lines 总数: {}, \
         前 30 行: {:?}",
        cache.read().lines.len(),
        cache
            .read()
            .lines
            .iter()
            .take(30)
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect::<Vec<_>>()
    );

    // reasoning 在普通模式下应该只有摘要行，不含完整内容
    let normal_reasoning_count = {
        let c = cache.read();
        c.lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("思考过程"))
            })
            .count()
    };
    // 普通模式下 reasoning 折叠，不应显示完整内容（但摘要行 "Thought for X chars" 应存在）
    let normal_reasoning_summary = {
        let c = cache.read();
        c.lines
            .iter()
            .filter(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("Thought for"))
            })
            .count()
    };
    assert!(
        normal_reasoning_summary > 0,
        "普通模式下 reasoning 摘要行应存在"
    );
    assert!(
        normal_reasoning_count == 0 || normal_reasoning_count < detail_reasoning_count,
        "普通模式下 reasoning 完整内容应折叠，detail={}, normal={}",
        detail_reasoning_count,
        normal_reasoning_count
    );
}

/// 新增 VM 时只重算尾部
#[tokio::test]
async fn test_incremental_wrap_map_add_new_vm() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    let vms1 = vec![MessageViewModel::user("Hello".to_string())];
    tx.send(RenderEvent::Rebuild(vms1)).await.unwrap();
    wait_render().await;

    // 新增一条 VM（prefix_stable_len = 1，覆盖旧消息）
    let vms2 = vec![
        MessageViewModel::user("Hello".to_string()),
        MessageViewModel::user("Added".to_string()),
    ];
    tx.send(RenderEvent::Rebuild(vms2.clone())).await.unwrap();
    wait_render().await;

    let (expected_total, expected_wrap) = full_wrap(&vms2, 80);
    let c = cache.read();
    assert_eq!(
        c.total_lines, expected_total,
        "新增 VM 后 total_lines 应一致"
    );
    assert_eq!(
        c.wrap_map.len(),
        expected_wrap.len(),
        "新增 VM 后 wrap_map 长度应一致"
    );
    // 前缀部分 wrap_map 应与全量计算一致
    for (i, (got, exp)) in c.wrap_map.iter().zip(expected_wrap.iter()).enumerate() {
        assert_eq!(
            got.visual_row_start, exp.visual_row_start,
            "新增 VM 后 wrap_map[{i}].visual_row_start 不一致"
        );
    }
}

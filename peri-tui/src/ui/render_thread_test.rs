use super::*;

/// 等待 RenderThread 处理完事件：yield 让出执行权给后台 task
async fn wait_render() {
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
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

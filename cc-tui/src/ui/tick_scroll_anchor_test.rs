use super::*;

fn make_task() -> RenderTask {
    RenderTask {
        last_messages: Vec::new(),
        message_lines: Vec::new(),
        message_links: Vec::new(),
        message_hashes: Vec::new(),
        cache: Arc::new(RwLock::new(RenderCache::new())),
        notify: Arc::new(Notify::new()),
        width: 9,
        show_tool_messages: false,
        diff_visible: false,
        hook_rows_visible: true,
        detail_mode: false,
        theme_fingerprint: theme::fingerprint(),
        alt_lines: Vec::new(),
        alt_links: Vec::new(),
        alt_keys: Vec::new(),
        alt_epochs: Vec::new(),
        line_epochs: Vec::new(),
        epoch_seq: 0,
        wrap_segs: [Vec::new(), Vec::new()],
    }
}

fn make_running_message(kind: &str) -> MessageViewModel {
    let mut message = if kind == "shell" {
        MessageViewModel::ShellCommand {
            id: "shell".into(),
            command: "build".into(),
            cwd: ".".into(),
            stdin: Vec::new(),
            stdout: "已有输出".into(),
            stderr: String::new(),
            exit_code: None,
            collapsed: false,
            content_hash: 0,
            started_at: Some(Instant::now()),
            moved_to_background: false,
        }
    } else {
        let mut bash = MessageViewModel::tool_block_with_id(
            "bash".into(),
            "Bash".into(),
            "Bash".into(),
            Some("build".into()),
            false,
        );
        if let MessageViewModel::ToolBlock {
            phase, started_at, ..
        } = &mut bash
        {
            *phase = ToolPhase::Running;
            *started_at = Some(Instant::now());
        }
        bash.recompute_hash();
        if kind == "subagent" {
            let mut group = MessageViewModel::subagent_group("child".into(), "build".into());
            if let MessageViewModel::SubAgentGroup {
                recent_messages,
                collapsed,
                ..
            } = &mut group
            {
                recent_messages.push(bash);
                *collapsed = false;
            }
            group
        } else {
            bash
        }
    };
    message.recompute_hash();
    message
}

fn make_history() -> MessageViewModel {
    MessageViewModel::system(
        (0..80)
            .map(|index| format!("历史第{index}行：abcdefghijklmnopqrstuvwxyz0123456789\n"))
            .collect(),
    )
}

fn make_reading_anchor(cache: &RenderCache, logical_line: usize) -> ViewportAnchor {
    let index = cache
        .line_origins
        .iter()
        .enumerate()
        .find(|(index, origin)| {
            origin.0 == 1
                && origin.1 >= logical_line
                && cache
                    .wrap_map
                    .get(*index)
                    .is_some_and(|line| line.visual_row_end > line.visual_row_start + 1)
        })
        .map(|(index, _)| index)
        .expect("长历史指定位置之后应存在真正的换行续行");
    let line = &cache.wrap_map[index];
    assert!(
        line.visual_row_end > line.visual_row_start + 1,
        "阅读位置应位于长行的换行续行"
    );
    cache
        .viewport_anchor_at(line.visual_row_start + 1)
        .expect("应能捕获历史行内位置")
}

fn make_elapsed(message: &mut MessageViewModel) {
    match message {
        MessageViewModel::ToolBlock { started_at, .. }
        | MessageViewModel::ShellCommand { started_at, .. } => {
            *started_at = Some(Instant::now() - Duration::from_millis(3_100));
        }
        MessageViewModel::SubAgentGroup {
            recent_messages, ..
        } => make_elapsed(&mut recent_messages[0]),
        _ => panic!("应为运行中的工具或命令"),
    }
}

fn make_inline_task() -> RenderTask {
    let mut task = make_task();
    let mut tool = MessageViewModel::tool_block_with_id(
        "read".into(),
        "Read".into(),
        "Read".into(),
        Some("file.rs".into()),
        false,
    );
    if let MessageViewModel::ToolBlock { phase, .. } = &mut tool {
        *phase = ToolPhase::Running;
    }
    tool.recompute_hash();
    task.rebuild_safe(vec![tool, make_history()]);
    // 固定一个就地补丁变宽的边界，避免依赖计时预检与补丁之间恰好跨秒。
    // tick 的 indicator 更新会把这份行缓存同步到成品并重算 wrap，消息逻辑行数不变。
    task.message_lines[0][0].spans[0].content = "●".into();
    task.message_lines[0][0].spans[1].content =
        "Read 状态文本增长到超过当前终端列宽，需要重新换行".into();
    task
}

#[test]
fn test_tick_rebuild_restores_consumed_reading_anchor_for_running_messages() {
    for kind in ["bash", "subagent", "shell"] {
        let mut task = make_task();
        task.rebuild_safe(vec![make_running_message(kind), make_history()]);
        let position = make_reading_anchor(&task.cache.read(), 20);
        let before = task
            .cache
            .read()
            .visual_row_for_anchor(position)
            .expect("旧布局应能恢复阅读位置");
        task.cache.write().scroll_anchor = Some(ScrollAnchor {
            position,
            visual_row: before,
            scroll_revision: 7,
        });
        assert!(task.cache.write().scroll_anchor.take().is_some());
        make_elapsed(&mut task.last_messages[0]);
        assert!(
            task.refresh_running_tool_indicators_with_scroll_anchor(0, Some((position, 7))),
            "跨过两秒阈值必须刷新运行状态"
        );
        let cache = task.cache.read();
        let anchor = cache
            .scroll_anchor
            .expect("旧锚点消费后仍需恢复当前阅读位置");
        assert_eq!(anchor.position, position);
        assert_eq!(anchor.scroll_revision, 7);
        assert!(anchor.visual_row > before, "上方运行状态应确实增高：{kind}");
        assert_eq!(
            cache.viewport_anchor_at(anchor.visual_row),
            Some(position),
            "上方耗时行增长不能改变下方正在阅读的逻辑行和续行：{kind}"
        );
    }
}

#[test]
fn test_tick_inline_wrap_refresh_restores_reading_and_recomputes_pending_anchor() {
    for consumed in [false, true] {
        let mut task = make_inline_task();
        let position = make_reading_anchor(&task.cache.read(), 20);
        let before = task
            .cache
            .read()
            .visual_row_for_anchor(position)
            .expect("旧布局应能恢复阅读位置");
        task.cache.write().scroll_anchor = Some(ScrollAnchor {
            position,
            visual_row: before,
            scroll_revision: 11,
        });
        if consumed {
            assert!(task.cache.write().scroll_anchor.take().is_some());
        }
        let reading_anchor = consumed.then_some((position, 11));
        let hashes_before = task.message_hashes.clone();
        assert!(task.refresh_running_tool_indicators_with_scroll_anchor(4, reading_anchor));
        assert_eq!(
            task.message_hashes, hashes_before,
            "本场景必须走就地补丁路径"
        );
        let cache = task.cache.read();
        let anchor = cache.scroll_anchor.expect("inline 更新必须恢复阅读位置");
        assert_eq!(anchor.position, position);
        assert_eq!(anchor.scroll_revision, 11);
        assert!(anchor.visual_row > before, "就地补丁的换行必须实际增加高度");
        assert_eq!(
            Some(anchor.visual_row),
            cache.visual_row_for_anchor(position),
            "不能保留换行更新前的旧视觉行号"
        );
        assert_eq!(cache.viewport_anchor_at(anchor.visual_row), Some(position));
    }
}

#[test]
fn test_tick_refresh_preserves_latest_scroll_revision_for_rebuild_and_inline_patch() {
    for inline in [false, true] {
        for reading_is_newer in [false, true] {
            let mut task = if inline {
                make_inline_task()
            } else {
                let mut task = make_task();
                task.rebuild_safe(vec![make_running_message("bash"), make_history()]);
                make_elapsed(&mut task.last_messages[0]);
                task
            };
            let pending_position = make_reading_anchor(&task.cache.read(), 5);
            let reading_position = make_reading_anchor(&task.cache.read(), 20);
            assert_ne!(pending_position, reading_position, "新旧阅读位置必须不同");
            let (pending_revision, reading_revision, expected_position) = if reading_is_newer {
                (12, 13, reading_position)
            } else {
                (13, 12, pending_position)
            };
            let pending_row = task
                .cache
                .read()
                .visual_row_for_anchor(pending_position)
                .expect("应能解析待消费锚点");
            task.cache.write().scroll_anchor = Some(ScrollAnchor {
                position: pending_position,
                visual_row: pending_row,
                scroll_revision: pending_revision,
            });
            assert!(task.refresh_running_tool_indicators_with_scroll_anchor(
                4,
                Some((reading_position, reading_revision)),
            ));
            let cache = task.cache.read();
            let anchor = cache.scroll_anchor.expect("布局刷新必须保留最新位置");
            assert_eq!(anchor.position, expected_position);
            assert_eq!(anchor.scroll_revision, 13, "旧位置版本不能覆盖新阅读位置");
            assert_eq!(
                cache.viewport_anchor_at(anchor.visual_row),
                Some(expected_position)
            );
        }
    }
}

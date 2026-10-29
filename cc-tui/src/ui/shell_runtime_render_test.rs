use super::*;

fn make_task() -> RenderTask {
    RenderTask {
        last_messages: Vec::new(),
        message_lines: Vec::new(),
        message_links: Vec::new(),
        message_hashes: Vec::new(),
        cache: Arc::new(RwLock::new(RenderCache::new())),
        notify: Arc::new(Notify::new()),
        width: 100,
        show_tool_messages: false,
        diff_visible: false,
        hook_rows_visible: true,
        detail_mode: false,
        theme_fingerprint: crate::ui::theme::fingerprint(),
        alt_lines: Vec::new(),
        alt_links: Vec::new(),
        alt_keys: Vec::new(),
        alt_epochs: Vec::new(),
        line_epochs: Vec::new(),
        epoch_seq: 0,
        wrap_segs: [Vec::new(), Vec::new()],
    }
}

fn make_bash() -> MessageViewModel {
    let mut vm = MessageViewModel::tool_block_with_id(
        "bash-call".into(),
        "Bash".into(),
        "Bash".into(),
        Some("sleep 30".into()),
        false,
    );
    if let MessageViewModel::ToolBlock {
        phase,
        started_at,
        timeout_ms,
        ..
    } = &mut vm
    {
        *phase = ToolPhase::Running;
        *started_at = Some(Instant::now());
        *timeout_ms = Some(300_000);
    }
    vm.recompute_hash();
    vm
}

fn set_elapsed(vm: &mut MessageViewModel, seconds: u64) {
    match vm {
        MessageViewModel::ToolBlock { started_at, .. } => {
            *started_at = Some(Instant::now() - Duration::from_secs(seconds))
        }
        MessageViewModel::SubAgentGroup {
            recent_messages, ..
        } => set_elapsed(&mut recent_messages[0], seconds),
        _ => panic!("应为正在执行的 Bash"),
    }
}

#[test]
fn test_shell_runtime_ticker_refreshes_nested_and_main_bash_with_deadline() {
    for nested in [false, true] {
        let mut task = make_task();
        let vm = if nested {
            let mut group = MessageViewModel::subagent_group("child".into(), "task".into());
            if let MessageViewModel::SubAgentGroup {
                recent_messages,
                collapsed,
                ..
            } = &mut group
            {
                *collapsed = false;
                recent_messages.push(make_bash());
            }
            group.recompute_hash();
            group
        } else {
            make_bash()
        };
        task.rebuild(vec![vm]);
        assert!(task.has_running_tool_blocks(), "嵌套命令也必须启动刷新时钟");
        assert!(!task
            .cache
            .read()
            .lines
            .iter()
            .any(|l| l.to_string().contains(CONTROL_B_BACKGROUND_HINT)));
        for seconds in [3, 5] {
            set_elapsed(&mut task.last_messages[0], seconds);
            assert!(
                task.refresh_running_tool_indicators(0),
                "无新输出也应刷新耗时"
            );
            let cache = task.cache.read();
            assert!(
                cache.lines.iter().any(|l| l
                    .to_string()
                    .contains(&format!("Running… ({seconds}s · timeout 5m)"))),
                "累计耗时和硬期限必须同时保留: {:?}",
                cache.lines
            );
            assert!(cache
                .lines
                .iter()
                .any(|l| l.to_string().contains(CONTROL_B_BACKGROUND_HINT)));
        }
    }
}

fn make_streaming_bash() -> MessageViewModel {
    let mut vm = make_bash();
    if let MessageViewModel::ToolBlock { live_output, .. } = &mut vm {
        *live_output = Some(crate::ui::message_view::ToolOutputPreview::new(
            "one\ntwo\nthree\nfour\nfive\nsix\nseven".into(),
            7,
            2048,
            1000,
        ));
    }
    vm.recompute_hash();
    vm
}

#[test]
fn test_streamed_bash_tick_reveals_cached_tail_after_threshold_and_refreshes_wrap() {
    for nested in [false, true] {
        let mut task = make_task();
        let mut vm = make_streaming_bash();
        if nested {
            if let MessageViewModel::ToolBlock {
                source_agent_id, ..
            } = &mut vm
            {
                *source_agent_id = Some("child".into());
            }
            let mut group = MessageViewModel::subagent_group("child".into(), "task".into());
            if let MessageViewModel::SubAgentGroup {
                recent_messages,
                collapsed,
                ..
            } = &mut group
            {
                *collapsed = false;
                recent_messages.push(vm);
            }
            group.recompute_hash();
            vm = group;
        }
        task.rebuild(vec![vm]);
        assert!(
            !task
                .cache
                .read()
                .lines
                .iter()
                .any(|line| line.to_string().contains("three")),
            "不足两秒不显示缓存尾部"
        );
        for seconds in [3, 10] {
            set_elapsed(&mut task.last_messages[0], seconds);
            assert!(
                task.refresh_running_tool_indicators(4),
                "无新输出也应跨阈值并更新计时"
            );
            let cache = task.cache.read();
            let text = cache
                .lines
                .iter()
                .map(Line::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                text.contains("three") && text.contains("seven") && text.contains("… +2 lines")
            );
            assert!(
                text.contains(&format!("({seconds}s · timeout 5m · 2.0 KB)")),
                "输出下方状态尾注需要逐秒更新: {text}"
            );
            assert!(!text.contains("Running…"));
            assert_eq!(text.contains(CONTROL_B_BACKGROUND_HINT), !nested);
            assert_eq!(cache.lines.len(), cache.wrap_map.len());
            for (line, wrap) in cache.lines.iter().zip(&cache.wrap_map) {
                assert_eq!(
                    line.to_string(),
                    wrap.plain_text,
                    "行内刷新后wrap快照必须同步"
                );
            }
        }
    }
}

#[test]
fn test_streamed_bash_detail_toggle_and_finish_invalidate_both_render_variants() {
    let mut task = make_task();
    let mut vm = make_streaming_bash();
    set_elapsed(&mut vm, 3);
    task.rebuild(vec![vm]);
    let (_, mut receiver) = mpsc::channel(1);
    task.handle_event(RenderEvent::ToggleDetail(true), &mut receiver);
    let text = task
        .cache
        .read()
        .lines
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("one") && !text.contains("… +2 lines"),
        "详细模式应读取全量保留尾部"
    );
    let mut finished = task.last_messages[0].clone();
    if let MessageViewModel::ToolBlock {
        phase,
        content,
        live_output,
        started_at,
        ..
    } = &mut finished
    {
        *phase = ToolPhase::Finished;
        *content = "final-result".into();
        *live_output = None;
        *started_at = None;
    }
    finished.recompute_hash();
    task.rebuild(vec![finished]);
    task.handle_event(RenderEvent::ToggleDetail(false), &mut receiver);
    let text = task
        .cache
        .read()
        .lines
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("final-result"));
    assert!(
        !text.contains("seven") && !text.contains("ctrl+b") && !text.contains("2.0 KB"),
        "完成后普通/详细缓存均不得残留运行输出"
    );
}

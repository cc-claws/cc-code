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
        detail_mode: false,
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
        started_at,
        execution_timeout_ms,
        ..
    } = &mut vm
    {
        *started_at = Some(Instant::now());
        *execution_timeout_ms = Some(300_000);
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
                    .contains(&format!("Running… ({seconds}s)    (timeout 5m)"))),
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

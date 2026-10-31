use super::{
    tools::{AgentSummary, BackgroundTaskStarted, ToolCategory, ToolEntry},
    MessageViewModel,
};

fn aggregatable_category(vm: &MessageViewModel) -> Option<ToolCategory> {
    let MessageViewModel::ToolBlock {
        tool_name,
        file_inspect,
        shell_backgrounded,
        content,
        ..
    } = vm
    else {
        return None;
    };

    // 带 hook 执行行的块照常参与聚合：聚合 = 用户主动折叠（完全信任），
    // 折叠组不承载 hook 行，展开也只显示工具内容——hook 行只属于单独显示的块。

    // 对齐参考实现：运行中的文件查询 Bash 也进组（计数含在跑的这条）。
    // 仍排除已转后台 / 后台启动标记的——它们要留在单条里显示后台状态。
    if tool_name == "Bash"
        && (!file_inspect
            || *shell_backgrounded
            || BackgroundTaskStarted::parse(content).is_some())
    {
        return None;
    }

    ToolCategory::from_tool_block(tool_name, *file_inspect)
}

/// ToolBlock 按值拆字段进 ToolEntry（移动，不 clone）。调用方须已确认是 ToolBlock。
fn tool_block_entry(vm: MessageViewModel) -> ToolEntry {
    let MessageViewModel::ToolBlock {
        tool_name,
        display_name,
        args_display,
        content,
        is_error,
        file_inspect,
        ..
    } = vm
    else {
        unreachable!("进组项必为 ToolBlock")
    };
    ToolEntry {
        tool_name,
        display_name,
        args_display,
        content,
        is_error,
        file_inspect,
    }
}

/// 将 view_messages 中相邻的只读 ToolBlock 聚合为 ToolCallGroup（支持跨类别，跳过空 thinking bubble）
pub fn aggregate_tool_groups(messages: &mut Vec<MessageViewModel>) {
    aggregate_tail_tool_groups(messages, 0);
}

/// 从 `from_idx` 开始聚合尾部相邻的只读 ToolBlock。
/// `from_idx` 之前的消息保持不变（已聚合的部分不需要重新处理）。
pub fn aggregate_tail_tool_groups(messages: &mut Vec<MessageViewModel>, from_idx: usize) {
    if from_idx >= messages.len() {
        return;
    }
    // #757：整列取走后逐条移动重建。passthrough 项与进组项均不再 clone：
    // 进组项的字段直接移动进 ToolEntry，组内被 reasoning-only 跳过的项丢弃，
    // 输出内容与旧实现逐条一致。
    let capacity = messages.len();
    let mut iter = std::mem::take(messages).into_iter().peekable();
    let mut result: Vec<MessageViewModel> = Vec::with_capacity(capacity);
    for _ in 0..from_idx {
        if let Some(vm) = iter.next() {
            result.push(vm);
        }
    }
    while let Some(vm) = iter.next() {
        let Some(cat) = aggregatable_category(&vm) else {
            result.push(vm);
            continue;
        };
        let mut entries = vec![tool_block_entry(vm)];
        while let Some(next) = iter.peek() {
            if let Some(entry_cat) = aggregatable_category(next) {
                if (cat == ToolCategory::AskUser) == (entry_cat == ToolCategory::AskUser) {
                    let item = iter.next().expect("peeked item");
                    entries.push(tool_block_entry(item));
                    continue;
                }
            }
            if next.is_reasoning_only() {
                iter.next();
                continue;
            }
            break;
        }
        let mut vm = MessageViewModel::ToolCallGroup {
            thinking: None,
            category: cat,
            tools: entries,
            collapsed: true,
            standalone_action: None,
            content_hash: 0,
        };
        vm.recompute_hash();
        result.push(vm);
    }

    *messages = result;
}

/// SubAgentGroup 按引用取批次摘要（截断字段仍需新串，与旧实现一致）。
fn agent_summary(vm: &MessageViewModel) -> AgentSummary {
    let MessageViewModel::SubAgentGroup {
        agent_id,
        task_preview,
        total_steps,
        is_error,
        final_result,
        ..
    } = vm
    else {
        unreachable!("摘要来源必为 SubAgentGroup")
    };
    AgentSummary {
        agent_id: agent_id.clone(),
        task_preview: task_preview.chars().take(50).collect(),
        tool_count: *total_steps,
        is_error: *is_error,
        final_result: final_result
            .as_ref()
            .map(|r| r.lines().next().unwrap_or("").chars().take(80).collect()),
    }
}

/// 将连续的、已完成的 SubAgentGroup 聚合为批次汇总视图。
///
/// 扫描 messages，找到连续的、`batch_agents` 为空且非运行中的 SubAgentGroup 区间，
/// 区间长度 > 1 时合并为一个带 `batch_agents` 的汇总 VM，默认折叠。
/// 流式期间 `is_running: true` 的 VM 不参与聚合。
pub fn aggregate_batch_groups(messages: &mut Vec<MessageViewModel>) {
    if messages.is_empty() {
        return;
    }

    // #757：整列取走后逐条移动重建，passthrough 项不再 clone；
    // 组首条复用为汇总 VM（原地改 batch_agents/collapsed），其余组内条目只取摘要后丢弃。
    let mut iter = std::mem::take(messages).into_iter().peekable();
    let mut result: Vec<MessageViewModel> = Vec::new();

    while let Some(mut vm) = iter.next() {
        let is_aggregatable = matches!(
            &vm,
            MessageViewModel::SubAgentGroup {
                is_running: false,
                batch_agents,
                ..
            } if batch_agents.is_empty()
        );
        if !is_aggregatable {
            result.push(vm);
            continue;
        }

        let mut batch_summaries = vec![agent_summary(&vm)];
        let mut run_len = 1;
        while let Some(next) = iter.peek() {
            let is_next = matches!(
                next,
                MessageViewModel::SubAgentGroup {
                    is_running: false,
                    batch_agents,
                    ..
                } if batch_agents.is_empty()
            );
            if !is_next {
                break;
            }
            let item = iter.next().expect("peeked item");
            batch_summaries.push(agent_summary(&item));
            run_len += 1;
        }

        if run_len <= 1 {
            result.push(vm);
        } else {
            if let MessageViewModel::SubAgentGroup {
                ref mut batch_agents,
                ref mut collapsed,
                ..
            } = vm
            {
                *batch_agents = batch_summaries;
                *collapsed = true;
            }
            vm.recompute_hash();
            result.push(vm);
        }
    }

    *messages = result;
}

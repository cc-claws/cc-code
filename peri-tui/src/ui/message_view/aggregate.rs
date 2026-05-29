use super::{
    tools::{AgentSummary, ToolCategory, ToolEntry},
    MessageViewModel,
};

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
    let mut result: Vec<MessageViewModel> = Vec::with_capacity(messages.len());
    result.extend(messages[..from_idx].iter().cloned());

    let mut i = from_idx;
    let original_len = messages.len();
    while i < original_len {
        let vm = &messages[i];
        if let MessageViewModel::ToolBlock { tool_name, .. } = vm {
            if let Some(cat) = ToolCategory::from_tool_name(tool_name) {
                let mut entries: Vec<ToolEntry> = Vec::new();
                let mut j = i;
                while j < original_len {
                    if let MessageViewModel::ToolBlock {
                        tool_name: tn,
                        display_name,
                        args_display,
                        content,
                        is_error,
                        ..
                    } = &messages[j]
                    {
                        let entry_cat = ToolCategory::from_tool_name(tn);
                        if entry_cat.is_some()
                            && (cat == ToolCategory::AskUser)
                                == (entry_cat == Some(ToolCategory::AskUser))
                        {
                            entries.push(ToolEntry {
                                tool_name: tn.clone(),
                                display_name: display_name.clone(),
                                args_display: args_display.clone(),
                                content: content.clone(),
                                is_error: *is_error,
                            });
                            j += 1;
                            continue;
                        }
                    }
                    if messages[j].is_reasoning_only() {
                        j += 1;
                        continue;
                    }
                    break;
                }
                result.push(MessageViewModel::ToolCallGroup {
                    category: cat,
                    tools: entries,
                    collapsed: true,
                });
                i = j;
                continue;
            }
        }
        result.push(messages[i].clone());
        i += 1;
    }

    *messages = result;
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

    let mut result: Vec<MessageViewModel> = Vec::with_capacity(messages.len());
    let mut i = 0;
    let len = messages.len();

    while i < len {
        let is_aggregatable = matches!(
            &messages[i],
            MessageViewModel::SubAgentGroup {
                is_running: false,
                batch_agents,
                ..
            } if batch_agents.is_empty()
        );

        if !is_aggregatable {
            result.push(messages[i].clone());
            i += 1;
            continue;
        }

        let run_start = i;
        let mut batch_summaries: Vec<AgentSummary> = Vec::new();

        while i < len {
            if let MessageViewModel::SubAgentGroup {
                agent_id,
                task_preview,
                total_steps,
                is_running: false,
                is_error,
                final_result,
                batch_agents,
                ..
            } = &messages[i]
            {
                if batch_agents.is_empty() {
                    batch_summaries.push(AgentSummary {
                        agent_id: agent_id.clone(),
                        task_preview: task_preview.chars().take(50).collect(),
                        tool_count: *total_steps,
                        is_error: *is_error,
                        final_result: final_result
                            .as_ref()
                            .map(|r| r.lines().next().unwrap_or("").chars().take(80).collect()),
                    });
                    i += 1;
                    continue;
                }
            }
            break;
        }

        let run_len = i - run_start;
        if run_len <= 1 {
            result.push(messages[run_start].clone());
        } else {
            let mut merged = messages[run_start].clone();
            if let MessageViewModel::SubAgentGroup {
                ref mut batch_agents,
                ref mut collapsed,
                ..
            } = merged
            {
                *batch_agents = batch_summaries;
                *collapsed = true;
            }
            result.push(merged);
        }
    }

    *messages = result;
}

use peri_agent::messages::BaseMessage;

use crate::{
    app::tool_display,
    ui::{
        message_view::{aggregate_tool_groups, tool_color, ContentBlockView, MessageViewModel},
        theme,
    },
};

pub use crate::ui::message_view::aggregate_batch_groups;

use super::MessagePipeline;

/// 从工具名和入参构造预渲染的 diff 行（仅 Write/Edit 工具）
fn try_build_diff_lines(
    name: &str,
    input: &serde_json::Value,
) -> Option<Vec<ratatui::text::Line<'static>>> {
    let diff_input = match name {
        "Edit" => {
            let old_string = input
                .get("old_string")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new_string = input
                .get("new_string")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if old_string.is_empty() || file_path.is_empty() {
                return None;
            }
            Some(peri_widgets::DiffInput {
                file_path: file_path.to_string(),
                old_content: old_string.to_string(),
                new_content: new_string.to_string(),
                is_new_file: false,
                is_deleted_file: false,
                is_binary: false,
            })
        }
        "Write" => {
            let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let file_path = input
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if content.is_empty() || file_path.is_empty() {
                return None;
            }
            Some(peri_widgets::DiffInput {
                file_path: file_path.to_string(),
                old_content: String::new(),
                new_content: content.to_string(),
                is_new_file: true,
                is_deleted_file: false,
                is_binary: false,
            })
        }
        _ => None,
    }?;
    let lines = peri_widgets::diff::render_diff(&diff_input, 80, &peri_widgets::DarkTheme);
    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

// ─── 管线事件 ────────────────────────────────────────────────────────────────

/// 管线处理事件后的输出动作
#[derive(Debug)]
pub enum PipelineAction {
    /// 无 UI 变化
    None,
    /// 新增消息（外部通知 + 用户消息）
    AddMessage(MessageViewModel),
    /// 尾部重建（prefix_len 标记不变前缀长度，tail_vms 存储重建尾部）
    RebuildAll {
        prefix_len: usize,
        tail_vms: Vec<MessageViewModel>,
    },
}

/// 合并冻结的 SubAgentGroup VM 到 reconcile 重建后的新 VMs 中，防止 Done 后 SubAgent 显示退化。
///
/// `frozen_vms` 是 SubAgentEnd 时构建的完整 SubAgentGroup VM（含 recent_messages、final_result 等），
/// 按 `agent_id` 匹配替换新 VMs 中的 SubAgentGroup 占位符。
///
/// 匹配策略：优先用 `instance_id`（如果两边都有值）精确匹配；
/// 回退到 `agent_id` 匹配（reconcile VM 的 instance_id 为 None 时的兼容路径）。
/// 对于同一 `agent_id` 的多个 VM，使用位置匹配保证一一对应。
///
/// 返回未匹配的冻结 VM 索引集合（供调用方决定是否追加到 tail_vms）。
pub(crate) fn merge_frozen_subagents(
    frozen_vms: &[MessageViewModel],
    new_vms: &mut [MessageViewModel],
) -> Vec<usize> {
    if frozen_vms.is_empty() {
        return Vec::new();
    }

    // 收集 reconcile 中 SubAgentGroup 的索引
    let new_subagent_indices: Vec<usize> = new_vms
        .iter()
        .enumerate()
        .filter(|(_, vm)| vm.is_subagent_group())
        .map(|(i, _)| i)
        .collect();

    let mut matched_frozen = vec![false; frozen_vms.len()];

    // 第一轮：用 instance_id 精确匹配（frozen 有 instance_id，reconcile 可能有也可能没有）
    for (fi, frozen_vm) in frozen_vms.iter().enumerate() {
        if matched_frozen[fi] {
            continue;
        }
        if let MessageViewModel::SubAgentGroup {
            instance_id: Some(frozen_iid),
            ..
        } = frozen_vm
        {
            // 尝试在 new_vms 中找到 instance_id 匹配的 SubAgentGroup
            for &ni in &new_subagent_indices {
                if let MessageViewModel::SubAgentGroup {
                    instance_id: Some(new_iid),
                    ..
                } = &new_vms[ni]
                {
                    if frozen_iid == new_iid {
                        new_vms[ni] = frozen_vm.clone();
                        matched_frozen[fi] = true;
                        break;
                    }
                }
            }
        }
    }

    // 第二轮：用 agent_id + 位置匹配（reconcile VM 的 instance_id 为 None）
    for (fi, frozen_vm) in frozen_vms.iter().enumerate() {
        if matched_frozen[fi] {
            continue;
        }
        if let MessageViewModel::SubAgentGroup {
            agent_id: frozen_aid,
            ..
        } = frozen_vm
        {
            for &ni in &new_subagent_indices {
                if let MessageViewModel::SubAgentGroup {
                    agent_id: new_aid, ..
                } = &new_vms[ni]
                {
                    if frozen_aid == new_aid {
                        new_vms[ni] = frozen_vm.clone();
                        matched_frozen[fi] = true;
                        break;
                    }
                }
            }
        }
    }

    // 返回未匹配的冻结 VM 索引
    matched_frozen
        .iter()
        .enumerate()
        .filter(|(_, &m)| !m)
        .map(|(i, _)| i)
        .collect()
}

impl MessagePipeline {
    /// 构建 RebuildAll action（用于外部 agent_ops 显式触发重建）。
    /// 由调用者提供 prefix_len（round_start_vm_idx），pipeline 内部不维护 VM 索引。
    pub fn build_rebuild_all(&self, prefix_len: usize) -> PipelineAction {
        let tail_vms = self.build_tail_vms();
        PipelineAction::RebuildAll {
            prefix_len,
            tail_vms,
        }
    }

    /// 从 pipeline 规范状态构建尾部 VMs。
    ///
    /// 两种情况：
    /// - has_snapshot_this_round == true：从 completed[last_human..] reconcile + streaming + pending tools
    /// - has_snapshot_this_round == false（Case 1）：跳过 reconcile，只输出 streaming + pending tools
    pub(crate) fn build_tail_vms(&self) -> Vec<MessageViewModel> {
        let mut tail_vms = Vec::new();

        if self.has_snapshot_this_round {
            let start = self.completed_len_at_round_start.min(self.completed.len());
            let round_completed = &self.completed[start..];
            let last_human_offset = round_completed
                .iter()
                .rposition(|msg| matches!(msg, BaseMessage::Human { .. }))
                .map(|idx| idx + start)
                .unwrap_or(start);
            tail_vms =
                Self::messages_to_view_models(&self.completed[last_human_offset..], &self.cwd);
            let reconcile_subagent_count =
                tail_vms.iter().filter(|vm| vm.is_subagent_group()).count();
            tracing::debug!(
                has_snapshot = true,
                completed_len = self.completed.len(),
                start_offset = start,
                last_human_offset,
                reconcile_total = tail_vms.len(),
                reconcile_subagent_count,
                frozen_count = self.frozen_subagent_vms.len(),
                "[bg-diag] build_tail_vms reconcile"
            );
        }

        // 追加流式 AssistantBubble（当前 AI 正在输出的文本）
        if self.has_streaming_content() {
            tail_vms.push(self.build_streaming_bubble());
        }

        // 追加 pending tool blocks（ToolStart 后、下一个 StateSnapshot 前的工具）
        for tc in &self.current_ai_tool_calls {
            if let Some(pending) = self.pending_tools.get(&tc.id) {
                if pending.name != "Agent" {
                    tail_vms.push(self.build_tool_start_vm(&tc.id, &pending.name, &pending.input));
                }
            }
        }

        // 追加已完成但尚未进入 completed 的工具结果
        for ct in &self.completed_tools {
            let display = tool_display::format_tool_name(&ct.name);
            let args = tool_display::format_tool_args(&ct.name, &ct.input, Some(&self.cwd));
            let diff_lines = if !ct.is_error {
                try_build_diff_lines(&ct.name, &ct.input)
            } else {
                None
            };
            let mut vm = MessageViewModel::ToolBlock {
                tool_name: ct.name.clone(),
                tool_call_id: ct.tool_call_id.clone(),
                display_name: display,
                args_display: args,
                content: ct.output.clone(),
                is_error: ct.is_error,
                collapsed: true,
                color: if ct.is_error {
                    theme::ERROR
                } else {
                    tool_color(&ct.name)
                },
                diff_lines,
                content_hash: 0,
            };
            vm.recompute_hash();
            tail_vms.push(vm);
        }

        // SubAgentGroup VMs
        if self.has_snapshot_this_round {
            let unmatched = merge_frozen_subagents(&self.frozen_subagent_vms, &mut tail_vms);
            // 将未匹配的冻结 VM（reconcile 中没有对应 SubAgentGroup 的后台 agent）
            // 直接追加到 tail_vms，防止后台 agent 从视图中消失。
            for idx in unmatched {
                if let Some(frozen) = self.frozen_subagent_vms.get(idx) {
                    tail_vms.push(frozen.clone());
                }
            }
            for sub in &self.subagent_stack {
                if sub.finalized_vm.is_none() {
                    let mut vm = MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id.clone(),
                        task_preview: sub.task_preview.clone(),
                        total_steps: sub.total_steps,
                        recent_messages: sub.recent_messages.clone(),
                        is_running: sub.is_running,
                        collapsed: false,
                        final_result: None,
                        is_error: false,
                        is_background: sub.is_background,
                        bg_hash: sub.bg_hash.clone(),
                        batch_agents: Vec::new(),
                        instance_id: Some(sub.instance_id.clone()),
                        content_hash: 0,
                    };
                    vm.recompute_hash();
                    tail_vms.push(vm);
                }
            }
        } else {
            for sub in &self.subagent_stack {
                let vm = if let Some(ref finalized) = sub.finalized_vm {
                    finalized.clone()
                } else {
                    let mut vm = MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id.clone(),
                        task_preview: sub.task_preview.clone(),
                        total_steps: sub.total_steps,
                        recent_messages: sub.recent_messages.clone(),
                        is_running: sub.is_running,
                        collapsed: false,
                        final_result: None,
                        is_error: false,
                        is_background: sub.is_background,
                        bg_hash: sub.bg_hash.clone(),
                        batch_agents: Vec::new(),
                        instance_id: Some(sub.instance_id.clone()),
                        content_hash: 0,
                    };
                    vm.recompute_hash();
                    vm
                };
                tail_vms.push(vm);
            }
        }

        aggregate_tool_groups(&mut tail_vms);

        if !self.has_streaming_content() && self.current_ai_tool_calls.is_empty() {
            aggregate_batch_groups(&mut tail_vms);
        }

        add_thinking_tail_snapshot(&mut tail_vms);

        tail_vms
    }
}

/// 提取文本的最后 `n` 行（按换行符切分，单行不截断）。
/// 返回换行分隔的字符串。
pub(crate) fn extract_tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// 扫描 tail_vms 的最后一个 AssistantBubble，
/// 若满足条件（无 Text block + 最后一个 block 是 Reasoning）则设置 tail_lines。
fn add_thinking_tail_snapshot(tail_vms: &mut [MessageViewModel]) {
    for vm in tail_vms.iter_mut().rev() {
        if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
            let has_text = blocks
                .iter()
                .any(|b| matches!(b, ContentBlockView::Text { raw, .. } if !raw.trim().is_empty()));
            if has_text {
                return;
            }
            if let Some(ContentBlockView::Reasoning {
                text, tail_lines, ..
            }) = blocks.last_mut()
            {
                let tail = extract_tail_lines(text, 3);
                if !tail.is_empty() {
                    *tail_lines = Some(tail);
                }
            }
            return;
        }
    }
}

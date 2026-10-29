use crate::{
    app::tool_display,
    ui::{
        message_view::{aggregate_tool_groups, tool_color, MessageViewModel},
        theme,
    },
};

pub use crate::ui::message_view::aggregate_batch_groups;

use super::transform::merge_consecutive_thinking;
use super::MessagePipeline;

/// 只保留一份活跃尾部的基础 VM，避免长任务为每个流式事件重做历史转换。
const COMPLETED_VIEW_CACHE_BUDGET: usize = 16 * 1024 * 1024;

pub(super) struct CompletedViewCache {
    revision: u64,
    start: usize,
    theme: u64,
    bytes: usize,
    messages: Vec<MessageViewModel>,
}

// ─── 管线事件 ────────────────────────────────────────────────────────────────

/// 管线处理事件后的输出动作
#[derive(Debug)]
pub enum PipelineAction {
    /// 无 UI 变化
    None,
    /// 新增消息（外部通知 + 用户消息）
    AddMessage(Box<MessageViewModel>),
    /// 尾部重建（prefix_len 标记不变前缀长度，tail_vms 存储重建尾部）
    RebuildAll {
        prefix_len: usize,
        tail_vms: Vec<MessageViewModel>,
    },
}

impl PipelineAction {
    /// 工具运行元数据增大 VM 后，以间接存储避免所有管线动作都携带完整 VM 大小。
    pub fn add_message(vm: MessageViewModel) -> Self {
        Self::AddMessage(Box::new(vm))
    }
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
    pub(crate) fn completed_view_cache_stats(&self) -> (usize, usize) {
        self.completed_view_cache
            .borrow()
            .as_ref()
            .map(|cache| (cache.messages.len(), cache.bytes))
            .unwrap_or_default()
    }

    pub(crate) fn invalidate_completed_view_cache(&mut self) {
        self.completed_revision = self.completed_revision.wrapping_add(1);
        *self.completed_view_cache.get_mut() = None;
    }

    pub(super) fn cached_completed_view_models(&self, start: usize) -> Vec<MessageViewModel> {
        let theme = theme::fingerprint();
        {
            let cache = self.completed_view_cache.borrow();
            if let Some(cache) = cache.as_ref().filter(|cache| {
                cache.revision == self.completed_revision
                    && cache.start == start
                    && cache.theme == theme
            }) {
                return cache.messages.clone();
            }
        }
        // 失效成品先释放，再构造新版本，不同时保留多个历史/主题的尾部。
        *self.completed_view_cache.borrow_mut() = None;
        let messages = Self::unaggregated_view_models(&self.completed[start..], &self.cwd);
        let bytes = crate::command::core::gc::estimate_view_messages_heap(&messages);
        if bytes <= COMPLETED_VIEW_CACHE_BUDGET {
            *self.completed_view_cache.borrow_mut() = Some(CompletedViewCache {
                revision: self.completed_revision,
                start,
                theme,
                bytes,
                messages: messages.clone(),
            });
        }
        messages
    }

    /// 构建 RebuildAll action（用于外部 agent_ops 显式触发重建）。
    /// 由调用者提供 prefix_len（round_start_vm_idx），pipeline 内部不维护 VM 索引。
    ///
    /// 后台子 Agent 的分组卡片（`is_background: true`）在这里被剔除：它们由底栏抽屉与
    /// 子视口承载，主时间线只保留启动摘要与完成行。
    ///
    /// 剔除必须发生在本函数（活跃轮次尾部的唯一产地），**不能**放到
    /// `apply_pipeline_action` 里对收到的整条 `tail_vms` 做——那条路径同样会收到
    /// 全量历史重放（`/rewind` 用 `RebuildAll{prefix_len: 0, 全量历史}` 表达「替换整条时间线」），
    /// 在那里剔除会把用户正在看的归档卡片一并删掉，且没有任何后续事件能把它们放回来。
    pub fn build_rebuild_all(&self, prefix_len: usize) -> PipelineAction {
        let mut tail_vms = self.build_tail_vms();
        // 曾用「`subagent_sessions` 是否为空」或会话级粘性标志当判据，二者都是
        // 会话状态代理：最后一个后台任务完成并回收后池子归空，代理立刻失效，
        // 冻结的 `◐ Agent(...)` 卡片就会漏回主消息区（底栏抽屉同时是空的）。
        // 判据就该是本函数产出的「活跃轮次尾部」本身。
        tail_vms.retain(|vm| {
            !matches!(
                vm,
                MessageViewModel::SubAgentGroup {
                    is_background: true,
                    ..
                }
            )
        });
        PipelineAction::RebuildAll {
            prefix_len,
            tail_vms,
        }
    }

    /// 从 pipeline 规范状态构建尾部 VMs。
    ///
    /// 两种情况：
    /// - has_snapshot_this_round == true：从本轮起点 reconcile + streaming + pending tools
    /// - has_snapshot_this_round == false（Case 1）：跳过 reconcile，只输出 streaming + pending tools
    pub(crate) fn build_tail_vms(&self) -> Vec<MessageViewModel> {
        let mut tail_vms = Vec::new();

        if self.has_snapshot_this_round {
            let start = self.completed_len_at_round_start.min(self.completed.len());
            // 本轮可能包含用户主动补充的 Human 消息，不能用最后一条 Human 截掉之前的执行过程。
            // 缓存只含规范历史投影；动态工具、hook、shell 和聚合仍在副本上更新。
            tail_vms = self.cached_completed_view_models(start);
            let reconcile_subagent_count =
                tail_vms.iter().filter(|vm| vm.is_subagent_group()).count();
            tracing::debug!(
                has_snapshot = true,
                completed_len = self.completed.len(),
                start_offset = start,
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
                    let mut vm = self.build_tool_start_vm(
                        &tc.id,
                        &pending.name,
                        &pending.input,
                        pending.started_at,
                    );
                    if let MessageViewModel::ToolBlock {
                        phase, live_output, ..
                    } = &mut vm
                    {
                        *phase = pending.phase;
                        *live_output = pending.live_output.clone();
                    }
                    vm.recompute_hash();
                    tail_vms.push(vm);
                }
            } else if let Some(ct) = self
                .completed_tools
                .iter()
                .find(|tool| tool.tool_call_id == tc.id)
            {
                let display = tool_display::format_tool_name_with_input(&ct.name, Some(&ct.input));
                let args = tool_display::format_tool_args(&ct.name, &ct.input, Some(&self.cwd));
                let diff_input = if !ct.is_error {
                    tool_display::build_diff_input(
                        &ct.name,
                        &ct.input,
                        Some(&self.cwd),
                        Some(&ct.tool_call_id),
                    )
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
                        theme::cur().error
                    } else {
                        tool_color(&ct.name)
                    },
                    diff_input,
                    phase: crate::ui::message_view::ToolPhase::Finished,
                    live_output: None,
                    source_agent_id: None,
                    started_at: None,
                    timeout_ms: ct.input.get("timeout").and_then(serde_json::Value::as_u64),
                    shell_backgrounded: false,
                    file_inspect: crate::ui::message_view::is_file_inspect(&ct.name, &ct.input),
                    hook_rows: Vec::new(),
                    content_hash: 0,
                };
                vm.recompute_hash();
                tail_vms.push(vm);
            }
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

        self.apply_shell_runtime(&mut tail_vms);
        // 每次在基础投影副本上贴最新 hook 行；随后沿用聚合规则，折叠组内不展示这些行。
        self.apply_hook_rows(&mut tail_vms);
        aggregate_tool_groups(&mut tail_vms);
        // Re-run after appending streaming/pending tools so a completed tool group
        // can be merged into the reasoning bubble currently being streamed.
        merge_consecutive_thinking(&mut tail_vms);

        if !self.has_streaming_content() && self.current_ai_tool_calls.is_empty() {
            aggregate_batch_groups(&mut tail_vms);
        }

        // 本轮仍在进行时，把最后一个思考行标成 active（渲染 `● Thinking for Ns, …`）
        self.mark_live_thinking(&mut tail_vms);

        tail_vms
    }
}

#[cfg(test)]
#[path = "completed_view_cache_test.rs"]
mod completed_view_cache_tests;

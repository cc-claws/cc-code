//! 统一消息渲染管线 (Unified Message Rendering Pipeline)
//!
//! 核心设计：所有 `MessageViewModel` 的产生都经过单一转换函数
//! `messages_to_view_models(base_messages, cwd)`。
//!
//! # 两条路径
//!
//! ```text
//!   流式事件 ──→ 增量更新 BaseMessage[] ──→ reconcile ──→ MessageViewModel[]
//!   历史恢复 ──→ BaseMessage[]            ──→ 直接转换  ──→ MessageViewModel[]
//!                                    ↑
//!                      同一个 messages_to_view_models()
//! ```
//!
//! # 流式 UX 优化
//!
//! `AssistantChunk` 使用 `AppendChunk` 直接操作渲染层（避免每字符重做 markdown），
//! 但在 "finalize 边界"（ToolStart / ToolEnd / Done）会 reconcile 最后的
//! AssistantBubble，确保最终状态与 restore 路径完全一致。

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use peri_agent::messages::{BaseMessage, ToolCallRequest};

use crate::app::{events::AgentEvent, tool_display};
#[allow(unused_imports)]
use crate::ui::message_view::{aggregate_tool_groups, ContentBlockView, MessageViewModel};

mod reconcile;
mod transform;

pub use crate::ui::message_view::aggregate_batch_groups;
pub use reconcile::PipelineAction;
#[allow(unused_imports)]
pub(crate) use reconcile::{
    add_thinking_tail_snapshot, extract_tail_lines, merge_frozen_subagents,
};

// ─── 管线内部状态 ────────────────────────────────────────────────────────────

/// 已开始但未结束的工具调用
pub(crate) struct PendingTool {
    #[allow(dead_code)]
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
}

/// ToolEnd 后、StateSnapshot 前的工具结果（用于在 reconcile gap 期间显示）
pub(crate) struct CompletedTool {
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
    output: String,
    is_error: bool,
}

/// 从字符串生成短 hash（FNV-1a，6 位十六进制，确定性）。
///
/// 用于为每个 Agent 实例生成唯一的显示标识符。
fn instance_hash(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:06x}", hash as u32)
}

/// 从后台任务结果字符串中解析 task_id 短格式（前 8 位）。
///
/// 输入格式: `"Background task bg-{uuid} started..."`
/// 输出: `Some("{前8位}")` 或 `None`（解析失败时优雅降级）
fn parse_bg_hash(result: &str) -> Option<String> {
    result
        .strip_prefix("Background task bg-")
        .and_then(|rest| rest.split(' ').next())
        .map(|uuid| uuid.chars().take(8).collect())
}

/// 活跃 SubAgent 执行状态
pub(crate) struct SubAgentState {
    /// subagent_type，仅用于显示
    agent_id: String,
    /// 唯一实例标识符，用于路由
    instance_id: String,
    task_preview: String,
    total_steps: usize,
    /// 流式期间的内部消息（不持久化）
    recent_messages: Vec<MessageViewModel>,
    is_running: bool,
    /// SubAgentEnd 时固化的完整 VM（含 recent_messages、final_result 等）
    finalized_vm: Option<MessageViewModel>,
    /// 是否为后台 agent
    is_background: bool,
    /// Agent 实例的短显示标识符（6 位十六进制）
    bg_hash: Option<String>,
}

/// 批次检测状态：跟踪连续的 SubAgentStart/SubAgentEnd
struct BatchInfo {
    /// 已开始的 agent 数
    started: usize,
    /// 已完成的 agent 数
    completed: usize,
    /// 批次开始时的 subagent_stack 深度（用于交叉验证）
    #[allow(dead_code)]
    stack_depth: usize,
}

// ─── MessagePipeline ─────────────────────────────────────────────────────────

/// 统一消息渲染管线。
///
/// 维护规范 `BaseMessage[]` 状态，通过单一转换函数 `messages_to_view_models()`
/// 产生 `MessageViewModel`。流式和恢复共享同一个转换路径。
pub struct MessagePipeline {
    cwd: String,
    /// 已完成的 BaseMessages（规范状态，可用于持久化）
    completed: Vec<BaseMessage>,
    /// 当前正在流式构建的 AI 文本
    current_ai_text: String,
    /// 当前正在流式构建的 AI 推理内容
    current_ai_reasoning: String,
    /// 当前 AI 消息中的 tool_calls（由 ToolStart 事件积累）
    current_ai_tool_calls: Vec<ToolCallRequest>,
    /// 当前 AI 消息是否已 finalize（ToolStart 到达后 finalize）
    current_ai_finalized: bool,
    /// 已开始但未结束的工具调用
    pending_tools: HashMap<String, PendingTool>,
    /// ToolEnd 后、StateSnapshot 前的工具结果（在 reconcile gap 期间显示）
    completed_tools: Vec<CompletedTool>,
    /// SubAgent 栈
    subagent_stack: Vec<SubAgentState>,
    /// 冻结的 SubAgentGroup VMs（SubAgentEnd 时构建，done() 时收集）
    frozen_subagent_vms: Vec<MessageViewModel>,
    /// 批次检测状态（连续的 SubAgentStart/SubAgentEnd 跟踪）
    active_batch: Option<BatchInfo>,
    // ── 节流状态 ──
    /// 是否有待发射的节流 RebuildAll（有流式 chunk 积累但尚未发射）
    throttle_armed: bool,
    /// 上次节流发射的时间
    throttle_last_fire: Option<Instant>,
    // ── 轮次追踪 ──
    /// 本轮开始时 completed 的长度（用于区分首轮 StateSnapshot 前/后）
    completed_len_at_round_start: usize,
    /// 本轮是否收到过 StateSnapshot
    has_snapshot_this_round: bool,
}

impl MessagePipeline {
    pub fn new(cwd: String) -> Self {
        Self {
            cwd,
            completed: Vec::new(),
            current_ai_text: String::new(),
            current_ai_reasoning: String::new(),
            current_ai_tool_calls: Vec::new(),
            current_ai_finalized: false,
            pending_tools: HashMap::new(),
            completed_tools: Vec::new(),
            subagent_stack: Vec::new(),
            frozen_subagent_vms: Vec::new(),
            active_batch: None,
            throttle_armed: false,
            throttle_last_fire: None,
            completed_len_at_round_start: 0,
            has_snapshot_this_round: false,
        }
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// 统一事件处理入口：将 AgentEvent 转换为 PipelineAction 列表。
    /// 所有事件只更新 pipeline 内部状态，返回 None。
    /// RebuildAll 由 agent_ops 通过 `check_throttle()` 或 `build_rebuild_all()` 显式触发。
    pub fn handle_event(&mut self, event: AgentEvent) -> Vec<PipelineAction> {
        match event {
            AgentEvent::AssistantChunk {
                chunk,
                source_agent_id,
            } => {
                if !chunk.is_empty() {
                    if let Some(ref aid) = source_agent_id {
                        if let Some(sub) = self.find_running_subagent_mut(aid) {
                            Self::push_chunk_to_subagent(sub, &chunk);
                        }
                    } else if self.in_subagent() {
                        // 顺序执行时 last() 就是当前 subagent（事件顺序到达）
                        if let Some(sub) = self.subagent_stack.last_mut() {
                            Self::push_chunk_to_subagent(sub, &chunk);
                        }
                    } else {
                        self.push_chunk(&chunk);
                    }
                    self.throttle_armed = true;
                }
                vec![PipelineAction::None]
            }
            AgentEvent::AiReasoning(text) => {
                if self.in_subagent() {
                    // SubAgent 内部推理：更新 subagent 状态，arm throttle
                    if let Some(_sub) = self.subagent_stack.last_mut() {
                        // 推理内容不直接显示，但需要 arm throttle 以刷新 SubAgentGroup
                    }
                    self.throttle_armed = true;
                } else {
                    self.push_reasoning(&text);
                    self.throttle_armed = true;
                }
                vec![PipelineAction::None]
            }
            AgentEvent::ToolStart {
                tool_call_id,
                name,
                display: _,
                args: _,
                input,
                source_agent_id,
            } => {
                // 仅解除 throttle，不在此处触发 RebuildAll。
                // agent_ops 中的 request_rebuild() 会以正确的 prefix_len
                // (= round_start_vm_idx) 触发重建，同时包含流式文本和工具调用。
                // 之前此处使用 prefix_len: 0 会导致 view_messages 被全部替换，
                // 随后 request_rebuild() 用旧的 round_start_vm_idx 做 drain 时 panic。
                self.throttle_armed = false;

                if let Some(ref aid) = source_agent_id {
                    let cwd = self.cwd.clone();
                    if let Some(sub) = self.find_running_subagent_mut(aid) {
                        Self::push_tool_start_to_subagent(sub, &tool_call_id, &name, &input, &cwd);
                    }
                } else if self.in_subagent() {
                    // 顺序执行时 last() 就是当前 subagent
                    let cwd = self.cwd.clone();
                    if let Some(sub) = self.subagent_stack.last_mut() {
                        Self::push_tool_start_to_subagent(sub, &tool_call_id, &name, &input, &cwd);
                    }
                } else if name == "Agent" {
                    // 父 Agent 调用 Agent 工具：只注册 tool_call 和 pending_tool，
                    // 不创建 SubAgentState（SubAgentStart 事件会处理）。
                    // 避免与 SubAgentStart 的 tool_start_internal 产生重复条目。
                    self.finalize_current_ai();
                    self.current_ai_tool_calls.push(ToolCallRequest::new(
                        &tool_call_id,
                        &name,
                        input.clone(),
                    ));
                    self.pending_tools.insert(
                        tool_call_id.to_string(),
                        PendingTool {
                            tool_call_id: tool_call_id.to_string(),
                            name: name.to_string(),
                            input,
                        },
                    );
                } else {
                    self.tool_start_internal(&tool_call_id, &name, input, false);
                }

                vec![PipelineAction::None]
            }
            AgentEvent::ToolEnd {
                tool_call_id,
                name,
                output,
                is_error,
                source_agent_id,
            } => {
                self.throttle_armed = false;
                if let Some(ref aid) = source_agent_id {
                    if let Some(sub) = self.find_running_subagent_mut(aid) {
                        Self::update_tool_end_in_subagent(sub, &tool_call_id, &output, is_error);
                    }
                } else if self.in_subagent() {
                    // 顺序执行时 last() 就是当前 subagent
                    if let Some(sub) = self.subagent_stack.last_mut() {
                        Self::update_tool_end_in_subagent(sub, &tool_call_id, &output, is_error);
                    }
                } else {
                    self.tool_end_internal(&tool_call_id, &name, &output, is_error);
                }
                vec![PipelineAction::None]
            }
            AgentEvent::SubAgentStart {
                agent_id,
                instance_id,
                task_preview,
                is_background,
            } => {
                let input =
                    serde_json::json!({"subagent_type": &agent_id, "prompt": &task_preview});
                self.tool_start_internal(&instance_id, "Agent", input, is_background);
                vec![PipelineAction::None]
            }
            AgentEvent::SubAgentEnd {
                result,
                is_error,
                agent_id: _,
                instance_id,
            } => {
                let tc_id = if let Some(ref iid) = instance_id {
                    // 按 instance_id 精确查找 RUNNING 的 SubAgent
                    self.subagent_stack
                        .iter()
                        .find(|s| s.instance_id == *iid && s.is_running)
                        .map(|s| s.instance_id.clone())
                        .unwrap_or_else(|| "subagent_end".to_string())
                } else {
                    // 防御性回退：instance_id=None 仅当旧版事件到达
                    self.subagent_stack
                        .last()
                        .map(|s| s.instance_id.clone())
                        .unwrap_or_else(|| "subagent_end".to_string())
                };
                self.tool_end_internal(&tc_id, "Agent", &result, is_error);
                vec![PipelineAction::None]
            }
            AgentEvent::Done => {
                if self.in_subagent() {
                    // Child agent done during tool execution — ignore to avoid
                    // finalizing parent state or corrupting the subagent_stack.
                    vec![PipelineAction::None]
                } else {
                    self.done();
                    vec![PipelineAction::None]
                }
            }
            AgentEvent::Interrupted => {
                if self.in_subagent() {
                    // Child agent interrupted — ignore; parent tool call will
                    // handle the result (including interruption) when it returns.
                    vec![PipelineAction::None]
                } else {
                    self.interrupt();
                    vec![PipelineAction::None]
                }
            }
            AgentEvent::StateSnapshot(msgs) => {
                if self.in_subagent() {
                    // 子 Agent 的 StateSnapshot 不应修改父 Agent 的 completed 列表，
                    // 否则子 Agent 的全部内部消息会污染父 Agent 的消息历史。
                    vec![PipelineAction::None]
                } else {
                    self.set_completed(msgs);
                    vec![PipelineAction::None]
                }
            }
            AgentEvent::SubagentLifecycle { .. } => {
                // SubagentLifecycle 仅由 agent_ops 处理（spinner + request_rebuild），
                // Pipeline 不修改状态，直接返回 None
                vec![PipelineAction::None]
            }
            // 以下事件由 agent_ops 直接处理，Pipeline 返回 None
            AgentEvent::Error(_)
            | AgentEvent::InteractionRequest { .. }
            | AgentEvent::TodoUpdate(_)
            | AgentEvent::CompactStarted
            | AgentEvent::CompactCompleted { .. }
            | AgentEvent::CompactError(_)
            | AgentEvent::TokenUsageUpdate { .. }
            | AgentEvent::LlmRetrying { .. }
            | AgentEvent::ContextWarning { .. }
            | AgentEvent::OAuthAuthorizationNeeded { .. }
            | AgentEvent::OAuthAuthorizationCompleted { .. }
            | AgentEvent::OAuthAuthorizationFailed { .. }
            | AgentEvent::BackgroundTaskCompleted { .. }
            | AgentEvent::McpActionCompleted { .. }
            | AgentEvent::PluginActionCompleted { .. }
            | AgentEvent::LspDiagnostics { .. } => {
                vec![PipelineAction::None]
            }
        }
    }

    // ─── 流式事件输入 ─────────────────────────────────────────────────────

    /// 追加流式文本 chunk
    pub fn push_chunk(&mut self, chunk: &str) {
        self.current_ai_text.push_str(chunk);
    }

    /// 追加推理 chunk
    pub fn push_reasoning(&mut self, text: &str) {
        self.current_ai_reasoning.push_str(text);
    }

    /// 工具调用开始（内部版本，只更新状态，不返回 PipelineAction）
    fn tool_start_internal(
        &mut self,
        tool_call_id: &str,
        name: &str,
        input: serde_json::Value,
        is_background: bool,
    ) {
        self.finalize_current_ai();
        self.current_ai_tool_calls
            .push(ToolCallRequest::new(tool_call_id, name, input.clone()));

        if name == "Agent" {
            let agent_id = input["subagent_type"]
                .as_str()
                .unwrap_or("Agent")
                .to_string();
            let task_preview: String = input["prompt"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(40)
                .collect();
            self.subagent_stack.push(SubAgentState {
                agent_id: agent_id.clone(),
                instance_id: tool_call_id.to_string(),
                task_preview: task_preview.clone(),
                total_steps: 0,
                recent_messages: Vec::new(),
                is_running: true,
                finalized_vm: None,
                is_background,
                bg_hash: Some(instance_hash(tool_call_id)),
            });
            // 批次检测：第一个 agent 创建批次，后续递增
            let stack_depth = self.subagent_stack.len() - 1;
            if let Some(ref mut batch) = self.active_batch {
                batch.started += 1;
            } else {
                self.active_batch = Some(BatchInfo {
                    started: 1,
                    completed: 0,
                    stack_depth,
                });
            }
        } else {
            // 非 Agent 工具打断批次连续性
            self.active_batch = None;
        }

        self.pending_tools.insert(
            tool_call_id.to_string(),
            PendingTool {
                tool_call_id: tool_call_id.to_string(),
                name: name.to_string(),
                input,
            },
        );
    }

    /// 工具调用结束（内部版本，只更新状态，不返回 PipelineAction）
    fn tool_end_internal(&mut self, tool_call_id: &str, name: &str, output: &str, is_error: bool) {
        let pending = self.pending_tools.remove(tool_call_id);
        let input = pending
            .as_ref()
            .map(|p| p.input.clone())
            .unwrap_or(serde_json::Value::Null);

        if name == "Agent" {
            // tool_call_id 现在就是 instance_id，直接精确匹配
            if let Some(sub) = self
                .subagent_stack
                .iter_mut()
                .find(|s| s.instance_id == tool_call_id && s.is_running)
            {
                if sub.is_background {
                    // 后台 agent 路径：不冻结，保持 is_running=true，解析 bg_hash
                    sub.bg_hash = parse_bg_hash(output);
                    // 保持 is_running=true，等待 BackgroundTaskCompleted 到达
                    // 显式确保 is_running=true（防止其他逻辑意外修改）
                    sub.is_running = true;
                } else {
                    // 前台 agent 路径：冻结 SubAgentGroup
                    sub.is_running = false;
                    let vm = MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id.clone(),
                        task_preview: sub.task_preview.clone(),
                        total_steps: sub.total_steps,
                        recent_messages: std::mem::take(&mut sub.recent_messages),
                        is_running: false,
                        collapsed: false,
                        final_result: Some(output.to_string()),
                        is_error,
                        is_background: false,
                        bg_hash: sub.bg_hash.clone(),
                        batch_agents: Vec::new(),
                        instance_id: Some(sub.instance_id.clone()),
                    };
                    sub.finalized_vm = Some(vm.clone());
                    // 立即冻结：RebuildAll 可能在下一个 StateSnapshot 前触发
                    self.frozen_subagent_vms.push(vm);
                }
            }
            // 批次检测：递增完成计数
            if let Some(ref mut batch) = self.active_batch {
                batch.completed += 1;
            }
        } else {
            // 非 SubAgent 工具：保存到 completed_tools，在 StateSnapshot 到达前显示
            self.completed_tools.push(CompletedTool {
                tool_call_id: tool_call_id.to_string(),
                name: name.to_string(),
                input,
                output: output.to_string(),
                is_error,
            });
        }
    }

    /// SubAgent 内部工具调用（路由进指定 SubAgentGroup）
    fn push_tool_start_to_subagent(
        sub: &mut SubAgentState,
        tool_call_id: &str,
        name: &str,
        input: &serde_json::Value,
        cwd: &str,
    ) {
        let display = tool_display::format_tool_name(name);
        let args = tool_display::format_tool_args(name, input, Some(cwd));
        let vm = MessageViewModel::tool_block_with_id(
            tool_call_id.to_string(),
            name.to_string(),
            display,
            args,
            false,
        );
        sub.total_steps += 1;
        if sub.recent_messages.len() >= 4 {
            sub.recent_messages.remove(0);
        }
        sub.recent_messages.push(vm);
    }

    /// SubAgent 内部 chunk（路由进指定 SubAgentGroup）
    fn push_chunk_to_subagent(sub: &mut SubAgentState, chunk: &str) {
        match sub.recent_messages.last_mut() {
            Some(m) if m.is_assistant() => m.append_chunk(chunk),
            _ => {
                sub.total_steps += 1;
                if sub.recent_messages.len() >= 4 {
                    sub.recent_messages.remove(0);
                }
                let mut bubble = MessageViewModel::assistant();
                bubble.append_chunk(chunk);
                sub.recent_messages.push(bubble);
            }
        }
    }

    /// SubAgent 内部 ToolEnd 更新（路由进指定 SubAgentGroup）
    fn update_tool_end_in_subagent(
        sub: &mut SubAgentState,
        tool_call_id: &str,
        output: &str,
        is_error: bool,
    ) {
        for vm in sub.recent_messages.iter_mut().rev() {
            if let MessageViewModel::ToolBlock {
                tool_call_id: tc_id,
                content,
                is_error: err,
                ..
            } = vm
            {
                if tc_id == tool_call_id {
                    *content = output.to_string();
                    *err = is_error;
                    break;
                }
            }
        }
    }

    /// 根据 instance_id 查找 subagent_stack 中正在运行的 SubAgent
    fn find_running_subagent_mut(&mut self, instance_id: &str) -> Option<&mut SubAgentState> {
        self.subagent_stack
            .iter_mut()
            .find(|s| s.instance_id == instance_id && s.is_running)
    }

    /// 标记当前 AI 轮次结束
    pub fn done(&mut self) {
        self.finalize_current_ai();
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.throttle_armed = false;
        self.throttle_last_fire = None;
        self.active_batch = None;
        self.drain_subagent_stack();
    }

    /// 中断：finalize 当前状态并清理残留
    pub fn interrupt(&mut self) {
        self.finalize_current_ai();
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.throttle_armed = false;
        self.throttle_last_fire = None;
        self.active_batch = None;
        self.drain_subagent_stack();
    }

    /// 清理 subagent_stack：只推入**未**在 tool_end_internal 中 freeze 的残留条目。
    ///
    /// `tool_end_internal` 在 SubAgentEnd 时已将 finalized_vm 推入 frozen_subagent_vms，
    /// 这里只处理异常情况（SubAgent 未正常结束，如被 Interrupted/Error 打断时仍在运行）。
    /// 已 finalized 的条目不重复推入，避免 frozen 列表膨胀导致 merge_frozen_subagents 错位。
    fn drain_subagent_stack(&mut self) {
        for sub in self.subagent_stack.drain(..) {
            if sub.finalized_vm.is_none() && !sub.is_running {
                // 未 finalized 但已停止：异常残留，构建一个基本 VM 保留显示
                self.frozen_subagent_vms
                    .push(MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id,
                        task_preview: sub.task_preview,
                        total_steps: sub.total_steps,
                        recent_messages: sub.recent_messages,
                        is_running: false,
                        collapsed: false,
                        final_result: None,
                        is_error: false,
                        is_background: sub.is_background,
                        bg_hash: sub.bg_hash,
                        batch_agents: Vec::new(),
                        instance_id: Some(sub.instance_id),
                    });
            } else if sub.finalized_vm.is_none() && sub.is_running && sub.is_background {
                // 后台 agent 仍在运行：冻结以保留当前 recent_messages，
                // 后续 BackgroundTaskCompleted 会直接更新 view_messages
                self.frozen_subagent_vms
                    .push(MessageViewModel::SubAgentGroup {
                        agent_id: sub.agent_id,
                        task_preview: sub.task_preview,
                        total_steps: sub.total_steps,
                        recent_messages: sub.recent_messages,
                        is_running: true,
                        collapsed: false,
                        final_result: None,
                        is_error: false,
                        is_background: true,
                        bg_hash: sub.bg_hash,
                        batch_agents: Vec::new(),
                        instance_id: Some(sub.instance_id),
                    });
            }
            // 已 finalized（finalized_vm.is_some()）的不推入——tool_end_internal 已处理
            // 仍在运行的前台 agent（is_running && !is_background）不推入
        }
    }

    /// 清空所有状态
    pub fn clear(&mut self) {
        self.completed.clear();
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.subagent_stack.clear();
        self.frozen_subagent_vms.clear();
        self.active_batch = None;
    }

    /// 清空并释放所有内部 buffer 的 capacity
    pub fn shrink_to_fit(&mut self) {
        self.completed.shrink_to_fit();
        self.current_ai_text.shrink_to_fit();
        self.current_ai_reasoning.shrink_to_fit();
        self.current_ai_tool_calls.shrink_to_fit();
        self.pending_tools.shrink_to_fit();
        self.completed_tools.shrink_to_fit();
        self.subagent_stack.shrink_to_fit();
        self.frozen_subagent_vms.shrink_to_fit();
    }

    /// 当前 AI 消息是否有可见内容
    pub fn has_streaming_content(&self) -> bool {
        !self.current_ai_text.trim().is_empty() || !self.current_ai_reasoning.is_empty()
    }

    /// 当前 AI 消息是否有待处理的 tool_calls
    pub fn has_pending_tool_calls(&self) -> bool {
        !self.current_ai_tool_calls.is_empty()
    }

    /// 是否在 SubAgent 执行中
    pub fn in_subagent(&self) -> bool {
        // 后台 agent 不会阻塞父 agent 的 Done 事件
        self.subagent_stack
            .last()
            .is_some_and(|s| s.is_running && !s.is_background)
    }

    /// 本轮是否已收到过 StateSnapshot
    pub fn has_snapshot_this_round(&self) -> bool {
        self.has_snapshot_this_round
    }

    /// 诊断用：返回 frozen_subagent_vms 的数量
    pub fn frozen_subagent_vms_count(&self) -> usize {
        self.frozen_subagent_vms.len()
    }

    /// 可变访问 frozen_subagent_vms（供 handle_background_task_completed 同步更新状态）
    pub fn frozen_subagent_vms_mut(&mut self) -> &mut Vec<MessageViewModel> {
        &mut self.frozen_subagent_vms
    }

    // ── 轮次管理 ──────────────────────────────────────────────────────────────

    /// 标记新一轮对话开始。由 submit_message() 调用。
    pub fn begin_round(&mut self) {
        self.completed_len_at_round_start = self.completed.len();
        self.has_snapshot_this_round = false;
        self.throttle_armed = false;
        self.throttle_last_fire = None;
        // 清空上一轮的 frozen_subagent_vms，防止跨轮次累积导致新轮次的
        // SubAgentGroup 按位置错误匹配到旧轮的 frozen VM（而非本轮的）。
        self.frozen_subagent_vms.clear();
    }

    // ── 节流机制 ──────────────────────────────────────────────────────────────

    /// 检查节流计时器，若 100ms 已过则发射 RebuildAll。
    /// 由 poll_agent() 每帧调用。
    pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        if !self.throttle_armed {
            return None;
        }
        let now = Instant::now();
        let should_fire = match self.throttle_last_fire {
            None => true,
            Some(last) => now.duration_since(last) >= Duration::from_millis(100),
        };
        if should_fire {
            self.throttle_last_fire = Some(now);
            self.throttle_armed = false;
            return Some(self.build_rebuild_all(prefix_len));
        }
        None
    }

    /// 获取已完成的 BaseMessages（用于持久化）
    pub fn completed_messages(&self) -> &[BaseMessage] {
        &self.completed
    }

    /// 从 pipeline 规范状态构建尾部 VMs。
    ///
    pub fn set_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.completed.extend(msgs);
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = true;
        self.has_snapshot_this_round = true;
        self.pending_tools.clear();
        self.completed_tools.clear();
    }

    /// 从外部加载全量 BaseMessages（用于历史恢复后覆盖），并清除所有状态
    pub fn restore_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.completed = msgs;
        self.completed_len_at_round_start = self.completed.len();
        self.has_snapshot_this_round = false;
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = true;
    }
}

#[cfg(test)]
#[path = "message_pipeline_test.rs"]
mod tests;

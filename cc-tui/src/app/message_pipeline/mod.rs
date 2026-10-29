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
    cell::RefCell,
    collections::HashMap,
    time::{Duration, Instant},
};

use cc_agent::agent::events::{HookFailureInfo, HookFailureKind};
use cc_agent::messages::{BaseMessage, ToolCallRequest};

use crate::app::{events::AgentEvent, tool_display};
use crate::ui::message_view::{instance_hash, parse_bg_hash, HookRow};
use crate::ui::message_view::{MessageViewModel, ToolOutputPreview, ToolPhase};

mod reconcile;
mod shell_runtime;
mod streaming_output;
pub(crate) mod transform;

pub use crate::ui::message_view::aggregate_batch_groups;
#[cfg(test)]
use reconcile::merge_frozen_subagents;
pub use reconcile::PipelineAction;

// ─── 流式渲染模式 ──────────────────────────────────────────────────────────

/// 流式渲染模式：控制 LLM 输出时的渲染粒度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum StreamingMode {
    /// 逐 token 实时渲染 + 自适应帧率（默认）
    #[default]
    Streaming,
    /// 按 Markdown block 粒度整块渲染（段落/代码块完成后渲染）
    Block,
    /// 不渲染流式内容，LLM 完成后一次性显示
    None,
}

// ─── 自适应分块策略 ──────────────────────────────────────────────────────────

/// 排空计划：控制每次 check_throttle 的消费量
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrainPlan {
    /// 正常模式：提交一行（单次 RebuildAll）
    Single,
    /// 积压模式：一次性排空所有积压行（单次 RebuildAll 含全部内容）
    Batch,
}

/// 分块模式（内部状态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChunkingMode {
    /// 平滑模式：逐行提交
    Smooth,
    /// 追赶模式：批量排空
    CatchUp,
}

/// 自适应分块策略：根据队列压力在 Smooth/CatchUp 模式间动态切换。
///
/// Smooth 模式（默认）：每次 tick 提交一行，保证流畅感。
/// CatchUp 模式：队列积压时一次性排空，快速收敛显示。
///
/// 进入 CatchUp 条件（满足任一）：
/// - 队列深度 >= `queue_depth_threshold`（默认 8 行）
/// - 最老行年龄 >= `oldest_age_threshold`（默认 120ms）
///
/// 退出 CatchUp 条件（同时满足）：
/// - 队列深度 <= `exit_depth`（默认 2 行）
/// - 最老行年龄 <= `exit_age`（默认 40ms）
pub(crate) struct AdaptiveChunkingPolicy {
    /// 当前是否处于 CatchUp 模式
    pub(crate) mode: ChunkingMode,
    /// 累积的未消费行数（按换行符计）
    pub(crate) pending_lines: usize,
    /// 首个未消费 chunk 的到达时间（用于计算最老行年龄）
    pub(crate) oldest_chunk_at: Option<Instant>,
    /// 进入 CatchUp 的队列深度阈值
    queue_depth_threshold: usize,
    /// 进入 CatchUp 的最老行年龄阈值
    oldest_age_threshold: Duration,
    /// 退出 CatchUp 的队列深度阈值
    exit_depth: usize,
    /// 退出 CatchUp 的最老行年龄阈值
    exit_age: Duration,
}

impl AdaptiveChunkingPolicy {
    /// 使用默认参数创建策略
    fn new() -> Self {
        Self {
            mode: ChunkingMode::Smooth,
            pending_lines: 0,
            oldest_chunk_at: None,
            queue_depth_threshold: 8,
            oldest_age_threshold: Duration::from_millis(120),
            exit_depth: 2,
            exit_age: Duration::from_millis(40),
        }
    }

    /// 通知策略有新的 chunk 到达。
    /// 按换行符统计行数，并记录首个 chunk 的时间戳。
    fn on_chunk(&mut self, chunk: &str) {
        let new_lines = chunk.lines().count().max(1);
        self.pending_lines += new_lines;
        if self.oldest_chunk_at.is_none() {
            self.oldest_chunk_at = Some(Instant::now());
        }
    }

    /// 通知策略有新的推理 chunk 到达（同样累积压力）
    fn on_reasoning_chunk(&mut self) {
        self.pending_lines += 1;
        if self.oldest_chunk_at.is_none() {
            self.oldest_chunk_at = Some(Instant::now());
        }
    }

    /// 检查当前是否应该触发重绘，若触发则返回 DrainPlan。
    ///
    /// 策略逻辑：
    /// - Smooth 模式：检查基础节流间隔（最小 16ms，约 60fps），满足则返回 Single
    /// - CatchUp 模式：立即返回 Batch，无节流间隔限制
    /// - 每次调用检查是否需要模式切换
    fn check(&mut self) -> Option<DrainPlan> {
        if self.pending_lines == 0 {
            return None;
        }

        self.update_mode();

        match self.mode {
            ChunkingMode::Smooth => Some(DrainPlan::Single),
            ChunkingMode::CatchUp => Some(DrainPlan::Batch),
        }
    }

    /// 消费后排空积压计数
    fn drain(&mut self) {
        self.pending_lines = 0;
        self.oldest_chunk_at = None;
    }

    /// 重置策略状态（用于 done/interrupt/begin_round）
    fn reset(&mut self) {
        self.mode = ChunkingMode::Smooth;
        self.pending_lines = 0;
        self.oldest_chunk_at = None;
    }

    /// 根据队列深度和最老行年龄更新模式
    fn update_mode(&mut self) {
        let now = Instant::now();
        let oldest_age = self
            .oldest_chunk_at
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::ZERO);

        match self.mode {
            ChunkingMode::Smooth => {
                // 进入 CatchUp：满足任一条件
                if self.pending_lines >= self.queue_depth_threshold
                    || oldest_age >= self.oldest_age_threshold
                {
                    self.mode = ChunkingMode::CatchUp;
                }
            }
            ChunkingMode::CatchUp => {
                // 退出 CatchUp：同时满足两个条件
                if self.pending_lines <= self.exit_depth && oldest_age <= self.exit_age {
                    self.mode = ChunkingMode::Smooth;
                }
            }
        }
    }

    /// 当前是否处于 CatchUp 模式（诊断用）
    #[allow(dead_code)]
    fn is_catch_up(&self) -> bool {
        self.mode == ChunkingMode::CatchUp
    }
}

// ─── 管线内部状态 ────────────────────────────────────────────────────────────

/// 已开始但未结束的工具调用
pub(crate) struct PendingTool {
    #[allow(dead_code)] // 用于工具调用匹配，reconcile 阶段读取
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
    phase: ToolPhase,
    live_output: Option<ToolOutputPreview>,
    /// ToolStart 的单调时钟起点，审批等待不计入执行耗时。
    started_at: Option<Instant>,
}

/// ToolEnd 后、StateSnapshot 前的工具结果（用于在 reconcile gap 期间显示）
pub(crate) struct CompletedTool {
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
    output: String,
    is_error: bool,
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
}

// ─── hook 执行结果合并 ───────────────────────────────────────────────────────

/// 异步 hook 的执行阶段。
///
/// 异步 hook 先上报开始（写 `Async hook X running…` 占位），完成时移除占位、
/// 结果行写进会话级尾行；同步 hook 只在完成时上报，直接是 `Finished`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookPhase {
    /// 异步 hook 已开始、尚未回报结果
    Running,
    /// 已有最终结果（成功汇总 / 阻断 / 失败）
    Finished,
}

/// 一次工具级 hook 执行结果（挂到工具块上的原始结构）。
///
/// 同一个工具调用、同一个事件可能由多个 `HookMiddleware` 实例分别上报
/// （plugin / global / project / local 各配各的 hook，每个实例独立跑一次
/// `run_hooks`）。归档时按 `(tool_call_id, event)` 合并，
/// 让 PRD C4 的「2 PreToolUse hooks ran」只占一行。
///
/// 异步完成不进这里：异步 hook 可能在工具块冻结进历史前缀之后才回来，
/// 结果行统一走会话级尾行（见 [`MessagePipeline::record_async_hook`]）。
#[derive(Debug, Clone)]
pub(crate) struct HookExec {
    pub event: String,
    pub ran: usize,
    pub duration_ms: u64,
    pub failures: Vec<HookFailureInfo>,
    pub phase: HookPhase,
}

impl HookExec {
    pub(crate) fn new(
        event: impl Into<String>,
        ran: usize,
        duration_ms: u64,
        failures: Vec<HookFailureInfo>,
    ) -> Self {
        Self {
            event: event.into(),
            ran,
            duration_ms,
            failures,
            phase: HookPhase::Finished,
        }
    }

    /// 异步 hook 开始占位（`Async hook {event} running…`）。
    ///
    /// 只表达「还在跑」：完成结果由会话级尾行承载，占位在完成事件到达时移除
    /// （见 [`MessagePipeline::clear_running_hook`]）。
    pub(crate) fn running(event: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            ran: 0,
            duration_ms: 0,
            failures: Vec::new(),
            phase: HookPhase::Running,
        }
    }

    /// 把一次执行结果转成渲染行：阻断/失败行在前，汇总行在后（PRD 快照 C5/C6）。
    ///
    /// 汇总行与 hook 自身失败行都是 `detail_only`，普通模式不渲染；
    /// 只有阻断行（工具真的没跑）进普通模式。
    fn to_rows(&self) -> Vec<HookRow> {
        if self.phase == HookPhase::Running {
            return vec![HookRow::async_row_running(format!(
                "Async hook {} running…",
                self.event
            ))];
        }
        let mut rows: Vec<HookRow> = self
            .failures
            .iter()
            .map(|failure| match failure.kind {
                HookFailureKind::Block => {
                    HookRow::blocked(format!("{} hook blocked: {}", self.event, failure.reason))
                }
                HookFailureKind::Error => {
                    HookRow::failed(format!("{} hook failed: {}", self.event, failure.reason))
                }
            })
            .collect();
        // ran 为 0 说明本次没有 hook 真正开跑（全被 once / matcher 过滤），
        // 不该渲染 `0 X hooks ran` 这种空汇总。
        if self.ran > 0 {
            rows.push(HookRow::summary(sync_summary_text(
                &self.event,
                self.ran,
                self.duration_ms,
            )));
        }
        rows
    }
}

// ─── 会话级 hook 行 ─────────────────────────────────────────────────────────

/// 跨事件异步完成的合并窗口。
///
/// 参考实现的形态是 `Async hook A completed, Async hook B completed`：同一时刻
/// 触发的多个异步 hook 各跑各的，完成时刻前后脚到达，逐条成行会把界面刷成流水账。
/// 只有不同事件之间才判窗口，同事件的多条完成无条件并进同一行。
const HOOK_LINE_MERGE_WINDOW: Duration = Duration::from_secs(1);

/// 同步汇总文案：`1 PostToolUse hook ran · 3ms` / `2 PostToolUse hooks ran · 8ms`。
pub(crate) fn sync_summary_text(event: &str, ran: usize, duration_ms: u64) -> String {
    let noun = if ran == 1 { "hook" } else { "hooks" };
    format!("{ran} {event} {noun} ran · {duration_ms}ms")
}

/// 异步完成段文案。
///
/// 独占一行时带计数（`1 async Stop hook completed` / `2 async Stop hooks completed`）；
/// 与其它事件并进同一行时用单数形式 `Async hook Stop completed`（与参考实现一致）。
pub(crate) fn async_summary_text(event: &str, count: usize, merged: bool) -> String {
    if merged && count == 1 {
        return format!("Async hook {event} completed");
    }
    let noun = if count == 1 { "hook" } else { "hooks" };
    format!("{count} async {event} {noun} completed")
}

/// 一条会话级 hook 行里的一个事件段（同一事件的多次上报合并成一段）。
#[derive(Debug, Clone)]
struct HookNoteSegment {
    event: String,
    /// 异步段的完成次数；同步段恒为 1，不参与文案。
    count: usize,
    /// 同步段汇总的 hook 数与耗时；异步段不用。
    ran: usize,
    duration_ms: u64,
    is_async: bool,
}

impl HookNoteSegment {
    fn sync(event: &str, ran: usize, duration_ms: u64) -> Self {
        Self {
            event: event.to_string(),
            count: 1,
            ran,
            duration_ms,
            is_async: false,
        }
    }

    fn async_once(event: &str) -> Self {
        Self {
            event: event.to_string(),
            count: 1,
            ran: 0,
            duration_ms: 0,
            is_async: true,
        }
    }

    fn text(&self, merged: bool) -> String {
        if self.is_async {
            async_summary_text(&self.event, self.count, merged)
        } else {
            sync_summary_text(&self.event, self.ran, self.duration_ms)
        }
    }
}

/// 一条会话级 hook 行（`⎿` 前缀、不挂工具块、可就地改写）。
///
/// 同一事件在一轮内多次上报要合并进同一行，因此这里保存「上一次写进视图的文案」，
/// 改写时按它定位旧行（见 `App::apply_hook_note`）。
#[derive(Debug, Clone)]
struct HookNoteLine {
    segments: Vec<HookNoteSegment>,
    /// 上一次写进视图的文案
    text: String,
    /// 最近一次写入/改写时刻：跨事件异步完成的合并窗口依据
    updated_at: Instant,
}

impl HookNoteLine {
    fn new(segment: HookNoteSegment, now: Instant) -> Self {
        let mut line = Self {
            segments: vec![segment],
            text: String::new(),
            updated_at: now,
        };
        line.text = line.render();
        line
    }

    /// 当前段组合应显示的文案。
    fn render(&self) -> String {
        // 行内有多个异步段说明发生了跨事件合并：各段改用单数形式。
        let merged = self.segments.iter().filter(|seg| seg.is_async).count() > 1;
        self.segments
            .iter()
            .map(|seg| seg.text(merged))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn has_async_segment(&self) -> bool {
        self.segments.iter().any(|seg| seg.is_async)
    }

    fn has_async_segment_of(&self, event: &str) -> bool {
        self.segments
            .iter()
            .any(|seg| seg.is_async && seg.event == event)
    }

    fn has_sync_segment(&self, event: &str) -> bool {
        self.segments
            .iter()
            .any(|seg| !seg.is_async && seg.event == event)
    }

    fn sync_segment_mut(&mut self, event: &str) -> Option<&mut HookNoteSegment> {
        self.segments
            .iter_mut()
            .find(|seg| !seg.is_async && seg.event == event)
    }

    fn async_segment_mut(&mut self, event: &str) -> Option<&mut HookNoteSegment> {
        self.segments
            .iter_mut()
            .find(|seg| seg.is_async && seg.event == event)
    }

    /// 就地改写：返回旧文案供调用方替换，并刷新合并窗口起点。
    fn rewrite(&mut self, now: Instant) -> HookNoteUpdate {
        let text = self.render();
        let previous = std::mem::replace(&mut self.text, text.clone());
        self.updated_at = now;
        HookNoteUpdate {
            previous: Some(previous),
            text,
        }
    }
}

/// [`MessagePipeline::record_sync_hook`] / [`MessagePipeline::record_async_hook`] 的结果。
pub(crate) struct HookNoteUpdate {
    /// 上一次写进视图的文案；`Some` 表示应就地改写而非新增。
    pub previous: Option<String>,
    /// 本次应显示的文案
    pub text: String,
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
    completed_revision: u64,
    completed_view_cache: RefCell<Option<reconcile::CompletedViewCache>>,
    /// 当前正在流式构建的 AI 文本
    current_ai_text: String,
    /// 当前正在流式构建的 AI 推理内容
    current_ai_reasoning: String,
    /// 当前这段思考的起始时刻：运行中用它把 Thinking 行的时间"走秒"
    reasoning_started_at: Option<std::time::Instant>,
    /// 当前"思考+工具"组内累计的间隔耗时（对齐参考实现 thoughtForMs，单段封顶 600s）
    group_ms: u64,
    /// 上一个事件（思考首 chunk / ToolStart / ToolEnd / 消息落库）的到达时刻
    last_event_at: Option<std::time::Instant>,
    /// 已冻结的组耗时，按时间顺序排列（每组在正文到达时冻结）
    finished_groups: Vec<u64>,
    /// 当前组是否仍在进行（正文到达即结束）——决定整行显示 Thinking 还是 Thought
    group_active: bool,
    /// 当前是否处于"思考中"：push_reasoning 置 true，正文到达（freeze_group）置 false
    thinking_now: bool,
    /// 这段思考是否已经落进 completed（快照已收纳）。
    /// 只有落库后才停止从流式气泡重复输出；仅 finalize（ToolStart）不足以判定。
    reasoning_flushed: bool,
    /// 当前 AI 消息中的 tool_calls（由 ToolStart 事件积累）
    current_ai_tool_calls: Vec<ToolCallRequest>,
    /// 当前 AI 消息是否已 finalize（ToolStart 到达后 finalize）
    current_ai_finalized: bool,
    /// 已开始但未结束的工具调用
    pending_tools: HashMap<String, PendingTool>,
    /// 按 `tool_call_id` 归档的 hook 执行结果（仅 UI 展示，不进 history）。
    ///
    /// PreToolUse 的汇总早于 ToolStart 到达，此时工具块还不存在，所以统一按 id
    /// 暂存在管线里，由 [`Self::build_tail_vms`] 生成 ToolBlock 时挂上——这样无论
    /// 事件先后、无论尾部被重建多少次，行都不会丢。
    ///
    /// 存原始结构而非渲染行：同事件的多次上报（global+project 各一个 hook）要在
    /// 这里合并成一条，才不会被渲染成多行。
    hook_results: HashMap<String, Vec<HookExec>>,
    /// 本轮会话级 hook 行（不挂工具块），按创建顺序排列，末条是「当前行」。
    ///
    /// 行本身是 `ephemeral_notes` 里的 SystemNote（由 agent_ops 写入视图），这里只
    /// 记录合并后的段与上次文案，供同事件下一次上报时就地改写。轮次结束
    /// （[`Self::done`] / [`Self::interrupt`]）清空，避免跨轮错并。
    hook_notes: Vec<HookNoteLine>,
    /// 真实 spawn 注册可能先于 ToolStart；按调用身份保存，禁止命令文本匹配。
    shell_runtimes: HashMap<(Option<String>, String), shell_runtime::ShellRuntime>,
    /// ToolEnd 后、StateSnapshot 前的工具结果（在 reconcile gap 期间显示）
    completed_tools: Vec<CompletedTool>,
    /// SubAgent 栈
    subagent_stack: Vec<SubAgentState>,
    /// 冻结的 SubAgentGroup VMs（SubAgentEnd 时构建，done() 时收集）
    frozen_subagent_vms: Vec<MessageViewModel>,
    /// 批次检测状态（连续的 SubAgentStart/SubAgentEnd 跟踪）
    active_batch: Option<BatchInfo>,
    // ── 节流状态 ──
    /// 自适应分块策略（替代固定 100ms 节流）
    adaptive_policy: AdaptiveChunkingPolicy,
    /// 上次节流发射的时间（Smooth 模式下的最小间隔守卫）
    throttle_last_fire: Option<Instant>,
    // ── 流式渲染模式 ──
    /// 当前流式渲染模式
    streaming_mode: StreamingMode,
    // ── Block 模式缓冲 ──
    /// Block 模式下累积未完成 block 的 chunk
    block_buffer: String,
    /// Block 模式的块边界扫描器（持有围栏状态与已扫描位置，只扫新增行）
    block_scanner: crate::ui::markdown::BlockBoundaryScanner,
    /// Block 模式下是否有待 flush 的内容
    block_pending_flush: bool,
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
            completed_revision: 0,
            completed_view_cache: RefCell::new(None),
            current_ai_text: String::new(),
            current_ai_reasoning: String::new(),
            reasoning_started_at: None,
            group_ms: 0,
            last_event_at: None,
            finished_groups: Vec::new(),
            group_active: false,
            thinking_now: false,
            reasoning_flushed: false,
            current_ai_tool_calls: Vec::new(),
            current_ai_finalized: false,
            pending_tools: HashMap::new(),
            hook_results: HashMap::new(),
            hook_notes: Vec::new(),
            shell_runtimes: HashMap::new(),
            completed_tools: Vec::new(),
            subagent_stack: Vec::new(),
            frozen_subagent_vms: Vec::new(),
            active_batch: None,
            adaptive_policy: AdaptiveChunkingPolicy::new(),
            throttle_last_fire: None,
            streaming_mode: StreamingMode::default(),
            block_buffer: String::new(),
            block_scanner: crate::ui::markdown::BlockBoundaryScanner::new(),
            block_pending_flush: false,
            completed_len_at_round_start: 0,
            has_snapshot_this_round: false,
        }
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// 归档一次工具级同步 hook 执行结果（挂在触发它的工具块上）。
    ///
    /// 合并键 `(tool_call_id, event)`：`tool_call_id` 由 map 键承担；同事件按名字
    /// 合并（`ran` / `duration_ms` 相加、`failures` 拼接）。
    pub(crate) fn push_hook_result(&mut self, tool_call_id: &str, exec: HookExec) {
        // Edit / Write 不展示 hook 行：这两类操作已经由用户显式授权，
        // 再叠一行 hook 执行结果只是噪音。其它工具（Bash / Read / Grep …）照常展示。
        let tool_name = self
            .pending_tools
            .get(tool_call_id)
            .map(|tool| tool.name.clone())
            .or_else(|| {
                self.completed_tools
                    .iter()
                    .find(|tool| tool.tool_call_id == tool_call_id)
                    .map(|tool| tool.name.clone())
            });
        // 只读工具（Read / Grep / Glob）不展示 hook 行：这类调用对用户没有决策价值，
        // 叠一行只是噪音。Edit / Write / Bash / Agent 等仍照常展示。
        if matches!(tool_name.as_deref(), Some("Read" | "Grep" | "Glob")) {
            return;
        }
        let execs = self
            .hook_results
            .entry(tool_call_id.to_string())
            .or_default();
        if let Some(existing) = execs
            .iter_mut()
            .find(|existing| existing.event == exec.event)
        {
            existing.ran += exec.ran;
            existing.duration_ms += exec.duration_ms;
            existing.failures.extend(exec.failures);
        } else {
            execs.push(exec);
        }
    }

    /// 异步 hook 开始：给该工具调用写一条 Running 占位行。
    ///
    /// 占位由 `Async hook {event} running…` 渲染，完成事件到达时移除
    /// （见 [`Self::clear_running_hook`]）。轮次结束时由 [`Self::drop_running_hooks`] 清理。
    pub(crate) fn start_hook_result(&mut self, tool_call_id: &str, event: impl Into<String>) {
        self.hook_results
            .entry(tool_call_id.to_string())
            .or_default()
            .push(HookExec::running(event));
    }

    /// 异步 hook 完成：移除工具块上的 `running…` 占位。
    ///
    /// 完成结果不进工具块——异步 hook 可能在工具块冻结进历史前缀之后才回来，
    /// 结果行统一由 [`Self::record_async_hook`] 写在会话级尾行。
    pub(crate) fn clear_running_hook(&mut self, tool_call_id: &str, event: &str) {
        if let Some(execs) = self.hook_results.get_mut(tool_call_id) {
            execs.retain(|exec| !(exec.phase == HookPhase::Running && exec.event == event));
        }
    }

    /// 记录一次会话级同步 hook 汇总。
    ///
    /// 同一事件在一轮内的多次上报（plugin / global / project 各配各的 hook）合并成
    /// 一行，计数与耗时累加；不同事件各占一行。返回 [`HookNoteUpdate`]，`previous`
    /// 为 `Some` 时调用方就地改写该行。
    pub(crate) fn record_sync_hook(
        &mut self,
        event: &str,
        ran: usize,
        duration_ms: u64,
        now: Instant,
    ) -> HookNoteUpdate {
        if let Some(index) = self
            .hook_notes
            .iter()
            .rposition(|line| line.has_sync_segment(event))
        {
            let line = &mut self.hook_notes[index];
            if let Some(segment) = line.sync_segment_mut(event) {
                segment.ran += ran;
                segment.duration_ms += duration_ms;
            }
            return line.rewrite(now);
        }
        let line = HookNoteLine::new(HookNoteSegment::sync(event, ran, duration_ms), now);
        let update = HookNoteUpdate {
            previous: None,
            text: line.text.clone(),
        };
        self.hook_notes.push(line);
        update
    }

    /// 记录一次异步 hook 完成（会话级与工具级共用，都写在视图尾部）。
    ///
    /// 合并规则：
    /// - 同一事件本轮已有异步行 → 就地累加计数（`2 async Stop hooks completed`）；
    /// - 否则若末行是异步行且距上次写入不超过 [`HOOK_LINE_MERGE_WINDOW`] →
    ///   并进同一行、用 `, ` 分隔（`Async hook A completed, Async hook B completed`）；
    /// - 否则新开一行。
    pub(crate) fn record_async_hook(&mut self, event: &str, now: Instant) -> HookNoteUpdate {
        if let Some(index) = self
            .hook_notes
            .iter()
            .rposition(|line| line.has_async_segment_of(event))
        {
            let line = &mut self.hook_notes[index];
            if let Some(segment) = line.async_segment_mut(event) {
                segment.count += 1;
            }
            return line.rewrite(now);
        }
        if let Some(line) = self.hook_notes.last_mut() {
            let within_window =
                now.saturating_duration_since(line.updated_at) <= HOOK_LINE_MERGE_WINDOW;
            if line.has_async_segment() && within_window {
                line.segments.push(HookNoteSegment::async_once(event));
                return line.rewrite(now);
            }
        }
        let line = HookNoteLine::new(HookNoteSegment::async_once(event), now);
        let update = HookNoteUpdate {
            previous: None,
            text: line.text.clone(),
        };
        self.hook_notes.push(line);
        update
    }

    /// 轮次结束清理：丢掉所有仍处于 Running 的异步 hook 占位。
    ///
    /// 异步 hook 不会回报超时，占位若留到下一轮就会永远显示 `running…`。
    /// 已 Finished 的结果行不动——它们要随工具块一起展示。
    fn drop_running_hooks(&mut self) {
        self.hook_results.retain(|_, execs| {
            execs.retain(|exec| exec.phase != HookPhase::Running);
            !execs.is_empty()
        });
    }

    /// 把归档的 hook 结果挂到对应 `ToolBlock`（含 SubAgent 内部的工具块）。
    ///
    /// 找不到匹配工具块的 id 保持暂存：PreToolUse 的行先于 ToolStart 到达，
    /// 等工具块建出来再挂。
    fn apply_hook_rows(&self, vms: &mut [MessageViewModel]) {
        if self.hook_results.is_empty() {
            return;
        }
        for vm in vms.iter_mut() {
            if let MessageViewModel::SubAgentGroup {
                recent_messages, ..
            } = vm
            {
                self.apply_hook_rows(recent_messages);
            }
            let rows = if let MessageViewModel::ToolBlock { tool_call_id, .. } = &*vm {
                self.hook_results.get(tool_call_id).map(|execs| {
                    execs
                        .iter()
                        .flat_map(|exec| exec.to_rows())
                        .collect::<Vec<_>>()
                })
            } else {
                None
            };
            if let Some(rows) = rows.filter(|rows| !rows.is_empty()) {
                vm.set_hook_rows(rows);
            }
        }
    }

    /// 获取当前流式渲染模式
    pub(crate) fn streaming_mode(&self) -> StreamingMode {
        self.streaming_mode
    }

    /// 设置流式渲染模式。切换时强制 flush Block 缓冲区。
    pub(crate) fn set_streaming_mode(&mut self, mode: StreamingMode) {
        if self.streaming_mode == StreamingMode::Block && mode != StreamingMode::Block {
            self.flush_block_buffer();
        }
        self.streaming_mode = mode;
        self.block_scanner.reset();
        tracing::info!(?mode, "streaming mode changed");
    }

    /// 从配置字符串设置初始模式（"streaming" / "block" / "none"）
    pub fn init_streaming_mode_from_config(&mut self, mode_str: &str) {
        let mode = match mode_str {
            "block" => StreamingMode::Block,
            "none" => StreamingMode::None,
            _ => StreamingMode::Streaming,
        };
        self.streaming_mode = mode;
    }

    /// 检查 Block 模式是否有待 flush 的内容
    pub(crate) fn has_pending_block_flush(&self) -> bool {
        self.block_pending_flush || !self.block_buffer.is_empty()
    }

    /// 统一事件处理入口：将 AgentEvent 转换为 PipelineAction 列表。
    /// 所有事件只更新 pipeline 内部状态，返回 None。
    /// RebuildAll 由 agent_ops 通过 `check_throttle()` 或 `build_rebuild_all()` 显式触发。
    pub fn handle_event(&mut self, event: AgentEvent) -> Vec<PipelineAction> {
        match event {
            // 后台 shell 事件不影响消息 pipeline（完成/通知由 agent_ops 单独处理），忽略
            AgentEvent::BackgroundShellCompleted { .. }
            | AgentEvent::BackgroundShellStalled { .. } => vec![PipelineAction::None],
            // hook 事件由 agent_ops 直接写入管线（start 占位 / complete 替换），pipeline 不参与
            AgentEvent::HooksStarted { .. } | AgentEvent::HooksCompleted { .. } => {
                vec![PipelineAction::None]
            }
            AgentEvent::AssistantChunk {
                chunk,
                source_agent_id,
            } => {
                if !chunk.is_empty() {
                    if let Some(ref aid) = source_agent_id {
                        if let Some(sub) = self.find_running_subagent_mut(aid) {
                            Self::push_chunk_to_subagent(sub, &chunk);
                            self.adaptive_policy.on_chunk(&chunk);
                        }
                    } else if self.in_subagent() {
                        // 顺序执行时 last() 就是当前 subagent（事件顺序到达）
                        if let Some(sub) = self.subagent_stack.last_mut() {
                            Self::push_chunk_to_subagent(sub, &chunk);
                            self.adaptive_policy.on_chunk(&chunk);
                        }
                    } else {
                        self.push_chunk(&chunk);
                        // push_chunk 内部已调用 adaptive_policy.on_chunk()
                    }
                }
                vec![PipelineAction::None]
            }
            AgentEvent::AiReasoning(text) => {
                if self.in_subagent() {
                    // SubAgent 内部推理：更新 subagent 状态，通知策略
                    if let Some(_sub) = self.subagent_stack.last_mut() {
                        self.adaptive_policy.on_reasoning_chunk();
                    }
                } else {
                    self.push_reasoning(&text);
                    // push_reasoning 内部已调用 adaptive_policy.on_reasoning_chunk()
                }
                vec![PipelineAction::None]
            }
            AgentEvent::ToolPending {
                tool_call_id,
                name,
                input,
                source_agent_id,
            } => {
                self.adaptive_policy.drain();
                self.force_flush_block();
                if let Some(ref aid) = source_agent_id {
                    let cwd = self.cwd.clone();
                    if let Some(sub) = self.find_running_subagent_mut(aid) {
                        Self::push_tool_to_subagent(
                            sub,
                            &tool_call_id,
                            &name,
                            &input,
                            &cwd,
                            ToolPhase::Queued,
                        );
                    }
                } else if !self.pending_tools.contains_key(&tool_call_id)
                    && !self
                        .completed_tools
                        .iter()
                        .any(|tool| tool.tool_call_id == tool_call_id)
                {
                    self.finalize_current_ai();
                    self.register_tool_call(&tool_call_id, &name, &input);
                    self.pending_tools.insert(
                        tool_call_id.clone(),
                        PendingTool {
                            tool_call_id,
                            name,
                            input,
                            phase: ToolPhase::Queued,
                            live_output: None,
                            started_at: None,
                        },
                    );
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
                self.adaptive_policy.drain();
                self.force_flush_block();

                if source_agent_id.is_none()
                    && (self
                        .pending_tools
                        .get(&tool_call_id)
                        .is_some_and(|tool| tool.phase == ToolPhase::Running)
                        || self
                            .completed_tools
                            .iter()
                            .any(|tool| tool.tool_call_id == tool_call_id))
                {
                    return vec![PipelineAction::None];
                }
                if let Some(ref aid) = source_agent_id {
                    let cwd = self.cwd.clone();
                    if let Some(sub) = self.find_running_subagent_mut(aid) {
                        Self::push_tool_to_subagent(
                            sub,
                            &tool_call_id,
                            &name,
                            &input,
                            &cwd,
                            ToolPhase::Running,
                        );
                    }
                } else if name == "Agent" {
                    // 父 Agent 调用 Agent 工具：只注册 tool_call 和 pending_tool，
                    // 不创建 SubAgentState（SubAgentStart 事件会处理）。
                    // 避免与 SubAgentStart 的 tool_start_internal 产生重复条目。
                    self.finalize_current_ai();
                    self.register_tool_call(&tool_call_id, &name, &input);
                    self.pending_tools.insert(
                        tool_call_id.to_string(),
                        PendingTool {
                            tool_call_id: tool_call_id.to_string(),
                            name: name.to_string(),
                            input,
                            phase: ToolPhase::Running,
                            live_output: None,
                            started_at: Some(Instant::now()),
                        },
                    );
                } else {
                    self.tool_start_internal(&tool_call_id, &name, input, false);
                }

                vec![PipelineAction::None]
            }
            AgentEvent::ToolOutput {
                tool_call_id,
                tail,
                total_lines,
                total_bytes,
                elapsed_ms,
                source_agent_id,
            } => {
                self.update_tool_output(
                    &tool_call_id,
                    ToolOutputPreview::new(tail, total_lines, total_bytes, elapsed_ms),
                    source_agent_id.as_deref(),
                );
                vec![PipelineAction::None]
            }
            AgentEvent::ToolEnd {
                tool_call_id,
                name,
                output,
                is_error,
                source_agent_id,
            } => {
                self.adaptive_policy.drain();
                self.force_flush_block();
                if let Some(ref aid) = source_agent_id {
                    if let Some(sub) = self.find_running_subagent_mut(aid) {
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
                prompt,
            } => {
                let actual_prompt = prompt.as_deref().unwrap_or(&task_preview);
                let input =
                    serde_json::json!({"subagent_type": &agent_id, "prompt": actual_prompt});
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
                    self.force_flush_block();
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
            AgentEvent::ShellCommandCompleted(_)
            | AgentEvent::Error(_)
            | AgentEvent::InteractionRequest { .. }
            | AgentEvent::TodoUpdate(_)
            | AgentEvent::CompactStarted { .. }
            | AgentEvent::CompactCompleted { .. }
            | AgentEvent::CompactError(_)
            | AgentEvent::RewindCompleted { .. }
            | AgentEvent::RecapCompleted { .. }
            | AgentEvent::RecapError(_)
            | AgentEvent::TokenUsageUpdate { .. }
            | AgentEvent::LlmRetrying { .. }
            | AgentEvent::ContextWarning { .. }
            | AgentEvent::SystemNotice { .. }
            | AgentEvent::OAuthAuthorizationNeeded { .. }
            | AgentEvent::OAuthAuthorizationCompleted { .. }
            | AgentEvent::OAuthAuthorizationFailed { .. }
            | AgentEvent::BackgroundTaskCompleted { .. }
            | AgentEvent::McpActionCompleted { .. }
            | AgentEvent::PluginActionCompleted { .. }
            | AgentEvent::LspDiagnostics { .. }
            | AgentEvent::ThreadTitleGenerated { .. }
            | AgentEvent::SubagentProgress { .. }
            | AgentEvent::SubagentEvent { .. }
            | AgentEvent::SubagentInputResult { .. } => {
                vec![PipelineAction::None]
            }
        }
    }

    // ─── 流式事件输入 ─────────────────────────────────────────────────────

    /// 追加流式文本 chunk
    pub fn push_chunk(&mut self, chunk: &str) {
        // 正文开始 = 本组结束：冻结该组的间隔累计并开新组。
        if !chunk.is_empty() {
            self.freeze_group();
        }
        match self.streaming_mode {
            StreamingMode::Streaming => {
                self.current_ai_text.push_str(chunk);
                self.adaptive_policy.on_chunk(chunk);
            }
            StreamingMode::Block => {
                if self.push_chunk_block(chunk) {
                    self.flush_block_buffer();
                }
            }
            StreamingMode::None => {
                self.current_ai_text.push_str(chunk);
            }
        }
    }

    /// 追加推理 chunk
    pub fn push_reasoning(&mut self, text: &str) {
        // 思考一到达就进入 active（真机要求：切到思考立刻显示 ● Thinking）
        self.thinking_now = true;
        self.reasoning_flushed = false;
        self.group_active = true;
        if self.last_event_at.is_none() {
            self.note_event();
        }
        self.current_ai_reasoning.push_str(text);
        self.adaptive_policy.on_reasoning_chunk();
    }

    /// 本轮是否仍在"思考中"：组还活跃且还没开始输出正文。
    /// 渲染层用它决定整个视图里是否允许存在 active 的思考行。
    pub fn live_thinking_active(&self) -> bool {
        // 1) 本组还没结束、且还没开始输出正文；
        // 2) 必须真的"有东西在跑"：正在流式思考，或者还有工具没结束。
        //    否则会出现"上一组已结束、新组还没产出思考"时把旧行标成活跃的错觉。
        // 只要有未结束的工具（任何阶段，含排队/审批）或还在流式思考，就算"有活在跑"。
        // 注意：不能再用 `current_ai_text.is_empty()` 做条件——正文已经输出、模型接着跑工具时，
        // 文本仍在缓冲里，那样会把整段误判成已完成（真机表现：还在跑却显示 Thought）。
        // 思考中（chunk 已到达且正文还没来）或还有工具未结束 —— 两者都算"在跑"。
        self.thinking_now || !self.pending_tools.is_empty()
    }

    /// 打一个事件边界：把"距上一个事件"的间隔累加进当前组（单段封顶 600s）。
    /// 事件 = 思考首 chunk / ToolStart / ToolEnd / 消息落库，对应参考实现的相邻消息边界。
    fn note_event(&mut self) {
        let now = std::time::Instant::now();
        if let Some(prev) = self.last_event_at {
            self.group_ms = self
                .group_ms
                .saturating_add((now - prev).as_millis().min(600_000) as u64);
        }
        self.last_event_at = Some(now);
        self.group_active = true;
    }

    /// 正文到达 = 本组结束：冻结当前组耗时，开新组。
    fn freeze_group(&mut self) {
        // 冻结前把"最后一条事件到此刻"的增量也计入：这一段就是参考实现里
        // 相邻消息（思考消息本身）的时间差，缺了它会让 `Thinking for 22s` 冻结成
        // `Thought for 1s`（数字跳水）。
        let now = std::time::Instant::now();
        if let Some(prev) = self.last_event_at {
            self.group_ms = self
                .group_ms
                .saturating_add((now - prev).as_millis().min(600_000) as u64);
        }
        self.finished_groups.push(self.group_ms);
        self.group_ms = 0;
        self.group_active = false;
        self.thinking_now = false;
        self.last_event_at = Some(now);
    }

    // ─── Block 模式缓冲区管理 ────────────────────────────────────────────

    /// Block 模式下追加 chunk 到缓冲区并检测 block 边界。返回 true 表示检测到边界。
    ///
    /// 边界判定交给 [`crate::ui::markdown::BlockBoundaryScanner`]：按行扫描、
    /// 跨调用保留围栏状态，只扫新增行。旧实现用「最后一行是否以 ``` 开头」
    /// 的启发式判断围栏，正文中出现一整行裸 ``` 会误判成围栏开合。
    fn push_chunk_block(&mut self, chunk: &str) -> bool {
        self.block_buffer.push_str(chunk);
        self.block_scanner.scan(&self.block_buffer)
    }

    fn flush_block_buffer(&mut self) {
        if !self.block_buffer.is_empty() {
            // block 模式下正文从这里落库，同样意味着"本组结束"
            self.freeze_group();
            self.current_ai_text.push_str(&self.block_buffer);
            self.block_buffer.clear();
            self.block_scanner.reset();
            self.block_pending_flush = true;
        }
    }

    fn force_flush_block(&mut self) {
        self.flush_block_buffer();
        self.block_scanner.reset();
    }

    /// 按首次出现的顺序登记工具，Start 只更新 Pending 已登记的参数。
    fn register_tool_call(&mut self, id: &str, name: &str, input: &serde_json::Value) {
        let request = ToolCallRequest::new(id, name, input.clone());
        if let Some(existing) = self
            .current_ai_tool_calls
            .iter_mut()
            .find(|tool| tool.id == id)
        {
            *existing = request;
        } else {
            self.current_ai_tool_calls.push(request);
        }
    }

    /// 审批仅推进精确匹配的调用，迟到的弹窗不能把 Running/Finished 回退。
    pub(crate) fn mark_tool_awaiting_approval(&mut self, source: Option<&str>, id: &str) -> bool {
        if let Some(source) = source {
            if let Some(sub) = self.find_running_subagent_mut(source) {
                for vm in &mut sub.recent_messages {
                    if let MessageViewModel::ToolBlock {
                        tool_call_id,
                        phase,
                        ..
                    } = vm
                    {
                        if tool_call_id == id && *phase == ToolPhase::Queued {
                            *phase = ToolPhase::AwaitingApproval;
                            vm.recompute_hash();
                            return true;
                        }
                    }
                }
            }
            return false;
        }
        if let Some(pending) = self.pending_tools.get_mut(id) {
            if pending.phase == ToolPhase::Queued {
                pending.phase = ToolPhase::AwaitingApproval;
                return true;
            }
        }
        false
    }

    fn trim_subagent_preview(sub: &mut SubAgentState, target_len: usize) {
        while sub.recent_messages.len() > target_len {
            // 未完成工具必须保留，不能为了预览上限丢掉随后到达的状态与结果。
            if let Some(index) = sub.recent_messages.iter().position(|vm| {
                !matches!(vm, MessageViewModel::ToolBlock { phase, .. } if *phase != ToolPhase::Finished)
            }) {
                sub.recent_messages.remove(index);
            } else {
                break;
            }
        }
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
        self.note_event();
        self.register_tool_call(tool_call_id, name, &input);

        if name == "Agent" {
            // 与 message_view 的恢复路径共用同一套标签规则，避免重放时名字跳变
            // （fork 只在真的 fork 时写 "fork"，漏参的失败调用写 "unknown"）。
            let explicit_type = input["subagent_type"]
                .as_str()
                .filter(|name| !name.is_empty());
            let is_fork_call =
                explicit_type == Some("fork") || input["fork"].as_bool().unwrap_or(false);
            let agent_id = explicit_type.map(str::to_string).unwrap_or_else(|| {
                if is_fork_call {
                    "fork".into()
                } else {
                    "unknown".into()
                }
            });
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
            if let Some(ref mut batch) = self.active_batch {
                batch.started += 1;
            } else {
                self.active_batch = Some(BatchInfo {
                    started: 1,
                    completed: 0,
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
                phase: ToolPhase::Running,
                live_output: None,
                started_at: Some(Instant::now()),
            },
        );
    }

    /// 工具调用结束（内部版本，只更新状态，不返回 PipelineAction）
    fn tool_end_internal(&mut self, tool_call_id: &str, name: &str, output: &str, is_error: bool) {
        self.note_event();
        let pending = self.pending_tools.remove(tool_call_id);
        let input = pending
            .as_ref()
            .map(|p| p.input.clone())
            .or_else(|| {
                self.current_ai_tool_calls
                    .iter()
                    .find(|tool| tool.id == tool_call_id)
                    .map(|tool| tool.arguments.clone())
            })
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
                    let mut vm = MessageViewModel::SubAgentGroup {
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
                        content_hash: 0,
                    };
                    vm.recompute_hash();
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
    fn push_tool_to_subagent(
        sub: &mut SubAgentState,
        tool_call_id: &str,
        name: &str,
        input: &serde_json::Value,
        cwd: &str,
        next_phase: ToolPhase,
    ) {
        let display = tool_display::format_tool_name(name);
        let args = tool_display::format_tool_args(name, input, Some(cwd));
        if let Some(vm) = sub.recent_messages.iter_mut().find(|vm| {
            matches!(vm, MessageViewModel::ToolBlock { tool_call_id: id, .. } if id == tool_call_id)
        }) {
            if let MessageViewModel::ToolBlock {
                phase,
                started_at,
                timeout_ms,
                args_display,
                ..
            } = vm
            {
                if next_phase == ToolPhase::Running
                    && matches!(*phase, ToolPhase::Queued | ToolPhase::AwaitingApproval)
                {
                    *phase = ToolPhase::Running;
                    *started_at = Some(Instant::now());
                    *timeout_ms = input.get("timeout").and_then(serde_json::Value::as_u64);
                    *args_display = args;
                    vm.recompute_hash();
                }
            }
            return;
        }
        let mut vm = MessageViewModel::tool_block_with_id(
            tool_call_id.to_string(),
            name.to_string(),
            display,
            args,
            false,
        );
        if let MessageViewModel::ToolBlock {
            phase,
            source_agent_id,
            started_at,
            timeout_ms,
            ..
        } = &mut vm
        {
            *phase = next_phase;
            *source_agent_id = Some(sub.instance_id.clone());
            *started_at = (next_phase == ToolPhase::Running).then(Instant::now);
            *timeout_ms = input.get("timeout").and_then(serde_json::Value::as_u64);
        }
        vm.recompute_hash();
        sub.total_steps += 1;
        Self::trim_subagent_preview(sub, 3);
        sub.recent_messages.push(vm);
    }

    /// SubAgent 内部 chunk（路由进指定 SubAgentGroup）
    fn push_chunk_to_subagent(sub: &mut SubAgentState, chunk: &str) {
        match sub.recent_messages.last_mut() {
            Some(m) if m.is_assistant() => m.append_chunk(chunk),
            _ => {
                sub.total_steps += 1;
                Self::trim_subagent_preview(sub, 3);
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
                phase,
                live_output,
                ..
            } = vm
            {
                if tc_id == tool_call_id {
                    *content = output.to_string();
                    *err = is_error;
                    *phase = crate::ui::message_view::ToolPhase::Finished;
                    *live_output = None;
                    vm.recompute_hash();
                    break;
                }
            }
        }
        Self::trim_subagent_preview(sub, 4);
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
        self.reasoning_started_at = None;
        self.freeze_group();
        self.last_event_at = None;
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.drop_running_hooks();
        self.adaptive_policy.reset();
        self.force_flush_block();
        self.throttle_last_fire = None;
        self.active_batch = None;
        // 轮次结束：会话级 hook 行不再累加，下一轮同类事件另起一行
        self.hook_notes.clear();
        self.drain_subagent_stack();
    }

    /// 中断：finalize 当前状态并清理残留
    pub fn interrupt(&mut self) {
        // 中断时当前思考段已经结束：必须冻结计时并熄灭 `thinking_now`。
        // 否则 `live_thinking_active()`（thinking_now || 未结束工具）仍为真，
        // 视图装配点会保留最后一行的活跃标记 —— 真机表现是"取消后
        // `∴ Thinking for 54s…` 继续走秒、符号还在闪"。
        if self.group_active || self.thinking_now {
            self.freeze_group();
        }
        self.finalize_current_ai();
        self.current_ai_finalized = false;
        self.pending_tools.clear();
        self.completed_tools.clear();
        self.drop_running_hooks();
        self.adaptive_policy.reset();
        self.force_flush_block();
        self.throttle_last_fire = None;
        self.active_batch = None;
        // 中断同样视为轮次结束：避免下一轮的会话级 hook 行并进本轮
        self.hook_notes.clear();
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
                let mut vm = MessageViewModel::SubAgentGroup {
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
                    content_hash: 0,
                };
                vm.recompute_hash();
                self.frozen_subagent_vms.push(vm);
            } else if sub.finalized_vm.is_none() && sub.is_running && sub.is_background {
                // 后台 agent 仍在运行：冻结以保留当前 recent_messages，
                // 后续 BackgroundTaskCompleted 会直接更新 view_messages
                let mut vm = MessageViewModel::SubAgentGroup {
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
                    content_hash: 0,
                };
                vm.recompute_hash();
                self.frozen_subagent_vms.push(vm);
            }
            // 已 finalized（finalized_vm.is_some()）的不推入——tool_end_internal 已处理
            // 仍在运行的前台 agent（is_running && !is_background）不推入
        }
    }

    /// 清空所有状态
    pub fn clear(&mut self) {
        self.invalidate_completed_view_cache();
        self.shell_runtimes.clear();
        self.completed.clear();
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.reasoning_flushed = true;
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
        self.invalidate_completed_view_cache();
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
        self.invalidate_completed_view_cache();
        self.completed_len_at_round_start = self.completed.len();
        self.has_snapshot_this_round = false;
        self.adaptive_policy.reset();
        self.throttle_last_fire = None;
        // 清空上一轮的 frozen_subagent_vms，防止跨轮次累积导致新轮次的
        // SubAgentGroup 按位置错误匹配到旧轮的 frozen VM（而非本轮的）。
        self.frozen_subagent_vms.clear();
    }

    // ── 节流机制 ──────────────────────────────────────────────────────────────

    /// 检查自适应节流策略，根据流式渲染模式决定是否发射 RebuildAll。
    ///
    /// - Streaming 模式：自适应分块策略（Smooth/CatchUp）
    /// - Block 模式：检测 block_pending_flush 标记
    /// - None 模式：始终返回 None（不触发流式重绘）
    ///
    /// 由 poll_agent() 每帧调用。
    pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        match self.streaming_mode {
            StreamingMode::Streaming => self.check_throttle_streaming(prefix_len),
            StreamingMode::Block => self.check_throttle_block(prefix_len),
            StreamingMode::None => None,
        }
    }

    fn check_throttle_streaming(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        let plan = self.adaptive_policy.check()?;

        match plan {
            DrainPlan::Single => {
                let now = Instant::now();
                let min_interval = Duration::from_millis(16);
                let should_fire = match self.throttle_last_fire {
                    None => true,
                    Some(last) => now.duration_since(last) >= min_interval,
                };
                if !should_fire {
                    return None;
                }
                self.throttle_last_fire = Some(now);
                self.adaptive_policy.drain();
                Some(self.build_rebuild_all(prefix_len))
            }
            DrainPlan::Batch => {
                self.throttle_last_fire = Some(Instant::now());
                self.adaptive_policy.drain();
                Some(self.build_rebuild_all(prefix_len))
            }
        }
    }

    fn check_throttle_block(&mut self, prefix_len: usize) -> Option<PipelineAction> {
        if self.block_pending_flush {
            self.block_pending_flush = false;
            Some(self.build_rebuild_all(prefix_len))
        } else {
            None
        }
    }

    /// 获取已完成的 BaseMessages（用于持久化）
    pub fn completed_messages(&self) -> &[BaseMessage] {
        &self.completed
    }

    /// 从 pipeline 规范状态构建尾部 VMs。
    ///
    pub fn set_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.invalidate_completed_view_cache();
        self.completed.extend(msgs);
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.reasoning_flushed = true;
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = true;
        self.has_snapshot_this_round = true;
        self.pending_tools.clear();
        self.completed_tools.clear();
    }

    /// 返回 completed 的条数和估算堆内存（字节），供 /gc 诊断用
    pub fn completed_stats(&self) -> (usize, usize) {
        let count = self.completed.len();
        let bytes = super::super::command::core::gc::estimate_messages_heap(&self.completed);
        (count, bytes)
    }

    /// 从外部加载全量 BaseMessages（用于历史恢复后覆盖），并清除所有状态
    pub fn restore_completed(&mut self, msgs: Vec<BaseMessage>) {
        self.invalidate_completed_view_cache();
        self.completed = msgs;
        self.completed_len_at_round_start = self.completed.len();
        self.has_snapshot_this_round = false;
        self.current_ai_text.clear();
        self.current_ai_reasoning.clear();
        self.reasoning_flushed = true;
        self.current_ai_tool_calls.clear();
        self.current_ai_finalized = true;
    }
}

#[cfg(test)]
#[path = "message_pipeline_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tool_status_test.rs"]
mod tool_status_tests;

#[cfg(test)]
#[path = "real_session_replay_test.rs"]
mod real_session_replay_test;

#[cfg(test)]
#[path = "real_session_full_test.rs"]
mod real_session_full_test;

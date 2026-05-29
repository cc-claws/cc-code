use peri_agent::{agent::AgentCancellationToken, messages::BaseMessage};
use tokio::sync::mpsc;

use super::{events::AgentEvent, InteractionPrompt};
use crate::acp_client::AcpNotification;

type SharedToolRegistry = std::sync::Arc<
    parking_lot::RwLock<
        std::collections::HashMap<String, std::sync::Arc<dyn peri_agent::tools::BaseTool>>,
    >,
>;

/// LLM 重试状态（由 AgentEvent::LlmRetrying 更新）
pub struct RetryStatus {
    pub attempt: usize,
    pub max_attempts: usize,
    pub delay_ms: u64,
    /// 最近一次重试的错误描述（供状态栏展示）
    pub error: String,
}

/// 后台任务结构化结果（供 auto-continuation 注入合成 tool_use + tool_result）
pub type BgTaskResult = peri_agent::agent::events::BackgroundTaskResult;

/// Agent 通信状态：事件接收、交互弹窗、取消/计时
pub struct AgentComm {
    /// Legacy: AgentEvent receiver (will be replaced by acp_notification_rx in Step 6-e)
    pub agent_rx: Option<mpsc::Receiver<AgentEvent>>,
    /// ACP notification receiver (new path, replaces agent_rx)
    pub acp_notification_rx: Option<mpsc::UnboundedReceiver<AcpNotification>>,
    /// 当前激活的交互弹窗（HITL 审批或 AskUser 问答，同一时刻只有一种）
    pub interaction_prompt: Option<InteractionPrompt>,
    /// 已发送待解决的 HITL 工具名称列表（用于 approval_resolved 广播）
    pub pending_hitl_items: Option<Vec<String>>,
    /// AskUser 是否已提交（用于广播 resolved）
    pub pending_ask_user: Option<bool>,
    /// TUI 侧的消息历史缓存（流式输出快照，非权威数据源）。
    /// 权威持久化数据由 ACP server 的 state.history 维护。
    pub origin_messages: Vec<BaseMessage>,
    /// 当前 Agent 的 ID（用于 AgentDefineMiddleware 加载 agent 定义）
    pub agent_id: Option<String>,
    /// 当前 Agent 任务的取消令牌（loading 时有效，Ctrl+C 触发）
    pub cancel_token: Option<AgentCancellationToken>,
    /// 当前 Agent 任务开始时间（用于计算运行时长）
    pub task_start_time: Option<std::time::Instant>,
    /// 上一次任务的总运行时长（任务结束后保留显示）
    pub last_task_duration: Option<std::time::Duration>,
    /// 测试用事件注入队列（仅测试时使用，生产时保持为空）
    pub agent_event_queue: Vec<AgentEvent>,
    /// 会话级 token 累积追踪（从 AgentEvent::TokenUsageUpdate 聚合）
    pub session_token_tracker: peri_agent::agent::token::TokenTracker,
    /// 当前模型的上下文窗口大小（从最近一次 TokenUsageUpdate 中的 model 推断）
    pub context_window: u32,
    /// LLM 重试状态（重试中时为 Some，收到下一个正常事件时清除）
    pub retry_status: Option<RetryStatus>,
    /// SubAgent 执行深度计数器（>0 表示当前在 SubAgent 内，忽略其 TokenUsageUpdate）
    pub subagent_depth: u32,
    /// 会话开始时间（首次 submit_message 时记录）
    pub session_start_time: Option<std::time::Instant>,
    /// 会话级工具调用次数（统计 ToolStart 事件数）
    pub tool_call_count: u32,
    /// 后台任务全部完成后的待提交 continuation（结构化结果，用于注入合成 tool_use + tool_result）
    pub pending_bg_continuation: Option<Vec<BgTaskResult>>,
    /// Agent 已完成（Done/Error）但仍有后台任务在运行，
    /// 此时 agent_rx 保持存活以接收 BackgroundTaskCompleted 事件
    pub agent_done_pending_bg: bool,
    /// Agent 尚未 Done 但后台任务已完成的通知缓存（显示文本，供 pre-Done 路径使用）
    pub pre_done_bg_completions: Vec<String>,
    /// Agent 尚未 Done 但后台任务已完成的结构化结果缓存（供 auto-continuation 路径使用）
    pub pre_done_bg_results: Vec<BgTaskResult>,
    /// 本轮 agent 是否已产生回复（收到 TextChunk/ToolStart/AssistantChunk）
    pub agent_replied: bool,
    /// 标记 Interrupted/Error 处理器已完成 reconcile，Done 到达时应跳过重复 reconcile
    pub reconcile_already_done: bool,
    /// LSP 诊断计数（由 LspDiagnostics 事件更新）
    pub lsp_errors: usize,
    pub lsp_warnings: usize,
    pub lsp_files_with_errors: usize,
    /// 会话级 ToolSearch 索引（跨 submit 复用，缓存 deferred tools 提示词）
    pub tool_search_index: Option<std::sync::Arc<peri_middlewares::tool_search::ToolSearchIndex>>,
    /// 会话级共享工具注册表（跨 submit 复用）
    pub shared_tools: Option<SharedToolRegistry>,
    /// cancel 通知发送时间（用于超时 fallback：5s 后仍未收到事件则强制清理）
    pub cancel_sent_at: Option<std::time::Instant>,
    /// Pending ACP request ID for HITL/AskUser response routing
    pub pending_acp_request_id: Option<peri_acp::transport::types::RequestId>,
    /// 标记当前 compact 是手动触发的（/compact 命令），而非 auto-compact
    /// handle_compact_completed 据此决定是否调用 set_loading(false)
    pub compact_manual: bool,
}

impl Default for AgentComm {
    fn default() -> Self {
        Self {
            agent_rx: None,
            acp_notification_rx: None,
            interaction_prompt: None,
            pending_hitl_items: None,
            pending_ask_user: None,
            origin_messages: Vec::new(),
            agent_id: None,
            cancel_token: None,
            task_start_time: None,
            last_task_duration: None,
            agent_event_queue: Vec::new(),
            session_token_tracker: peri_agent::agent::token::TokenTracker::default(),
            context_window: 200_000,
            retry_status: None,
            subagent_depth: 0,
            session_start_time: None,
            tool_call_count: 0,
            pending_bg_continuation: None,
            agent_done_pending_bg: false,
            pre_done_bg_completions: Vec::new(),
            pre_done_bg_results: Vec::new(),
            agent_replied: false,
            reconcile_already_done: false,
            lsp_errors: 0,
            lsp_warnings: 0,
            lsp_files_with_errors: 0,
            tool_search_index: None,
            shared_tools: None,
            pending_acp_request_id: None,
            cancel_sent_at: None,
            compact_manual: false,
        }
    }
}

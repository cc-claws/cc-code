/// 后台任务完成通知（注入到主 agent 消息流中）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackgroundTaskResult {
    pub task_id: String,
    pub agent_name: String,
    pub prompt_summary: String,
    pub success: bool,
    pub output: String,
    pub tool_calls_count: usize,
    pub duration_ms: u64,
    /// SQLite child thread ID（uuid7），用于 TUI 聚焦时 load_messages
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_thread_id: Option<String>,
}

impl BackgroundTaskResult {
    /// 格式化为注入到 LLM 消息流的通知文本。
    ///
    /// 使用英文：这段文本属于 agent 内部上下文（不经过 TUI 的 i18n 层），
    /// 英文与 UI 语言解耦，避免非英文环境泄漏中文。
    pub fn to_notification(&self) -> String {
        let short_id = &self.task_id[..8.min(self.task_id.len())];
        if self.success {
            format!(
                "[Background task {} completed] Agent: {} | Tool calls: {} | Duration: {}ms\nResult:\n{}",
                short_id, self.agent_name, self.tool_calls_count, self.duration_ms, self.output,
            )
        } else {
            format!(
                "[Background task {} failed] Agent: {}\nError:\n{}",
                short_id, self.agent_name, self.output,
            )
        }
    }
}

/// hook 阻断或执行失败的描述（UI 展示用）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HookFailureInfo {
    /// `block` = 按策略阻断；`error` = 脚本崩溃 / 超时 / 输出非法
    pub kind: String,
    /// 给用户看的原因（hook 输出的 reason、超时说明或错误摘要）
    pub reason: String,
}

/// Compact 保留的文件信息摘要
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactFileInfo {
    pub path: String,
    pub lines: usize,
}

/// Todo 列表条目（用于 ExecutorEvent::TodoUpdate）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TodoEntry {
    pub content: String,
    #[serde(
        default,
        rename = "activeForm",
        skip_serializing_if = "Option::is_none"
    )]
    pub active_form: Option<String>,
    pub status: TodoStatus,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// 系统提示的严重级别（供 UI 决定配色与图标）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warn,
    Error,
}

/// Agent 执行过程中的增量事件
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AgentEvent {
    /// 子线程事件信封。新增过程事件自动保留来源，不得解包后送进父线程。
    SubagentEvent {
        source_agent_id: String,
        event: Box<AgentEvent>,
    },
    /// AI 推理内容（reasoning/思考过程）
    AiReasoning(String),
    /// LLM 输出最终文字（非流式，整段答案），携带所属 AI 消息的 message_id
    TextChunk {
        message_id: crate::messages::MessageId,
        chunk: String,
        source_agent_id: Option<String>,
    },
    /// 工具已排队，审批与语义门尚未完成；仅更新展示，不写入 AgentState。
    ToolPending {
        message_id: crate::messages::MessageId,
        tool_call_id: String,
        name: String,
        input: serde_json::Value,
        source_agent_id: Option<String>,
    },
    /// 工具调用开始（工具名 + 参数），携带所属 AI 消息的 message_id
    ToolStart {
        message_id: crate::messages::MessageId,
        tool_call_id: String,
        name: String,
        input: serde_json::Value,
        /// 执行侧采集的写入前快照；旧事件没有此字段。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_snapshot: Option<crate::tools::FileSnapshot>,
        source_agent_id: Option<String>,
    },
    /// 工具运行中的有界输出快照，仅供展示，不进入消息历史。
    ToolOutput {
        message_id: crate::messages::MessageId,
        tool_call_id: String,
        tail: String,
        total_lines: usize,
        total_bytes: usize,
        elapsed_ms: u64,
        source_agent_id: Option<String>,
    },
    /// 工具调用结束（结果或错误），携带所属 AI 消息的 message_id
    ToolEnd {
        message_id: crate::messages::MessageId,
        tool_call_id: String,
        name: String,
        output: String,
        is_error: bool,
        source_agent_id: Option<String>,
    },
    /// 状态快照（含完整的消息历史），用于持久化和断点续跑
    StateSnapshot(Vec<crate::messages::BaseMessage>),
    /// 增量消息（BaseMessage），持久化和遥测的最小数据单元
    MessageAdded(crate::messages::BaseMessage),
    /// LLM 调用开始（携带完整 input messages 快照 + 工具定义，用于 Langfuse Generation）
    LlmCallStart {
        step: usize,
        /// Arc 共享引用——Clone AgentEvent 时为浅拷贝（引用计数 +1），不产生独立副本
        messages: std::sync::Arc<Vec<crate::messages::BaseMessage>>,
        tools: Vec<crate::tools::ToolDefinition>,
    },
    /// LLM 调用结束（携带模型名、输出文本、token 使用量）
    LlmCallEnd {
        step: usize,
        model: String,
        output: String,
        usage: Option<crate::llm::types::TokenUsage>,
        /// LLM 响应停止原因（None 表示 LLM 调用失败/异常）
        stop_reason: Option<crate::llm::types::StopReason>,
    },
    /// 上下文窗口使用警告（阈值触发时发出）
    ContextWarning {
        used_tokens: u64,
        total_tokens: u64,
        percentage: f64,
    },
    /// LLM 调用重试中
    LlmRetrying {
        attempt: usize,
        max_attempts: usize,
        delay_ms: u64,
        error: String,
    },
    /// 系统级运行状态提示（非阻塞）。
    ///
    /// 与 [`Self::LlmRetrying`] / [`Self::ContextWarning`] 同类：面向用户的
    /// **运行态播报**，不进入消息历史（也不该伪装成模型说的），由 UI 决定
    /// 展示位置（状态栏 / 浮层）。
    ///
    /// 存在的理由：中间件层过去只有「弹窗」和「塞 System 消息」两条出口，
    /// 而 System 消息在 TUI 被过滤——于是"安全判定已降级"这类**必须让用户
    /// 知道**的事实完全没有展示通道，退化成逐条弹窗。
    SystemNotice { level: NoticeLevel, message: String },
    /// 后台 agent 任务完成（TUI 使用，用于空闲时通知）
    BackgroundTaskCompleted(BackgroundTaskResult),
    /// 子 agent 开始执行
    SubagentStarted {
        agent_name: String,
        /// 唯一实例标识符（用于并发同类型 SubAgent 路由）
        instance_id: String,
        /// 是否为后台模式（run_in_background）
        is_background: bool,
        #[serde(default)]
        task_preview: String,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        prompt: Option<String>,
    },
    /// 子 agent 执行完成
    SubagentStopped {
        agent_name: String,
        result: String,
        is_error: bool,
        /// 唯一实例标识符
        instance_id: String,
        #[serde(default)]
        elapsed_ms: u64,
    },
    /// 子 agent 进度增量事件（用于后台 agent 步数同步）
    SubagentProgress {
        /// 唯一实例标识符
        instance_id: String,
        /// 当前已执行工具步数
        total_steps: usize,
    },
    /// 上下文压缩开始
    CompactStarted {
        /// 是否为手动触发（/compact 命令为 true，CompactMiddleware 自动触发为 false）
        #[serde(default)]
        manual: bool,
    },
    /// 上下文压缩完成
    CompactCompleted {
        /// 摘要文本（full compact 时非空，micro compact 时为空）
        summary: String,
        /// 保留的文件摘要列表
        files: Vec<CompactFileInfo>,
        /// 保留的 Skill 名称列表
        skills: Vec<String>,
        /// micro-compact 清除的工具结果数量（>0 表示 micro-compact）
        micro_cleared: usize,
        /// 压缩后的新消息列表（full compact 时非空）
        messages: Vec<crate::messages::BaseMessage>,
    },
    /// 对话回退完成（rewind 命令，移除目标用户消息及其之后的所有消息）
    RewindCompleted {
        /// 摘要文本（如"已回滚 N 条消息"）
        summary: String,
        /// 回退后的新消息列表（目标消息之前，不含目标本身）
        messages: Vec<crate::messages::BaseMessage>,
    },
    /// 一批 hook 执行完成（同步事件一次、异步 hook 各自一次）
    ///
    /// 只用于 UI 展示，不进入 agent history：TUI 在详细模式下渲染为
    /// `N <Event> hooks ran · Xms`，阻断/失败在普通模式也显示。
    HooksCompleted {
        /// 触发的事件名（PreToolUse / PostToolUse / Stop ...）
        event: String,
        /// 真正执行成功的 hook 数（被 matcher / once 跳过的不计）
        ran: usize,
        /// 该事件所有同步 hook 的总耗时
        duration_ms: u64,
        /// 阻断与执行失败清单
        failures: Vec<HookFailureInfo>,
        /// true 表示这是异步 hook 完成后的单独上报
        is_async: bool,
        /// 归属的工具调用；非工具事件为 None
        tool_call_id: Option<String>,
    },
    /// 上下文压缩失败
    CompactError { message: String },
    /// 会话回顾生成完成（/recap 命令，输出 ≤40/60 字的单句摘要）
    RecapCompleted {
        /// 摘要文本（plain text，无 markdown）
        text: String,
    },
    /// 会话回顾生成失败
    RecapError { message: String },
    /// Todo 列表更新
    TodoUpdate(Vec<TodoEntry>),
    /// LSP 诊断更新
    LspDiagnostics {
        errors: usize,
        warnings: usize,
        files_with_errors: usize,
    },
    /// Agent 执行失败（由 executor 在 agent.execute() 返回 Err 时发送）
    AgentExecutionFailed { message: String },
}

/// 事件回调 trait（应用层实现）
///
/// 在 `ReActAgent` 执行过程中，关键节点会调用 `on_event`。
/// 实现者通过 `mpsc::Sender` 等机制将事件转发给 UI 层。
pub trait AgentEventHandler: Send + Sync {
    fn on_event(&self, event: AgentEvent);

    /// 是否需要 `LlmCallStart.messages`（完整消息历史快照）与 `tools` 载荷。
    ///
    /// 该载荷需要把**整个消息历史**（含工具结果正文、图片 base64）深拷贝一份，
    /// 长会话下每轮 LLM 调用可达 MB 级（#369）。目前唯一的消费者是 Langfuse
    /// 追踪（TUI 与 ACP mapper 均丢弃该事件）。
    ///
    /// 返回 `false` 时，executor 发出的 `LlmCallStart` 携带**空载荷**
    /// （`Arc::new(Vec::new())` / 空 tools），不执行拷贝。
    /// 默认 `false`——不关心该事件的 handler 无需感知此方法；
    /// 包装型 handler 应委托内部 handler 的声明（见 `SourceAgentIdHandler`）。
    fn wants_llm_call_payload(&self) -> bool {
        false
    }
}

/// 函数闭包适配器 —— 方便快速实现 `AgentEventHandler`
///
/// # 示例
/// ```rust,ignore
/// let tx = tx.clone();
/// let handler = FnEventHandler(move |event| {
///     let _ = tx.try_send(event);
/// });
/// executor.with_event_handler(Arc::new(handler))
/// ```
pub struct FnEventHandler<F>(pub F)
where
    F: Fn(AgentEvent) + Send + Sync;

impl<F> AgentEventHandler for FnEventHandler<F>
where
    F: Fn(AgentEvent) + Send + Sync,
{
    fn on_event(&self, event: AgentEvent) {
        (self.0)(event)
    }
}

#[cfg(test)]
#[path = "events_test.rs"]
mod tests;

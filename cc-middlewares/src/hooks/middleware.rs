use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};

use cc_agent::{
    agent::{
        events::{AgentEvent, HookFailureInfo, HookFailureKind},
        react::{AgentOutput, ReactLLM, ToolCall, ToolResult},
        state::State,
    },
    error::{AgentError, AgentResult},
    messages::BaseMessage,
    middleware::Middleware,
};

use crate::hitl::{PermissionMode, SharedPermissionMode};
use crate::hooks::{
    executor::{execute_command_hook, execute_http_hook},
    permission_bridge::HookPermissionBridge,
    runtime::HookRuntime,
    types::{HookAction, HookEvent, HookInput, HookType, PermissionDecision, RegisteredHook},
};

/// Plugin hook middleware — fires registered hooks at lifecycle events.
pub struct HookMiddleware {
    /// 可跨组件共享的执行器（hook 注册表、once 状态、UI 通道都在这里）。
    runtime: Arc<HookRuntime>,
    cwd: String,
    session_id: String,
    transcript_path: String,
    /// 共享权限模式（运行时可变，Shift+Tab 切换）。
    /// PermissionRequest 仅在权限对话框即将展示时触发。
    permission_mode: Arc<SharedPermissionMode>,
    current_model: String,
    /// SessionStart hook 的触发与场景。None 表示不触发；Some(source) 表示触发并
    /// 携带 matcher：startup（新会话首次 prompt）/ resume（恢复历史会话）/
    /// clear（/clear 后首次 prompt）/ compact（compact 后）。
    /// 对齐 CC SessionStart 的 4 种 matcher。
    session_start_source: Option<String>,
    /// 判断工具是否需要用户审批。用于 PermissionRequest hook 门控。
    /// 默认使用 [`crate::hitl::default_requires_approval`]，
    /// 可通过 `with_requires_approval` 覆盖。
    requires_approval: fn(&str) -> bool,
    /// 与 HITL 共享的放行通道：`permissionDecision: allow` 的调用写在这里，
    /// 审批中间件据此跳过弹框（对齐参考实现的 hook 裁决语义）。
    permission_bridge: Option<Arc<HookPermissionBridge>>,
}

impl HookMiddleware {
    pub fn new(
        registered_hooks: Vec<RegisteredHook>,
        llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
        cwd: impl Into<String>,
        session_id: impl Into<String>,
        transcript_path: impl Into<String>,
        permission_mode: Arc<SharedPermissionMode>,
        current_model: impl Into<String>,
    ) -> Self {
        Self::with_session_start(
            registered_hooks,
            llm_factory,
            cwd,
            session_id,
            transcript_path,
            permission_mode,
            current_model,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_session_start(
        registered_hooks: Vec<RegisteredHook>,
        llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
        cwd: impl Into<String>,
        session_id: impl Into<String>,
        transcript_path: impl Into<String>,
        permission_mode: Arc<SharedPermissionMode>,
        current_model: impl Into<String>,
        session_start_source: Option<String>,
    ) -> Self {
        let mut map: HashMap<HookEvent, Vec<RegisteredHook>> = HashMap::new();
        for hook in registered_hooks {
            map.entry(hook.event.clone()).or_default().push(hook);
        }
        let event_count = map.len();
        let total_hooks: usize = map.values().map(|v| v.len()).sum();
        tracing::info!(
            total_hooks,
            event_count,
            session_start_source = ?session_start_source,
            "HookMiddleware created with registered hooks"
        );
        let hooks = Arc::new(RwLock::new(map));
        let once_fired = Arc::new(Mutex::new(HashSet::new()));
        let event_tx = Arc::new(Mutex::new(None));
        let cwd = cwd.into();
        let session_id = session_id.into();
        let transcript_path = transcript_path.into();
        let current_model = current_model.into();
        let runtime = HookRuntime::new(
            Arc::clone(&hooks),
            llm_factory,
            cwd.clone(),
            session_id.clone(),
            transcript_path.clone(),
            Arc::clone(&permission_mode),
            Arc::clone(&once_fired),
            Arc::clone(&event_tx),
        );
        Self {
            runtime,
            cwd,
            session_id,
            transcript_path,
            permission_mode,
            current_model,
            session_start_source,
            requires_approval: crate::hitl::default_requires_approval,
            permission_bridge: None,
        }
    }

    // -----------------------------------------------------------------------
    // Hook 执行上报
    // -----------------------------------------------------------------------

    /// 注入 UI 事件通道。未注入时 hook 执行对用户完全静默（历史行为）。
    pub fn with_event_tx(self, tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>) -> Self {
        self.runtime.set_event_tx(tx);
        self
    }

    /// 注入与 HITL 共享的放行通道（由 `build_agent` 同时交给两个中间件）。
    pub fn with_permission_bridge(mut self, bridge: Arc<HookPermissionBridge>) -> Self {
        self.permission_bridge = Some(bridge);
        self
    }

    /// 取出可跨组件共享的执行器（工具、审批流程用它触发链外事件）。
    pub fn runtime(&self) -> Arc<HookRuntime> {
        Arc::clone(&self.runtime)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// 判断当前权限模式下，给定工具是否会触发权限对话框。
    ///
    /// 对齐 CC：PermissionRequest 仅在权限对话框即将展示时触发。
    fn needs_permission_dialog(&self, tool_name: &str) -> bool {
        let _ = tool_name;
        match self.permission_mode.load() {
            // Bypass: 所有工具直接放行，无对话框
            PermissionMode::Bypass => false,
            // AutoMode: 由语义门判定；这里保守地认为"可能弹窗"，
            // 以免 hook 系统依赖分类器/门的具体结论。
            PermissionMode::AutoMode => true,
        }
    }

    /// 应用 hook 返回的 `permissionDecision`（对齐参考实现的裁决语义）。
    ///
    /// 返回 `Some(result)` 表示本方法已给出终局结果；`None` 表示不干预、
    /// 继续走原有审批流程。
    ///
    /// - `Deny` → 直接拒绝（不执行、不弹框）
    /// - `Allow` → 写入与 HITL 共享的放行通道（审批跳过弹框）；
    ///   若同时带回 `updatedInput`，以新输入执行
    /// - `Ask` / `Passthrough` → 不干预
    fn apply_permission_decision(
        &self,
        tool_call: &ToolCall,
        decision: &PermissionDecision,
        reason: Option<&String>,
        new_input: &Option<serde_json::Value>,
    ) -> Option<AgentResult<ToolCall>> {
        match decision {
            PermissionDecision::Deny => Some(Err(AgentError::ToolRejected {
                tool: tool_call.name.clone(),
                reason: reason
                    .cloned()
                    .unwrap_or_else(|| "Denied by hook permissionDecision".to_string()),
            })),
            PermissionDecision::Allow => {
                if let Some(bridge) = &self.permission_bridge {
                    bridge.allow(&tool_call.id);
                }
                Some(Ok(new_input.as_ref().map_or_else(
                    || tool_call.clone(),
                    |input| ToolCall {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        input: input.clone(),
                    },
                )))
            }
            PermissionDecision::Ask | PermissionDecision::Passthrough => None,
        }
    }

}

#[async_trait]
impl<S: State> Middleware<S> for HookMiddleware {
    fn name(&self) -> &str {
        "HookMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // Extract the latest human message as prompt text
        let prompt = state
            .messages()
            .iter()
            .rev()
            .find(|m| matches!(m, BaseMessage::Human { .. }))
            .map(|m| m.content())
            .unwrap_or_default();

        // SessionStart: 触发条件由 session_start_source 决定，source 同时作为
        // CC matcher 字段（startup/resume/clear/compact）传给 hook。
        if let Some(source) = self.session_start_source.as_ref() {
            let input = HookInput::session_start(
                &self.session_id,
                &self.transcript_path,
                &self.cwd,
                source,
                &self.current_model,
            );
            let action = self
                .runtime.fire(HookEvent::SessionStart, &input, None, None, None)
                .await;
            match &action {
                HookAction::Block { reason } => {
                    return Err(AgentError::ToolRejected {
                        tool: "SessionStart".to_string(),
                        reason: reason.clone(),
                    });
                }
                HookAction::PreventContinuation { stop_reason } => {
                    let reason = stop_reason
                        .clone()
                        .unwrap_or_else(|| "SessionStart hook prevented continuation".to_string());
                    return Err(AgentError::ToolRejected {
                        tool: "SessionStart".to_string(),
                        reason,
                    });
                }
                HookAction::SystemMessage { message } => {
                    tracing::info!("SessionStart hook system message: {}", message);
                }
                HookAction::AdditionalContext { context } => {
                    tracing::info!("SessionStart hook additional context: {}", context);
                }
                HookAction::InitialUserMessage { message } => {
                    tracing::info!("SessionStart hook initial user message: {}", message);
                }
                _ => {}
            }
        }

        // UserPromptSubmit: on every user prompt
        let input = HookInput::user_prompt_submit(
            &self.session_id,
            &self.transcript_path,
            &self.cwd,
            &prompt,
        );
        let action = self
            .runtime.fire(HookEvent::UserPromptSubmit, &input, None, None, None)
            .await;

        // Handle UserPromptSubmit actions
        match &action {
            HookAction::Block { reason } => {
                return Err(AgentError::ToolRejected {
                    tool: "UserPromptSubmit".to_string(),
                    reason: reason.clone(),
                });
            }
            HookAction::PreventContinuation { stop_reason } => {
                let reason = stop_reason
                    .clone()
                    .unwrap_or_else(|| "Hook prevented continuation".to_string());
                return Err(AgentError::ToolRejected {
                    tool: "UserPromptSubmit".to_string(),
                    reason,
                });
            }
            _ => {}
        }

        Ok(())
    }

    async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
        let permission_mode_str = format!("{:?}", self.permission_mode.load());
        let input = HookInput::tool_call(
            &self.session_id,
            &self.transcript_path,
            &self.cwd,
            &permission_mode_str,
            &tool_call.name,
            &tool_call.input,
            &tool_call.id,
        );

        // Fire PreToolUse
        let action = self
            .runtime.fire(
                HookEvent::PreToolUse,
                &input,
                Some(&tool_call.name),
                Some(&tool_call.input),
                Some(&tool_call.id),
            )
            .await;

        match &action {
            HookAction::Block { reason } => {
                return Err(AgentError::ToolRejected {
                    tool: tool_call.name.clone(),
                    reason: reason.clone(),
                });
            }
            HookAction::PreventContinuation { stop_reason } => {
                let reason = stop_reason
                    .clone()
                    .unwrap_or_else(|| "Hook prevented continuation".to_string());
                return Err(AgentError::ToolRejected {
                    tool: tool_call.name.clone(),
                    reason,
                });
            }
            HookAction::ModifyInput { new_input } => {
                return Ok(ToolCall {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    input: new_input.clone(),
                });
            }
            HookAction::PermissionOverride {
                decision,
                reason,
                new_input,
            } => {
                // allow 是终局裁决：直接返回，不再触发 PermissionRequest / Notification
                //（对齐 CC：PreToolUse 放行后不再进入权限请求流程）。
                if let Some(result) =
                    self.apply_permission_decision(tool_call, decision, reason.as_ref(), new_input)
                {
                    return result;
                }
            }
            _ => {}
        }

        // PermissionRequest 门控：仅对敏感工具 + 权限对话框即将展示时触发。
        //
        // CC 行为：PermissionRequest 仅在权限对话框即将展示给用户时触发。
        // Bypass（全放行）与 Auto 的自动放行路径都不展示对话框，因此不触发。
        //
        // 使用 hitl::default_requires_approval 判断工具是否需要审批（Bash/Write/Edit/Agent/
        // mcp__*/WebFetch/WebSearch 等）。非敏感工具（Read/Glob/Grep 等）不触发。
        let is_sensitive = (self.requires_approval)(&tool_call.name);
        let needs_dialog = self.needs_permission_dialog(&tool_call.name);

        if is_sensitive && needs_dialog {
            let action = self
                .runtime.fire(
                    HookEvent::PermissionRequest,
                    &input,
                    Some(&tool_call.name),
                    Some(&tool_call.input),
                    Some(&tool_call.id),
                )
                .await;

            match &action {
                HookAction::Block { reason } => {
                    return Err(AgentError::ToolRejected {
                        tool: tool_call.name.clone(),
                        reason: reason.clone(),
                    });
                }
                HookAction::PreventContinuation { stop_reason } => {
                    let reason = stop_reason
                        .clone()
                        .unwrap_or_else(|| "Hook prevented continuation".to_string());
                    return Err(AgentError::ToolRejected {
                        tool: tool_call.name.clone(),
                        reason,
                    });
                }
                HookAction::ModifyInput { new_input } => {
                    return Ok(ToolCall {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        input: new_input.clone(),
                    });
                }
                HookAction::PermissionOverride {
                    decision,
                    reason,
                    new_input,
                } => {
                    if let Some(result) = self.apply_permission_decision(
                        tool_call,
                        decision,
                        reason.as_ref(),
                        new_input,
                    ) {
                        return result;
                    }
                }
                _ => {}
            }

            // Fire Notification (agent is waiting for user permission)
            self.runtime.fire(
                HookEvent::Notification,
                &input,
                Some(&tool_call.name),
                Some(&tool_call.input),
                Some(&tool_call.id),
            )
            .await;
        }

        Ok(tool_call.clone())
    }

    async fn after_tool(
        &self,
        _state: &mut S,
        tool_call: &ToolCall,
        result: &ToolResult,
    ) -> AgentResult<()> {
        // 放行标记的生命周期到此为止：敏感工具已被 HITL 取走，非敏感工具
        // 根本不会经过审批。残留会让集合随会话无界增长。
        if let Some(bridge) = &self.permission_bridge {
            bridge.discard(&tool_call.id);
        }

        let event = if result.is_error {
            HookEvent::PostToolUseFailure
        } else {
            HookEvent::PostToolUse
        };

        let permission_mode_str = format!("{:?}", self.permission_mode.load());
        let input = HookInput::tool_result(
            &self.session_id,
            &self.transcript_path,
            &self.cwd,
            &permission_mode_str,
            &tool_call.name,
            &tool_call.input,
            &serde_json::json!(result.output),
            result.is_error,
        );

        let _action = self
            .runtime.fire(
                event,
                &input,
                Some(&tool_call.name),
                Some(&tool_call.input),
                Some(&tool_call.id),
            )
            .await;

        Ok(())
    }

    async fn after_agent(&self, _state: &mut S, output: &AgentOutput) -> AgentResult<AgentOutput> {
        // 构造 Stop hook 的 HookInput。
        // subagent_result 携带 agent 最终输出（截断到 500 字符），
        // source 携带 stop_reason（若存在）标识结束原因。
        let input = HookInput {
            session_id: self.session_id.clone(),
            transcript_path: self.transcript_path.clone(),
            cwd: self.cwd.clone(),
            permission_mode: Some(format!("{:?}", self.permission_mode.load())),
            agent_id: None,
            agent_type: None,
            hook_event_name: HookEvent::Stop,
            tool_name: None,
            tool_input: None,
            tool_use_id: None,
            tool_output: None,
            prompt: None,
            source: output
                .stop_reason
                .as_deref()
                .map(|_| "agent_complete".to_string()),
            model: Some(self.current_model.clone()),
            subagent_name: None,
            subagent_result: Some(output.text.chars().take(500).collect::<String>()),
            message_count: None,
            name: None,
            worktree_path: None,
            reason: None,
            old_cwd: None,
            new_cwd: None,
        };

        let stop_action = self.runtime.fire(HookEvent::Stop, &input, None, None, None).await;

        // Stop hook Block/PreventContinuation 语义：CC 规范中，Stop hook
        // 返回 block 时 agent 不应停止而应继续工作。这里捕获 action 并把 reason
        // 附加到 AgentOutput.stop_reason，调用方（ACP server / TUI）可识别并自动
        // 重提一轮 prompt。当前实现是"信号透传 + 日志"，更深层的 auto-continue
        // 需要在 execute_prompt 顶层循环处理。
        let output = match stop_action {
            HookAction::Block { reason } => {
                tracing::info!(
                    reason = %reason,
                    "Stop hook blocked; surfacing continue signal to caller"
                );
                let mut next = output.clone();
                next.stop_reason = Some(format!("stop_hook_blocked: {reason}"));
                next
            }
            HookAction::PreventContinuation { stop_reason } => {
                let reason =
                    stop_reason.unwrap_or_else(|| "Stop hook prevented continuation".to_string());
                tracing::info!(reason = %reason, "Stop hook prevented continuation");
                let mut next = output.clone();
                next.stop_reason = Some(reason);
                next
            }
            _ => output.clone(),
        };

        // Fire Notification (agent done, waiting for user input)
        self.runtime.fire(HookEvent::Notification, &input, None, None, None)
            .await;

        Ok(output.clone())
    }

    async fn on_error(
        &self,
        _state: &mut S,
        error: &cc_agent::error::AgentError,
    ) -> AgentResult<()> {
        // 工具被拒绝 → PermissionDenied。
        //
        // 拒绝可能来自审批（HITL）、hook 自身（Block）或权限模式；`on_error` 是唯一
        // 能同时看到这些路径的钩子。payload 只带工具名与原因——被拒的工具输入不再
        // 外发一次。
        if let AgentError::ToolRejected { tool, reason } = error {
            self.runtime.fire_permission_denied(tool, reason).await;
        }

        // 按 CC 规范，StopFailure 仅在 API/LLM 错误导致轮次结束时触发。
        // 其他错误类型（用户中断、最大迭代次数、工具拒绝等）不触发 StopFailure。
        //
        // 触发 StopFailure 的变体：
        // - LlmError / LlmHttpError：LLM 调用失败
        //
        // 不触发的变体：
        // - Interrupted：用户主动 Ctrl+C，非失败
        // - MaxIterationsExceeded：达到循环上限，非 API 错误
        // - ToolRejected：HITL 或 hook 拒绝工具，非 API 错误
        // - ToolNotFound / ToolExecutionFailed：工具层面错误，agent 可继续
        // - MiddlewareError / SerializationError / Other：非 API 错误
        let is_api_error = matches!(
            error,
            AgentError::LlmError(_) | AgentError::LlmHttpError { .. }
        );
        if !is_api_error {
            return Ok(());
        }

        // API 错误路径不经过 after_agent（直接返回 Err），需要在此处单独触发 StopFailure。
        let error_description = format!("{:?}", error);
        let input = HookInput {
            session_id: self.session_id.clone(),
            transcript_path: self.transcript_path.clone(),
            cwd: self.cwd.clone(),
            permission_mode: Some(format!("{:?}", self.permission_mode.load())),
            agent_id: None,
            agent_type: None,
            hook_event_name: HookEvent::StopFailure,
            tool_name: None,
            tool_input: None,
            tool_use_id: None,
            tool_output: Some(serde_json::json!(error_description)),
            prompt: None,
            source: None,
            model: Some(self.current_model.clone()),
            subagent_name: None,
            subagent_result: None,
            message_count: None,
            name: None,
            worktree_path: None,
            reason: None,
            old_cwd: None,
            new_cwd: None,
        };

        self.runtime.fire(HookEvent::StopFailure, &input, None, None, None)
            .await;

        Ok(())
    }
}

/// Fire standalone lifecycle hooks outside of the middleware lifecycle.
///
/// Used by the TUI layer for events that occur outside the agent ReAct loop:
/// - `SessionEnd`: when `/clear` resets the session
/// - `PreCompact` / `PostCompact`: before/after context compaction
/// - `Notification`: when agent needs user attention (e.g. AskUserQuestion)
///
/// The HookMiddleware instance is owned by the agent task and not accessible
/// from these code paths, so we dispatch hooks directly.
///
/// 本入口不带事件通道，执行结果**不上报**（历史行为）。需要在 UI 上可见的旁路
/// 调用点改用 [`fire_standalone_lifecycle_hooks_with_tx`]。
#[allow(clippy::too_many_arguments)]
pub async fn fire_standalone_lifecycle_hooks(
    registered_hooks: &[RegisteredHook],
    event: HookEvent,
    cwd: &str,
    session_id: &str,
    transcript_path: &str,
    current_model: &str,
    message_count: Option<usize>,
    source: Option<&str>,
) {
    fire_standalone_lifecycle_hooks_with_tx(
        registered_hooks,
        event,
        cwd,
        session_id,
        transcript_path,
        current_model,
        message_count,
        source,
        // 未注入通道：静默执行（TUI 的 SessionEnd 等既有调用点走这里）
        None,
    )
    .await;
}

/// 同 [`fire_standalone_lifecycle_hooks_with_tx`]，但等待所有异步 hook 完成。
///
/// 进程退出路径（TUI 退出、窗口关闭）专用：async hook 是 fire-and-forget 的
/// `tokio::spawn` 任务，若不等待，runtime 提前 drop 会把任务连同 SessionEnd
/// 通知一起取消（clawd 永远收不到 "sleeping"）。`EXIT_HOOK_WAIT` 总超时只覆盖
/// async hook 的 join；同步 hook 在 `with_tx` 内联执行、只受各自 hook 超时约束
/// （默认 600s），关窗路径最终由 OS ~5s 强杀兜底，不能指望这里的超时兜底。
#[allow(clippy::too_many_arguments)]
pub async fn fire_standalone_lifecycle_hooks_blocking(
    registered_hooks: &[RegisteredHook],
    event: HookEvent,
    cwd: &str,
    session_id: &str,
    transcript_path: &str,
    current_model: &str,
    message_count: Option<usize>,
    source: Option<&str>,
) {
    let handles = fire_standalone_lifecycle_hooks_with_tx(
        registered_hooks,
        event.clone(),
        cwd,
        session_id,
        transcript_path,
        current_model,
        message_count,
        source,
        None,
    )
    .await;
    let join_all = async {
        for handle in handles {
            // 单个 hook panic/取消不影响其余 hook
            let _ = handle.await;
        }
    };
    if tokio::time::timeout(EXIT_HOOK_WAIT, join_all).await.is_err() {
        tracing::warn!(
            event = ?event,
            "standalone lifecycle hooks 超时未全部完成，继续退出"
        );
    }
}

/// 退出路径等待 hook 完成的总超时。
///
/// Windows `CTRL_CLOSE_EVENT` 只给约 5s 就强杀进程，控制台 handler 侧还要
/// 留出进程收尾时间；正常退出路径 8s 也足够 node/脚本类 hook 跑完一次通知。
pub(crate) const EXIT_HOOK_WAIT: std::time::Duration = std::time::Duration::from_secs(8);

/// 同 [`fire_standalone_lifecycle_hooks`]，但把执行结果汇总上报到事件通道。
///
/// 旁路调用点没有 `HookMiddleware` 实例，不能复用 `emit_hooks_completed`；这里用
/// 传进来的 sender 发同一份 [`AgentEvent::HooksCompleted`]，口径与主链一致：
/// 同步 hook 循环结束后上报一次（`is_async = false`），异步 hook 各自完成后单独
/// 上报一次（`is_async = true`）。`tool_call_id` 恒为 `None`——生命周期事件不归属
/// 任何工具调用，TUI 侧渲染成独立的 `⎿` 行。
///
/// 返回异步 hook 的 `JoinHandle`（同步 hook 结束时已完成，不包含在内）：调用方
/// 可以选择等待它们完成（退出路径用 [`fire_standalone_lifecycle_hooks_blocking`]），
/// 也可以忽略返回值保持 fire-and-forget 行为。
#[allow(clippy::too_many_arguments)]
pub async fn fire_standalone_lifecycle_hooks_with_tx(
    registered_hooks: &[RegisteredHook],
    event: HookEvent,
    cwd: &str,
    session_id: &str,
    transcript_path: &str,
    current_model: &str,
    message_count: Option<usize>,
    source: Option<&str>,
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>,
) -> Vec<tokio::task::JoinHandle<()>> {
    // SessionEnd 不上报：会话结束（退出或 /clear 切换）时没有稳定的渲染时机，
    // 展示出来也没有意义。PRD「决策记录」明确排除该事件。
    let event_tx = if event == HookEvent::SessionEnd {
        None
    } else {
        event_tx
    };

    // Filter hooks matching the event
    let matching: Vec<&RegisteredHook> = registered_hooks
        .iter()
        .filter(|h| h.event == event)
        .collect();

    if matching.is_empty() {
        return Vec::new();
    }

    let input = match &event {
        HookEvent::SessionEnd => HookInput {
            session_id: session_id.to_string(),
            transcript_path: transcript_path.to_string(),
            cwd: cwd.to_string(),
            permission_mode: None,
            agent_id: None,
            agent_type: None,
            hook_event_name: event.clone(),
            tool_name: None,
            tool_input: None,
            tool_use_id: None,
            tool_output: None,
            prompt: None,
            // SessionEnd 的 reason：clear（/clear 新建 thread）、prompt_input_exit（输入框
            // 可见时退出：/exit、/quit、双击 Ctrl+C）、resume（恢复其他会话导致当前会话
            // 结束）、other（关窗等系统性退出）。
            // 调用方按场景传值，对齐 CC SessionEnd reason 字段。
            source: source.map(|s| s.to_string()),
            model: Some(current_model.to_string()),
            subagent_name: None,
            subagent_result: None,
            message_count: None,
            name: None,
            worktree_path: None,
            reason: None,
            old_cwd: None,
            new_cwd: None,
        },
        HookEvent::PreCompact | HookEvent::PostCompact => HookInput::compact(
            session_id,
            transcript_path,
            cwd,
            event.clone(),
            message_count.unwrap_or(0),
        ),
        HookEvent::Notification => HookInput {
            session_id: session_id.to_string(),
            transcript_path: transcript_path.to_string(),
            cwd: cwd.to_string(),
            permission_mode: None,
            agent_id: None,
            agent_type: None,
            hook_event_name: event.clone(),
            tool_name: None,
            tool_input: None,
            tool_use_id: None,
            tool_output: None,
            prompt: None,
            source: None,
            model: Some(current_model.to_string()),
            subagent_name: None,
            subagent_result: None,
            message_count: None,
            name: None,
            worktree_path: None,
            reason: None,
            old_cwd: None,
            new_cwd: None,
        },
        _ => return Vec::new(),
    };

    let started = std::time::Instant::now();
    let mut ran = 0usize;
    let mut failures: Vec<HookFailureInfo> = Vec::new();
    let mut async_handles = Vec::new();

    for registered in matching {
        if let Some(ref msg) = registered.hook.get_status_message() {
            tracing::info!(
                plugin = %registered.plugin_name,
                event = ?event,
                "Hook status: {}",
                msg
            );
        }

        if registered.hook.is_async() {
            // Fire-and-forget async hook：完成后单独上报 is_async summary。
            // handle 返回给调用方，退出路径可选择等待完成。
            let hook = registered.hook.clone();
            let input = input.clone();
            let registered = registered.clone();
            let event_name = event.as_str().to_string();
            let event_tx = event_tx.clone();
            let handle = tokio::spawn(async move {
                let started = std::time::Instant::now();
                let action = match &hook {
                    HookType::Command { .. } => {
                        execute_command_hook(&hook, &input, &registered).await
                    }
                    HookType::Http { .. } => execute_http_hook(&hook, &input).await,
                    _ => HookAction::Allow,
                };
                let failures: Vec<HookFailureInfo> = hook_failure(&action).into_iter().collect();
                emit_hooks_summary(
                    event_tx.as_ref(),
                    &event_name,
                    usize::from(failures.is_empty()),
                    started,
                    &failures,
                    true,
                    None,
                );
            });
            async_handles.push(handle);
            continue;
        }

        let action = match &registered.hook {
            HookType::Command { .. } => {
                execute_command_hook(&registered.hook, &input, registered).await
            }
            HookType::Http { .. } => execute_http_hook(&registered.hook, &input).await,
            // Prompt/Agent 需要 LLM factory，standalone 上下文没有：视为未执行，
            // 不计入 ran，避免 summary 谎报「跑了几条」。
            HookType::Prompt { .. } | HookType::Agent { .. } => {
                tracing::debug!(
                    event = ?event,
                    "standalone hook skipped: LLM factory unavailable"
                );
                continue;
            }
        };

        // 旁路不做短路：Block/Error 只作为失败项上报，控制流由调用方决定。
        ran += 1;
        if let Some(failure) = hook_failure(&action) {
            failures.push(failure);
        }
    }

    emit_hooks_summary(
        event_tx.as_ref(),
        event.as_str(),
        ran,
        started,
        &failures,
        false,
        None,
    );
    async_handles
}

#[cfg(test)]
#[path = "middleware_test.rs"]
mod tests;

/// 把一次 hook 执行汇总成 [`AgentEvent::HooksCompleted`] 发到事件通道。
///
/// 主链（`HookMiddleware::emit_hooks_completed`）与旁路（standalone lifecycle、
/// subagent lifecycle）共用这一份口径，避免两处统计漂移。
///
/// - `ran == 0 && failures.is_empty()`（一条 hook 都没真正执行，或被 matcher/once 全跳过）
///   时不上报，避免 UI 噪音。
/// - `event_tx` 为 `None` 表示没有 UI 订阅，完全静默。
/// - `tool_call_id` 为 `None` 表示非工具事件。
pub(crate) fn emit_hooks_summary(
    event_tx: Option<&tokio::sync::mpsc::UnboundedSender<AgentEvent>>,
    event: &str,
    ran: usize,
    started: std::time::Instant,
    failures: &[HookFailureInfo],
    is_async: bool,
    tool_call_id: Option<&str>,
) {
    if ran == 0 && failures.is_empty() {
        return;
    }
    let Some(tx) = event_tx else {
        return;
    };
    let _ = tx.send(AgentEvent::HooksCompleted {
        event: event.to_string(),
        ran,
        duration_ms: started.elapsed().as_millis() as u64,
        failures: failures.to_vec(),
        is_async,
        tool_call_id: tool_call_id.map(str::to_string),
    });
}

/// 把 hook 的阻断/失败语义转成 UI 可渲染的失败项。
///
/// `Block` 是策略性阻断（琥珀色），`Error` 是 hook 自身出错（红色）——后者
/// **不改变控制流**，只是把「你配的 hook 坏了」如实告诉用户。
pub(crate) fn hook_failure(action: &HookAction) -> Option<HookFailureInfo> {
    match action {
        HookAction::Block { reason } => Some(HookFailureInfo {
            kind: HookFailureKind::Block,
            reason: reason.clone(),
        }),
        HookAction::Error { reason } => Some(HookFailureInfo {
            kind: HookFailureKind::Error,
            reason: reason.clone(),
        }),
        HookAction::PreventContinuation { stop_reason } => Some(HookFailureInfo {
            kind: HookFailureKind::Block,
            reason: stop_reason
                .clone()
                .unwrap_or_else(|| "prevented continuation".to_string()),
        }),
        _ => None,
    }
}

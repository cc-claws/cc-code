//! Hook 执行运行时。
//!
//! `HookMiddleware` 负责「在中间件链的什么时机触发」，本模块负责「怎么执行」。
//!
//! 抽出来的原因：hook 的触发点不只在中间件链上——`WorktreeCreate` /
//! `WorktreeRemove` 由工具触发，`PermissionDenied` 由审批流程触发。这些组件
//! 拿不到 `HookMiddleware`，但可以共享同一个 [`HookRuntime`]。

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use parking_lot::{Mutex, RwLock};

use cc_agent::agent::{
    events::{AgentEvent, HookFailureInfo},
    react::ReactLLM,
};

use crate::hitl::SharedPermissionMode;
use crate::hooks::{
    executor::{
        execute_agent_hook, execute_command_hook, execute_command_hook_captured, execute_http_hook,
        execute_prompt_hook,
    },
    matcher::{matches_if_condition, matches_matcher},
    middleware::{emit_hooks_summary, hook_failure},
    types::{HookAction, HookEvent, HookInput, HookType, RegisteredHook},
};

/// `WorktreeCreate` hook 的执行结果。
///
/// 对齐参考实现的「hook 优先」：一旦 hook 在 stdout 交出路径，就**不再调用 git**，
/// 直接使用该目录（非 git 版本控制系统正是靠这个接入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeCreateHookOutcome {
    /// 没有配置该事件 hook —— 走 git 原生流程。
    NotConfigured,
    /// hook 配置了但没有交出路径 —— 仍走 git 流程。
    AllowedWithoutPath,
    /// hook 交出路径 —— 直接使用，不调用 git。
    Path(String),
    /// hook 拒绝创建。
    Blocked(String),
}

/// 可跨组件共享的 hook 执行器。
///
/// 所有挂载点（中间件、工具、审批流程）通过 `Arc<HookRuntime>` 共享同一份
/// 注册表与执行状态：`once` 语义、UI 上报通道、插件变量解析都只有一份。
pub struct HookRuntime {
    hooks: Arc<RwLock<HashMap<HookEvent, Vec<RegisteredHook>>>>,
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    cwd: String,
    session_id: String,
    transcript_path: String,
    permission_mode: Arc<SharedPermissionMode>,
    once_fired: Arc<Mutex<HashSet<String>>>,
    event_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>>>,
}

impl HookRuntime {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hooks: Arc<RwLock<HashMap<HookEvent, Vec<RegisteredHook>>>>,
        llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
        cwd: impl Into<String>,
        session_id: impl Into<String>,
        transcript_path: impl Into<String>,
        permission_mode: Arc<SharedPermissionMode>,
        once_fired: Arc<Mutex<HashSet<String>>>,
        event_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            hooks,
            llm_factory,
            cwd: cwd.into(),
            session_id: session_id.into(),
            transcript_path: transcript_path.into(),
            permission_mode,
            once_fired,
            event_tx,
        })
    }

    /// 注入 UI 事件通道。未注入时 hook 执行对用户完全静默。
    pub fn set_event_tx(&self, tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>) {
        *self.event_tx.lock() = Some(tx);
    }

    /// 该事件是否注册了任意 hook（供「hook 优先」分支判断）。
    pub fn has_hooks(&self, event: &HookEvent) -> bool {
        self.hooks
            .read()
            .get(event)
            .is_some_and(|hooks| !hooks.is_empty())
    }

    /// 构造带会话上下文的空输入，供链外触发点（工具、审批）补齐 payload。
    pub fn base_input(&self, event: HookEvent) -> HookInput {
        HookInput {
            session_id: self.session_id.clone(),
            transcript_path: self.transcript_path.clone(),
            cwd: self.cwd.clone(),
            permission_mode: Some(format!("{:?}", self.permission_mode.load())),
            agent_id: None,
            agent_type: None,
            hook_event_name: event,
            tool_name: None,
            tool_input: None,
            tool_use_id: None,
            tool_output: None,
            prompt: None,
            source: None,
            model: None,
            subagent_name: None,
            subagent_result: None,
            message_count: None,
            name: None,
            worktree_path: None,
            reason: None,
            old_cwd: None,
            new_cwd: None,
        }
    }

    /// 触发 `PermissionDenied`。
    ///
    /// 只带工具名与拒绝原因，**不携带 tool_input**：该事件发生在拒绝之后，
    /// 把被拒内容再外发一次没有语义收益，只会扩大外发面。
    pub async fn fire_permission_denied(&self, tool_name: &str, reason: &str) -> HookAction {
        let mut input = self.base_input(HookEvent::PermissionDenied);
        input.tool_name = Some(tool_name.to_string());
        input.reason = Some(reason.to_string());
        self.fire(HookEvent::PermissionDenied, &input, None, None, None)
            .await
    }

    /// 触发 `CwdChanged`。
    ///
    /// 参考实现里该事件还能回传 `watchPaths` 动态增加监控范围；peri 没有文件
    /// 监控能力（`FileChanged` 未实现），因此只承载「目录变了」这一事实。
    pub async fn fire_cwd_changed(&self, old_cwd: &str, new_cwd: &str) -> HookAction {
        let mut input = self.base_input(HookEvent::CwdChanged);
        input.old_cwd = Some(old_cwd.to_string());
        input.new_cwd = Some(new_cwd.to_string());
        self.fire(HookEvent::CwdChanged, &input, None, None, None)
            .await
    }

    /// 执行 `WorktreeCreate` hook，返回它给出的 worktree 路径。
    ///
    /// 与 [`Self::fire`] 的区别：本事件需要 hook 的 stdout 作为结果，因此走
    /// [`execute_command_hook_captured`]。`once` 语义、失败计数与 UI 上报口径
    /// 与主链保持一致。**只执行命令型 hook**——prompt/agent/http 无法通过 stdout
    /// 交出路径，配置了也只当作没有路径。
    pub async fn run_worktree_create_hook(&self, name: &str) -> WorktreeCreateHookOutcome {
        let hooks = {
            let map = self.hooks.read();
            match map.get(&HookEvent::WorktreeCreate) {
                Some(hooks) if !hooks.is_empty() => hooks.clone(),
                _ => return WorktreeCreateHookOutcome::NotConfigured,
            }
        };
        let input = HookInput::worktree_create(
            &self.session_id,
            &self.transcript_path,
            &self.cwd,
            &format!("{:?}", self.permission_mode.load()),
            name,
        );

        let started = std::time::Instant::now();
        let mut ran = 0usize;
        let mut failures: Vec<HookFailureInfo> = Vec::new();
        let mut outcome = WorktreeCreateHookOutcome::AllowedWithoutPath;

        for registered in &hooks {
            if !matches!(registered.hook, HookType::Command { .. }) {
                continue;
            }
            if Self::is_once_hook(&registered.hook) && self.was_once_fired(registered) {
                continue;
            }
            let (action, stdout) =
                execute_command_hook_captured(&registered.hook, &input, registered).await;
            ran += 1;
            if let Some(failure) = hook_failure(&action) {
                failures.push(failure);
            }
            if Self::is_once_hook(&registered.hook) {
                self.mark_once_fired(registered);
            }
            match action {
                HookAction::Block { reason } => {
                    outcome = WorktreeCreateHookOutcome::Blocked(reason);
                    break;
                }
                HookAction::PreventContinuation { stop_reason } => {
                    outcome = WorktreeCreateHookOutcome::Blocked(
                        stop_reason.unwrap_or_else(|| "WorktreeCreate hook stopped".to_string()),
                    );
                    break;
                }
                HookAction::Allow => {
                    let path = stdout.trim();
                    if !path.is_empty() {
                        outcome = WorktreeCreateHookOutcome::Path(path.to_string());
                        break;
                    }
                }
                // Error 按「hook 坏了但不阻断」处理，继续看下一个 hook。
                _ => {}
            }
        }

        self.emit_hooks_completed(
            &HookEvent::WorktreeCreate,
            ran,
            started,
            &failures,
            false,
            None,
        );
        outcome
    }

    /// 触发 `WorktreeRemove`；返回的 `HookAction` 若为 Block 则拒绝删除。
    pub async fn fire_worktree_remove(&self, worktree_path: &str) -> HookAction {
        let input = HookInput::worktree_remove(
            &self.session_id,
            &self.transcript_path,
            &self.cwd,
            &format!("{:?}", self.permission_mode.load()),
            worktree_path,
        );
        self.fire(HookEvent::WorktreeRemove, &input, None, None, None)
            .await
    }

    // -----------------------------------------------------------------------
    // 执行循环
    // -----------------------------------------------------------------------

    pub async fn fire(
        &self,
        event: HookEvent,
        input: &HookInput,
        tool_name: Option<&str>,
        tool_input: Option<&serde_json::Value>,
        tool_call_id: Option<&str>,
    ) -> HookAction {
        // 确保 hook_event_name 与实际触发的事件一致。
        //
        // 调用方可能在 before_tool 中复用同一个 HookInput 连续触发多个事件
        // （PreToolUse → PermissionRequest → Notification），而 HookInput::tool_call()
        // 构造函数硬编码 hook_event_name = PreToolUse。若不修正，PermissionRequest hook
        // 脚本从 stdin 读到的 hook_event_name 会是 "PreToolUse" 而非 "PermissionRequest"。
        let input = if input.hook_event_name != event {
            let mut corrected = input.clone();
            corrected.hook_event_name = event.clone();
            corrected
        } else {
            input.clone()
        };

        // Notification 是「等待用户」这类纯通知事件，不携带工具输入。
        //
        // 对齐 CC `executeNotificationHooks`：payload 只有 message / title /
        // notification_type。工具输入属于审批与执行语义，放进通知事件等于在
        // 用户做出决定之前再外发一次完整内容（同一调用最坏三次中的第三次）。
        let input = if event == HookEvent::Notification {
            let mut stripped = input;
            stripped.tool_name = None;
            stripped.tool_input = None;
            stripped.tool_use_id = None;
            stripped
        } else {
            input
        };

        let hooks = {
            let map = self.hooks.read();
            match map.get(&event) {
                Some(h) => {
                    tracing::debug!(
                        event = ?event,
                        count = h.len(),
                        "HookRuntime: found hooks for event"
                    );
                    h.clone()
                }
                None => {
                    return HookAction::Allow;
                }
            }
        };

        if hooks.is_empty() {
            return HookAction::Allow;
        }

        let started = std::time::Instant::now();
        let mut final_action = HookAction::Allow;
        let mut ran = 0usize;
        let mut failures: Vec<HookFailureInfo> = Vec::new();

        for registered in &hooks {
            // once check
            if Self::is_once_hook(&registered.hook) && self.was_once_fired(registered) {
                continue;
            }

            // matcher check
            if let Some(name) = tool_name {
                let matcher_str = registered.matcher.as_deref().unwrap_or_else(|| {
                    registered
                        .hook
                        .get_matcher()
                        .map(|s| s.as_str())
                        .unwrap_or("*")
                });
                if !matches_matcher(matcher_str, name) {
                    continue;
                }
            }

            // if condition check
            if let Some(condition) = registered.hook.get_condition() {
                if let (Some(name), Some(inp)) = (tool_name, tool_input) {
                    if !matches_if_condition(condition, name, inp) {
                        continue;
                    }
                }
            }

            // Execute hook (async hooks are spawned in background, result ignored)
            if let Some(ref msg) = registered.hook.get_status_message() {
                tracing::info!(
                    plugin = %registered.plugin_name,
                    event = ?event,
                    "Hook status: {}",
                    msg
                );
            }
            let action = if registered.hook.is_async() {
                // Fire-and-forget: spawn in background, return Allow immediately。
                // 完成后单独上报一次 is_async 的 summary（见 PRD C9/C10）。
                let hook = registered.hook.clone();
                let owned_input = input.clone();
                let registered = registered.clone();
                let event_name = event.as_str().to_string();
                let event_tx = Arc::clone(&self.event_tx);
                let tool_call_id = tool_call_id.map(str::to_string);
                // 先报开始：TUI 立刻挂一条 Running 占位。完成事件可能晚于工具块
                // 进入历史前缀，届时已无位置可贴（见 hooks-display.md）。
                if let Some(tx) = event_tx.lock().as_ref() {
                    let _ = tx.send(AgentEvent::HooksStarted {
                        event: event_name.clone(),
                        tool_call_id: tool_call_id.clone(),
                    });
                }
                tokio::spawn(async move {
                    let started = std::time::Instant::now();
                    let action = match &hook {
                        HookType::Command { .. } => {
                            execute_command_hook(&hook, &owned_input, &registered).await
                        }
                        HookType::Http { .. } => execute_http_hook(&hook, &owned_input).await,
                        // Prompt/Agent hooks need LLM factory which can't be cloned into spawn;
                        // async only applies to Command per schema definition.
                        _ => HookAction::Allow,
                    };
                    let failures: Vec<HookFailureInfo> =
                        hook_failure(&action).into_iter().collect();
                    let guard = event_tx.lock();
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(AgentEvent::HooksCompleted {
                            event: event_name,
                            ran: usize::from(failures.is_empty()),
                            duration_ms: started.elapsed().as_millis() as u64,
                            failures,
                            is_async: true,
                            tool_call_id,
                        });
                    }
                });
                HookAction::Allow
            } else {
                match &registered.hook {
                    HookType::Command { .. } => {
                        execute_command_hook(&registered.hook, &input, registered).await
                    }
                    HookType::Prompt { .. } => {
                        execute_prompt_hook(&registered.hook, &input, &self.llm_factory).await
                    }
                    HookType::Http { .. } => execute_http_hook(&registered.hook, &input).await,
                    HookType::Agent { .. } => {
                        execute_agent_hook(&registered.hook, &input, &self.llm_factory, &self.cwd)
                            .await
                    }
                }
            };

            // 同步 hook：计入 ran，阻断/失败收进 failures
            if !registered.hook.is_async() {
                ran += 1;
                if let Some(failure) = hook_failure(&action) {
                    failures.push(failure);
                }
            }

            // once mark
            if Self::is_once_hook(&registered.hook) {
                self.mark_once_fired(registered);
            }

            // Short-circuit on Block / PreventContinuation
            match &action {
                HookAction::Block { .. } | HookAction::PreventContinuation { .. } => {
                    self.emit_hooks_completed(&event, ran, started, &failures, false, tool_call_id);
                    return action;
                }
                HookAction::ModifyInput { new_input } => {
                    final_action = HookAction::ModifyInput {
                        new_input: new_input.clone(),
                    };
                }
                HookAction::PermissionOverride {
                    decision,
                    reason,
                    new_input,
                } => {
                    tracing::debug!(
                        "PermissionOverride from hook: {:?} (reason: {:?})",
                        decision,
                        reason
                    );
                    final_action = HookAction::PermissionOverride {
                        decision: decision.clone(),
                        reason: reason.clone(),
                        new_input: new_input.clone(),
                    };
                }
                // hook 自身出错（非预期退出码/超时/信号/启动失败）不阻断调用：
                // 已计入 failures 供 UI 展示，控制流继续按 Allow 走。
                HookAction::Error { .. } => {}
                _ => {}
            }
        }

        self.emit_hooks_completed(&event, ran, started, &failures, false, tool_call_id);
        final_action
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// 上报一次 hook 执行结果（同步事件一次，异步 hook 各自一次）。
    fn emit_hooks_completed(
        &self,
        event: &HookEvent,
        ran: usize,
        started: std::time::Instant,
        failures: &[HookFailureInfo],
        is_async: bool,
        tool_call_id: Option<&str>,
    ) {
        let guard = self.event_tx.lock();
        emit_hooks_summary(
            guard.as_ref(),
            event.as_str(),
            ran,
            started,
            failures,
            is_async,
            tool_call_id,
        );
    }

    fn is_once_hook(hook: &HookType) -> bool {
        hook.is_once()
    }

    fn once_key(registered: &RegisteredHook) -> String {
        format!(
            "{}:{}:{:?}",
            registered.plugin_id,
            serde_json::to_string(&registered.hook).unwrap_or_default(),
            registered.event
        )
    }

    fn was_once_fired(&self, registered: &RegisteredHook) -> bool {
        let key = Self::once_key(registered);
        self.once_fired.lock().contains(&key)
    }

    fn mark_once_fired(&self, registered: &RegisteredHook) {
        let key = Self::once_key(registered);
        self.once_fired.lock().insert(key);
    }
}

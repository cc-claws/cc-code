use std::sync::Arc;

use async_trait::async_trait;
use peri_agent::{
    agent::{react::ToolCall, state::State},
    error::{AgentError, AgentResult},
    interaction::{
        ApprovalDecision, ApprovalItem, InteractionContext, InteractionResponse,
        UserInteractionBroker,
    },
    middleware::r#trait::Middleware,
};

use crate::tool_search::core_tools::{
    TOOL_AGENT, TOOL_BASH, TOOL_EDIT, TOOL_WEBFETCH, TOOL_WEBSEARCH, TOOL_WRITE,
};

pub mod approval_memory;
pub mod auto_classifier;
pub mod jev;
pub mod shared_mode;

pub use approval_memory::ApprovalMemory;
pub use auto_classifier::{AutoClassifier, Classification, LlmAutoClassifier};
pub use jev::{GateDecision, JevGate, JevRules};
pub use peri_agent::hitl::{BatchItem, HitlDecision};
pub use shared_mode::{PermissionMode, SharedPermissionMode};

// ─── YOLO 模式检测 ─────────────────────────────────────────────────────────────

/// 检测是否处于 YOLO 模式（默认启用）
///
/// - `YOLO_MODE` 未设置或为 `true`/`1` → YOLO（跳过审批）
/// - `YOLO_MODE=false`/`0` → 启用 HITL 审批
pub fn is_yolo_mode() -> bool {
    std::env::var("YOLO_MODE")
        .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
        .unwrap_or(true)
}

// ─── 默认规则 ──────────────────────────────────────────────────────────────────

/// 默认敏感工具判断规则
///
/// - `bash`：所有 bash 命令
/// - `Write`：文件写入
/// - `Edit`：文件编辑
/// - `launch_agent`：子 Agent 委派（子 Agent 不含 HITL，可传递绕过审批）
pub fn default_requires_approval(tool_name: &str) -> bool {
    tool_name == TOOL_BASH
        || tool_name == TOOL_AGENT
        || tool_name == TOOL_WRITE
        || tool_name == TOOL_EDIT
        || tool_name.starts_with("delete_")
        || tool_name.starts_with("rm_")
        || tool_name == TOOL_WEBFETCH
        || tool_name == TOOL_WEBSEARCH
        || tool_name.starts_with("mcp__")
}

/// 判断工具是否为文件编辑类工具（AcceptEdits 模式使用）
///
/// `Write`、`Edit` 归类为编辑工具，在 AcceptEdits 模式下自动放行。
/// `Bash`、`Agent`、`delete_*`、`rm_*` 不属于编辑工具，仍需审批。
pub fn is_edit_tool(tool_name: &str) -> bool {
    tool_name == TOOL_WRITE || tool_name == TOOL_EDIT
}

// ─── ExecuteExtraTool 权限透传 ─────────────────────────────────────────────

/// 获取有效的工具名称
///
/// 当 tool_name 为 [`crate::tool_search::core_tools::EXECUTE_EXTRA_TOOL_NAME`] 时，
/// 从 `input[EXTRA_TOOL_NAME_FIELD]` 提取目标工具名，用于 HITL 权限判断。
/// 否则直接返回原始工具名。
pub use crate::tool_search::core_tools::resolve_effective_tool_name as effective_tool_name;

// ─── HumanInTheLoopMiddleware ──────────────────────────────────────────────────

/// HumanInTheLoopMiddleware — 敏感工具调用前需用户确认
///
/// 在 `before_tool` 时拦截工具调用，通过注入的 [`UserInteractionBroker`] 请求用户审批。
///
/// # HITL 模式
/// 通过 `HumanInTheLoopMiddleware::new(...)` 或环境变量 `YOLO_MODE=false` 启用审批。
pub struct HumanInTheLoopMiddleware {
    broker: Option<Arc<dyn UserInteractionBroker>>,
    requires_approval: fn(&str) -> bool,
    /// 共享权限模式（动态切换），None 时走原有 Some/None broker 逻辑（向后兼容）
    mode: Option<Arc<SharedPermissionMode>>,
    /// Auto 模式的 LLM 分类器（旧实现，作为无 Jev 时的兜底）
    auto_classifier: Option<Arc<dyn AutoClassifier>>,
    /// Auto 模式的 Jev 语义门（优先使用；不可用时回退 auto_classifier）
    jev_gate: Option<Arc<JevGate>>,
    /// 审批记忆（路径级，会话作用域）。同 `(工具, 路径)` 一经批准即免问，
    /// 规避语义门对绝对路径反复弹窗的抖动。`None` 时不启用记忆。
    approval_memory: Option<Arc<ApprovalMemory>>,
}

impl HumanInTheLoopMiddleware {
    /// 创建启用的 HITL 中间件，使用注入的 broker
    pub fn new(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
    ) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
            mode: None,
            auto_classifier: None,
            jev_gate: None,
            approval_memory: None,
        }
    }

    /// YOLO 模式：所有工具调用直接放行
    pub fn disabled() -> Self {
        Self {
            broker: None,
            requires_approval: default_requires_approval,
            mode: None,
            auto_classifier: None,
            jev_gate: None,
            approval_memory: None,
        }
    }

    /// 从环境变量决定是否启用（默认 YOLO；`YOLO_MODE=false` 则启用审批）
    pub fn from_env(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
    ) -> Self {
        if is_yolo_mode() {
            Self::disabled()
        } else {
            Self::new(broker, requires_approval)
        }
    }

    /// 创建带共享权限模式的 HITL 中间件
    pub fn with_shared_mode(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
        mode: Arc<SharedPermissionMode>,
        auto_classifier: Option<Arc<dyn AutoClassifier>>,
        jev_gate: Option<Arc<JevGate>>,
    ) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
            mode: Some(mode),
            auto_classifier,
            jev_gate,
            approval_memory: None,
        }
    }

    /// 创建带共享权限模式 + 审批记忆的 HITL 中间件。
    ///
    /// `approval_memory` 由 session 层持有（middleware 每次 prompt 重建，
    /// 记忆必须跨 prompt 存活）。命中记忆的 `(工具, 路径)` 直接放行，跳过语义判定。
    pub fn with_shared_mode_and_memory(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
        mode: Arc<SharedPermissionMode>,
        auto_classifier: Option<Arc<dyn AutoClassifier>>,
        jev_gate: Option<Arc<JevGate>>,
        approval_memory: Arc<ApprovalMemory>,
    ) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
            mode: Some(mode),
            auto_classifier,
            jev_gate,
            approval_memory: Some(approval_memory),
        }
    }

    /// 构建一次调用的 `GateCall`（提取 cwd、命令/路径）。
    ///
    /// 刻意**不读 `state.messages()`**：用户对话不参与判定（见 `GateCall` 文档），
    /// 这也让本中间件符合"链上中间件在 before_tool 阶段不读消息历史"的不变量。
    fn build_gate_call<S: State>(&self, state: &S, tool_call: &ToolCall) -> Option<jev::GateCall> {
        // 没有门就不构建（调用方据此跳过判定）
        self.jev_gate.as_ref()?;
        let effective = effective_tool_name(&tool_call.name, &tool_call.input);
        // ExecuteExtraTool 把真实参数包在 `params` 里，必须解包才能看到 command/path
        let params = jev::effective_params(&tool_call.name, &tool_call.input);
        Some(jev::GateCall {
            tool_name: effective,
            command: params
                .get("command")
                .and_then(|v| v.as_str())
                .map(String::from),
            path: params
                .get("file_path")
                .or_else(|| params.get("path"))
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from),
            // 仓库现场：读 `.git/HEAD`，零成本。规则常带条件（"不要在 main 上提交"），
            // 缺了它 judge 只能匹配命令里出现的分支名。
            branch: jev::policy::git_branch(std::path::Path::new(state.cwd())),
            cwd: std::path::PathBuf::from(state.cwd()),
        })
    }
}

/// 将 `ApprovalDecision` 映射为 `AgentResult<ToolCall>`
fn apply_decision(call: &ToolCall, decision: ApprovalDecision) -> AgentResult<ToolCall> {
    match decision {
        ApprovalDecision::Approve { .. } => Ok(call.clone()),
        ApprovalDecision::Edit { new_input } => {
            let mut modified = call.clone();
            modified.input = new_input;
            Ok(modified)
        }
        ApprovalDecision::Reject { reason, source: _ } => Err(AgentError::ToolRejected {
            tool: call.name.clone(),
            reason,
        }),
        ApprovalDecision::Respond { message } => Err(AgentError::ToolRejected {
            tool: call.name.clone(),
            reason: message,
        }),
    }
}

impl HumanInTheLoopMiddleware {
    /// 批量处理一批工具调用：收集所有需要审批的项，一次性弹窗，返回每个 call 的处理结果
    pub async fn process_batch<S: State>(
        &self,
        state: &S,
        calls: &[ToolCall],
    ) -> Vec<AgentResult<ToolCall>> {
        let mut results: Vec<AgentResult<ToolCall>> = Vec::with_capacity(calls.len());

        // 快照当前 mode，确保整个批处理内评估一致（避免迭代过程中 mode 被外部修改）
        let mode_snapshot = self.mode.clone();

        for (i, call) in calls.iter().enumerate() {
            // 非敏感工具 → 直接放行（ExecuteExtraTool 透传目标工具名）
            let effective_name = effective_tool_name(&call.name, &call.input);
            if !(self.requires_approval)(&effective_name) {
                results.push(Ok(call.clone()));
                continue;
            }

            // 有 mode → 使用快照模式决策
            if let Some(mode) = &mode_snapshot {
                results.push(self.decide_by_mode(state, mode, call).await);
                continue;
            }

            // 无 mode 且无 broker → 放行
            let Some(broker) = &self.broker else {
                results.push(Ok(call.clone()));
                continue;
            };

            // 无 mode 但有 broker → 收集后批量弹窗
            return self
                .batch_broker_approve(
                    broker,
                    calls,
                    i,
                    &mut results,
                    std::path::Path::new(state.cwd()),
                )
                .await;
        }

        results
    }

    /// 通过 broker 请求用户审批单个工具调用。
    ///
    /// 用户**批准**后，把 `(工具, 路径)` 记入审批记忆，后续同类调用免问
    /// （仅 `Approve` 记录；`Edit`/`Reject` 不记录）。
    async fn broker_approve(
        &self,
        broker: &Arc<dyn UserInteractionBroker>,
        tool_call: &ToolCall,
        cwd: &std::path::Path,
    ) -> AgentResult<ToolCall> {
        let ctx = InteractionContext::Approval {
            items: vec![ApprovalItem {
                tool_call_id: tool_call.id.clone(),
                tool_name: tool_call.name.clone(),
                tool_input: tool_call.input.clone(),
            }],
        };
        let response = broker.request(ctx).await;
        let decision = match response {
            InteractionResponse::Decisions(mut d) => d.pop().unwrap_or(ApprovalDecision::Reject {
                reason: "用户拒绝".to_string(),
                source: None,
            }),
            _ => ApprovalDecision::Reject {
                reason: "用户拒绝".to_string(),
                source: None,
            },
        };
        // 仅「本次会话同意」才记忆：`source == "session"` 表示用户在弹窗选了会话级放行。
        // `once`（默认）不记忆——保持逐次批准语义；Reject/Edit/Respond 亦不记忆。
        if let ApprovalDecision::Approve { source } = &decision {
            if source.as_deref() == Some("session") {
                if let Some(memory) = &self.approval_memory {
                    if let Some(fp) = ApprovalMemory::fingerprint(
                        &effective_tool_name(&tool_call.name, &tool_call.input),
                        tool_call
                            .input
                            .get("file_path")
                            .or_else(|| tool_call.input.get("path"))
                            .and_then(|v| v.as_str())
                            .map(std::path::Path::new),
                        cwd,
                    ) {
                        memory.record(fp);
                    }
                }
            }
        }
        apply_decision(tool_call, decision)
    }

    /// 根据共享权限模式决策单个工具调用
    async fn decide_by_mode<S: State>(
        &self,
        state: &S,
        mode: &Arc<SharedPermissionMode>,
        tool_call: &ToolCall,
    ) -> AgentResult<ToolCall> {
        match mode.load() {
            PermissionMode::Bypass => Ok(tool_call.clone()),
            PermissionMode::AutoMode => {
                // 审批记忆：同 `(工具, 路径)` 一经批准即免问（路径级、会话作用域）。
                // 放在语义判定之前短路，规避模型对绝对路径打分的抖动导致的反复弹窗。
                if let Some(memory) = &self.approval_memory {
                    if let Some(fp) = ApprovalMemory::fingerprint(
                        &effective_tool_name(&tool_call.name, &tool_call.input),
                        tool_call
                            .input
                            .get("file_path")
                            .or_else(|| tool_call.input.get("path"))
                            .and_then(|v| v.as_str())
                            .map(std::path::Path::new),
                        std::path::Path::new(state.cwd()),
                    ) {
                        if memory.is_approved(&fp) {
                            tracing::debug!(
                                tool = %tool_call.name,
                                "命中审批记忆，跳过判定直接放行"
                            );
                            return Ok(tool_call.clone());
                        }
                    }
                }
                if let Some(call) = self.build_gate_call(state, tool_call) {
                    if let Some(gate) = &self.jev_gate {
                        // 确定性层**先跑，且不依赖判定凭据**。
                        // 硬黑名单 / 人写的规则 / 只读白名单是零成本、确定性的，
                        // 绝不该因为"没配 key"或"判定服务挂了"就一起失效。
                        gate.ensure_rules_loaded().await;
                        if let Some(decision) = gate.deterministic(&call) {
                            return self
                                .apply_gate_decision(
                                    decision,
                                    tool_call,
                                    std::path::Path::new(state.cwd()),
                                )
                                .await;
                        }
                        // 语义层：只有配置了凭据才发请求；否则落到下面的旧分类器兜底
                        if gate.has_judge() {
                            let decision = gate.evaluate_semantic(&call).await;
                            return self
                                .apply_gate_decision(
                                    decision,
                                    tool_call,
                                    std::path::Path::new(state.cwd()),
                                )
                                .await;
                        }
                        tracing::debug!(
                            tool = %tool_call.name,
                            "未配置语义判定凭据：已跑确定性层，交由兜底分类器"
                        );
                    }
                }
                // 兜底：旧 LLM 分类器
                self.auto_mode_fallback(tool_call, std::path::Path::new(state.cwd()))
                    .await
            }
        }
    }

    /// 把门的判决落到工具调用上（Allow/Block/Ask 的统一处理）。
    async fn apply_gate_decision(
        &self,
        decision: GateDecision,
        tool_call: &ToolCall,
        cwd: &std::path::Path,
    ) -> AgentResult<ToolCall> {
        match decision {
            GateDecision::Allow { rationale } => {
                tracing::debug!(tool = %tool_call.name, %rationale, "Jev 放行");
                Ok(tool_call.clone())
            }
            GateDecision::Block { rationale } => Err(AgentError::ToolRejected {
                tool: tool_call.name.clone(),
                // rationale 已是面向人和 agent 的中文说明（含原因、依据规则、下一步）
                reason: rationale,
            }),
            GateDecision::Ask { rationale } => match &self.broker {
                Some(broker) => self.broker_approve(broker, tool_call, cwd).await,
                None => Err(AgentError::ToolRejected {
                    tool: tool_call.name.clone(),
                    reason: format!("{rationale}\n（当前没有可用的确认通道，因此默认拒绝）"),
                }),
            },
        }
    }

    /// Auto 模式兜底：无 Jev 门或 Jev 不可用时的旧 LLM 分类器路径。
    async fn auto_mode_fallback(
        &self,
        tool_call: &ToolCall,
        cwd: &std::path::Path,
    ) -> AgentResult<ToolCall> {
        match &self.auto_classifier {
            Some(classifier) => {
                let result = classifier.classify(&tool_call.name, &tool_call.input).await;
                match result {
                    Classification::Allow => Ok(tool_call.clone()),
                    Classification::Deny => Err(AgentError::ToolRejected {
                        tool: tool_call.name.clone(),
                        reason: "Auto 模式：分类器拒绝".to_string(),
                    }),
                    Classification::Unsure => match &self.broker {
                        Some(broker) => self.broker_approve(broker, tool_call, cwd).await,
                        None => Err(AgentError::ToolRejected {
                            tool: tool_call.name.clone(),
                            reason: "Auto 模式：分类器不确定且无 broker".to_string(),
                        }),
                    },
                }
            }
            None => match &self.broker {
                Some(broker) => self.broker_approve(broker, tool_call, cwd).await,
                None => Err(AgentError::ToolRejected {
                    tool: tool_call.name.clone(),
                    reason: "Auto 模式：无分类器且无 broker".to_string(),
                }),
            },
        }
    }

    /// 无 mode 时原有的批量 broker 审批逻辑（向后兼容）
    async fn batch_broker_approve(
        &self,
        broker: &Arc<dyn UserInteractionBroker>,
        calls: &[ToolCall],
        start_idx: usize,
        initial_results: &mut Vec<AgentResult<ToolCall>>,
        cwd: &std::path::Path,
    ) -> Vec<AgentResult<ToolCall>> {
        let mut results: Vec<AgentResult<ToolCall>> = std::mem::take(initial_results);

        let needs_approval: Vec<(usize, &ToolCall)> = calls
            .iter()
            .enumerate()
            .skip(start_idx)
            .filter(|(_, c)| (self.requires_approval)(&effective_tool_name(&c.name, &c.input)))
            .collect();

        if needs_approval.is_empty() {
            results.extend(calls.iter().skip(start_idx).map(|c| Ok(c.clone())));
            return results;
        }

        let items: Vec<ApprovalItem> = needs_approval
            .iter()
            .map(|(_, c)| ApprovalItem {
                tool_call_id: c.id.clone(),
                tool_name: c.name.clone(),
                tool_input: c.input.clone(),
            })
            .collect();

        let ctx = InteractionContext::Approval { items };
        let response = broker.request(ctx).await;

        let decisions = match response {
            InteractionResponse::Decisions(d) => d,
            _ => vec![
                ApprovalDecision::Reject {
                    reason: "unexpected response".to_string(),
                    source: None,
                };
                needs_approval.len()
            ],
        };

        let mut decision_iter = decisions.into_iter();

        for call in calls.iter().skip(start_idx) {
            if (self.requires_approval)(&effective_tool_name(&call.name, &call.input)) {
                let decision = decision_iter.next().unwrap_or(ApprovalDecision::Reject {
                    reason: "用户拒绝".to_string(),
                    source: None,
                });
                // 与单次路径一致：仅「本次会话同意」写入审批记忆
                if let ApprovalDecision::Approve { source } = &decision {
                    if source.as_deref() == Some("session") {
                        if let Some(memory) = &self.approval_memory {
                            if let Some(fp) = ApprovalMemory::fingerprint(
                                &effective_tool_name(&call.name, &call.input),
                                call.input
                                    .get("file_path")
                                    .or_else(|| call.input.get("path"))
                                    .and_then(|v| v.as_str())
                                    .map(std::path::Path::new),
                                cwd,
                            ) {
                                memory.record(fp);
                            }
                        }
                    }
                }
                results.push(apply_decision(call, decision));
            } else {
                results.push(Ok(call.clone()));
            }
        }

        results
    }
}

#[async_trait]
impl<S: State> Middleware<S> for HumanInTheLoopMiddleware {
    fn name(&self) -> &str {
        "HumanInTheLoopMiddleware"
    }

    /// 批量工具调用前处理：对一批工具调用一次性收集所有需审批的项，
    /// 通过 broker 弹出一个 [多工具审批] 弹窗，避免逐个弹窗打断用户。
    async fn before_tools_batch(
        &self,
        state: &mut S,
        calls: &[ToolCall],
    ) -> Vec<AgentResult<ToolCall>> {
        self.process_batch(state, calls).await
    }

    async fn before_tool(&self, state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
        // 1. 非敏感工具 → 所有模式都放行
        if !(self.requires_approval)(&effective_tool_name(&tool_call.name, &tool_call.input)) {
            return Ok(tool_call.clone());
        }

        // 2. 有 mode → 按权限模式决策
        if let Some(mode) = &self.mode {
            return self.decide_by_mode(state, mode, tool_call).await;
        }

        // 3. 无 mode 且无 broker → 放行（disabled() 路径）
        let Some(broker) = &self.broker else {
            return Ok(tool_call.clone());
        };

        // 4. 无 mode 但有 broker → 原有弹窗审批逻辑
        self.broker_approve(broker, tool_call, std::path::Path::new(state.cwd()))
            .await
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

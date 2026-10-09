use std::sync::Arc;

use async_trait::async_trait;
use cc_agent::{
    agent::{react::ToolCall, state::AgentState},
    error::{AgentError, AgentResult},
    middleware::r#trait::Middleware,
};

use crate::{
    hitl::HumanInTheLoopMiddleware,
    tool_search::{EXECUTE_EXTRA_TOOL_NAME, EXTRA_TOOL_NAME_FIELD},
};

/// 子 Agent 共享父判定配置，真实工具及代理目标都受定义中的工具限制约束。
pub(super) struct SubAgentPermissionMiddleware {
    hitl: Arc<HumanInTheLoopMiddleware>,
    allowed: Vec<String>,
    disallowed: Vec<String>,
    available: Vec<String>,
    execution_cwd: String,
}

impl SubAgentPermissionMiddleware {
    pub(super) fn new(
        hitl: Arc<HumanInTheLoopMiddleware>,
        allowed: Vec<String>,
        disallowed: Vec<String>,
        available: Vec<String>,
        execution_cwd: String,
    ) -> Self {
        Self {
            hitl,
            allowed,
            disallowed,
            available,
            execution_cwd,
        }
    }

    fn reject(call: &ToolCall, reason: &str) -> AgentError {
        AgentError::ToolRejected {
            tool: call.name.clone(),
            reason: reason.to_string(),
        }
    }

    fn resolve_call(&self, call: &ToolCall) -> AgentResult<ToolCall> {
        let mut resolved = call.clone();
        // executor 会在 before_tool 后解析大小写/别名；这里先钉住真实名称，避免审批看到假工具。
        let outer = self
            .available
            .iter()
            .find(|name| name.as_str() == call.name)
            .or_else(|| {
                self.available
                    .iter()
                    .find(|name| name.eq_ignore_ascii_case(&call.name))
            })
            .ok_or_else(|| Self::reject(call, "工具名称未匹配子 Agent 的实际可用工具"))?;
        resolved.name = outer.clone();
        let target = if outer == EXECUTE_EXTRA_TOOL_NAME {
            let requested = call
                .input
                .get(EXTRA_TOOL_NAME_FIELD)
                .and_then(|value| value.as_str())
                .ok_or_else(|| Self::reject(call, "子 Agent 代理调用缺少真实工具名称"))?;
            // 必须先钉住真实名称，不能让 ExecuteExtraTool 的模糊别名解析在判定后换目标。
            let canonical = self
                .available
                .iter()
                .find(|name| name.as_str() == requested)
                .or_else(|| {
                    self.available
                        .iter()
                        .find(|name| name.eq_ignore_ascii_case(requested))
                })
                .ok_or_else(|| Self::reject(call, "子 Agent 代理目标不在继承的工具集合中"))?;
            resolved.input[EXTRA_TOOL_NAME_FIELD] = canonical.clone().into();
            canonical.as_str()
        } else {
            outer.as_str()
        };
        // Agent 工具既有禁递归约束也覆盖 fork 和代理执行，嵌套代理不得绕过单次解包。
        if target.eq_ignore_ascii_case("Agent")
            || target.eq_ignore_ascii_case(EXECUTE_EXTRA_TOOL_NAME)
        {
            return Err(Self::reject(call, "子 Agent 不允许递归委派或嵌套代理执行"));
        }
        let wildcard = self.allowed.len() == 1 && self.allowed[0] == "*";
        if (!self.allowed.is_empty()
            && !wildcard
            && !self
                .allowed
                .iter()
                .any(|name| name.eq_ignore_ascii_case(target)))
            || self
                .disallowed
                .iter()
                .any(|name| name.eq_ignore_ascii_case(target))
        {
            return Err(Self::reject(
                call,
                "工具不在子 Agent 的允许集合中或已被明确禁止",
            ));
        }
        Ok(resolved)
    }
}

#[async_trait]
impl Middleware<AgentState> for SubAgentPermissionMiddleware {
    fn name(&self) -> &str {
        "SubAgentPermissionMiddleware"
    }

    async fn before_tool(&self, _state: &mut AgentState, call: &ToolCall) -> AgentResult<ToolCall> {
        let resolved = self.resolve_call(call)?;
        // 子 cwd 可以只用于指引；继承 Bash/FS 工具仍固定在父目录执行。
        // 审批事实必须跟随真实执行目录，不改主子状态，也不读取消息历史。
        let mut permission_state = AgentState::new(&self.execution_cwd);
        self.hitl
            .before_tool(&mut permission_state, &resolved)
            .await
    }
}

#[cfg(test)]
#[path = "permission_test.rs"]
mod tests;

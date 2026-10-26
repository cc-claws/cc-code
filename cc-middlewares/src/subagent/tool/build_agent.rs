use std::sync::Arc;

use cc_agent::{
    agent::{react::ReactLLM, state::AgentState, AgentCancellationToken, ReActAgent},
    thread::ThreadMeta,
};

use crate::{
    claude_agent_parser::{ClaudeAgent, ToolsValue},
    subagent::SubAgentMiddlewareConfig,
};

use super::build_subagent_middlewares;

/// 子 Agent 构建参数；后台 Agent 独立取消，Skill fork 随父调用取消。
///
/// **fork 与普通子 Agent 走同一条构建路径**，差异只有 `fork`；建档、中间件链、
/// 持久化、工具过滤全部共用，避免同一件事写两遍后只改一处导致的漂移。
#[derive(Debug, Clone)]
pub(crate) struct AgentBuildOptions {
    /// fork 模式：系统提示不做 agent.md 覆写、工具全量继承主 Agent
    pub fork: bool,
    /// fork：子线程记录「从父线程哪条消息分叉」，供历史追溯
    pub snapshot_at_message_id: Option<String>,
    pub cascade_cancel: bool,
}

/// Concrete builder type for subagents.
type SubReActAgent = ReActAgent<Box<dyn ReactLLM + Send + Sync>, AgentState>;

/// Result of building a subagent from an agent definition.
/// The caller handles execute, result handling, and cleanup.
pub(crate) struct AgentBuildResult {
    pub builder: SubReActAgent,
    pub state: AgentState,
    pub child_thread_id: String,
    pub cancel_token: Option<AgentCancellationToken>,
}

impl super::SubAgentTool {
    /// Build a ReActAgent + AgentState from an agent definition.
    ///
    /// 后台 Agent 与同步 Skill fork 共用；调用方配置事件处理器和执行方式。
    /// `options.fork`: fork 模式（复用主 Agent 系统提示 + 工具全量继承）。
    pub(crate) async fn build_agent_from_def(
        &self,
        agent_def: &ClaudeAgent,
        agent_name: &str,
        cwd: &str,
        options: AgentBuildOptions,
    ) -> Result<AgentBuildResult, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Generate child_thread_id
        let child_thread_id = uuid::Uuid::now_v7().to_string();

        // 2. 后台任务独立取消，Skill fork 随父级取消。
        if let Some(ref store) = self.thread_store {
            let mut child_meta = ThreadMeta::new(cwd);
            child_meta.id = child_thread_id.clone();
            child_meta.parent_thread_id = self.parent_thread_id.clone();
            child_meta.snapshot_at_message_id = options.snapshot_at_message_id.clone();
            child_meta.hidden = true;
            child_meta.cancel_policy = if options.cascade_cancel { "cascade" } else { "independent" }.to_string();
            child_meta.title = Some(agent_name.to_string());
            store
                .create_thread(child_meta)
                .await
                .map_err(|e| format!("Failed to create child thread: {}", e))?;
        }

        // 3. Filter tools（fork 全量继承主 Agent，filter_tools 内部仍剔除 Agent 防递归）
        let mut filtered_tools = if options.fork {
            self.filter_tools(&ToolsValue::Empty, &ToolsValue::Empty)
        } else {
            self.filter_tools(
                &agent_def.frontmatter.tools,
                &agent_def.frontmatter.disallowed_tools,
            )
        };
        // 子代理的激活记录必须独立，不能沿用父工具闭包中的会话 registry。
        // 保留定义工具白名单：父工具集中没有 Skill 时不擅自添加。
        let invoked_skills = cc_agent::agent::compact::re_inject::InvokedSkillRegistry::default();
        let child_skill_tool = crate::tools::SkillTool::new(crate::skills::SkillsMiddleware::resolve_dirs_static(cwd, &self.extra_skill_dirs))
            .with_session_id(Some(child_thread_id.clone()))
            .with_invoked_skills(invoked_skills);
        for tool in &mut filtered_tools {
            if tool.name() == "Skill" {
                *tool = Box::new(child_skill_tool.clone());
            }
        }

        tracing::debug!(
            agent_id = %agent_name,
            parent_count = self.parent_tools.len(),
            filtered_count = filtered_tools.len(),
            filtered_names = ?filtered_tools.iter().map(|t| t.name()).collect::<Vec<_>>(),
            allowed = ?agent_def.frontmatter.tools,
            disallowed = ?agent_def.frontmatter.disallowed_tools,
            "build_agent_from_def: tool filter results"
        );

        // 4. Model alias → LLM factory
        let model_alias: Option<&str> = agent_def
            .frontmatter
            .model
            .as_deref()
            .filter(|m| !m.is_empty() && *m != "inherit");
        let llm = (self.llm_factory)(model_alias);

        // 5. Max iterations
        let raw_turns = agent_def.frontmatter.max_turns.unwrap_or(200);
        let max_iterations = if raw_turns == 0 {
            200
        } else {
            raw_turns as usize
        };

        // 6. Build agent
        let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);

        // 7. Middlewares（fork 不带 agent.md 的 skills，其余链完全一致）
        let mut middleware_config = self.permission_config(
                if options.fork {
                    SubAgentMiddlewareConfig::for_fork(cwd)
                } else {
                    SubAgentMiddlewareConfig::for_agent_def(
                        agent_def.frontmatter.skills.clone(),
                        cwd,
                    )
                }
                .with_inherited_instructions(self.inherited_instructions.clone()),
                if options.fork {
                    &ToolsValue::Empty
                } else {
                    &agent_def.frontmatter.tools
                },
                if options.fork {
                    &ToolsValue::Empty
                } else {
                    &agent_def.frontmatter.disallowed_tools
                },
            );
        middleware_config.skill_tool = Some(child_skill_tool);
        for mw in build_subagent_middlewares(middleware_config) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        // 8. System prompt（fork 续用主 Agent 人格，不做 agent.md 覆写）
        if let Some(ref builder) = self.system_builder {
            let overrides = if options.fork {
                None
            } else {
                Self::overrides_from_agent_def(
                    &agent_def.system_prompt,
                    &agent_def.frontmatter.tone,
                    &agent_def.frontmatter.proactiveness,
                )
            };
            let system_content = builder(overrides.as_ref(), cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // 9. Register tools
        for tool in filtered_tools {
            agent_builder = agent_builder.register_tool(tool);
        }

        // 10. Agent state
        let state = if let Some(ref store) = self.thread_store {
            AgentState::new(cwd.to_string())
                .with_persistence(Arc::clone(store), child_thread_id.clone())
        } else {
            AgentState::new(cwd.to_string())
        }
        .with_context("source_agent_id", child_thread_id.clone());

        // 11. 后台任务独立于父级取消；同步 Skill 的取消必须级联。
        let cancel_token = Some(if options.cascade_cancel {
            self.cancel.as_ref().map(AgentCancellationToken::child_token).unwrap_or_default()
        } else {
            AgentCancellationToken::new()
        });

        Ok(AgentBuildResult {
            builder: agent_builder,
            state,
            child_thread_id,
            cancel_token,
        })
    }
}

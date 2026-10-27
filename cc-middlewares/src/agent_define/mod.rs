use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cc_agent::{agent::state::State, error::AgentResult, middleware::r#trait::Middleware};

use crate::parse_agent_file;

/// agent.md 中可覆盖 system prompt 的部分
///
/// 所有字段均为 `Option`，`None` 表示使用默认值。
#[derive(Debug, Clone, Default)]
pub struct AgentOverrides {
    /// 角色定位（替换 `{{persona}}`）
    pub persona: Option<String>,
    /// 输出风格（替换 `{{tone_and_style}}`）
    pub tone: Option<String>,
    /// 主动性（替换 `{{proactiveness}}`）
    pub proactiveness: Option<String>,
}

impl AgentOverrides {
    pub fn is_empty(&self) -> bool {
        self.persona.is_none() && self.tone.is_none() && self.proactiveness.is_none()
    }
}

/// AgentDefineMiddleware - 根据 agent_id 注入 CC Agent 定义文件
///
/// Agent 定义文件搜索路径（按优先级）：
/// 1. `{cwd}/.claude/agents/{agent_id}/agent.md`
/// 2. `{cwd}/.claude/agents/{agent_id}.md`
/// 3. `{cwd}/agents/{agent_id}/agent.md`
/// 4. `{cwd}/agents/{agent_id}.md`
/// 5. `{home}/.claude/agents/{agent_id}/agent.md`（用户级，见 #759）
/// 6. `{home}/.claude/agents/{agent_id}.md`（用户级，见 #759）
///
/// 插件目录不在此列：调用侧（`SubAgentTool::load_agent_def`）在上述路径
/// 之后再查 `extra_agent_dirs`，最终优先级为：项目级 > 用户级 > 插件 > 内置。
///
/// Agent 定义文件格式（CC YAML frontmatter）：
/// ```markdown
/// ---
/// name: code-reviewer
/// description: Reviews code for quality and best practices
/// tools: Read, Glob, Grep
/// tone: |
///   Be thorough and explain your reasoning in detail.
/// proactiveness: |
///   Proactively review related files and suggest improvements.
/// ---
///
/// You are a code reviewer. Focus on code quality and best practices.
/// ```
pub struct AgentDefineMiddleware;

impl AgentDefineMiddleware {
    pub fn new() -> Self {
        Self
    }

    /// agent_id 安全检查：空、含路径分隔符或 `..` 一律视为不安全，
    /// 调用侧在查插件目录前同样用它把关，防止路径遍历。
    pub fn is_safe_agent_id(agent_id: &str) -> bool {
        !agent_id.is_empty()
            && !agent_id.contains('/')
            && !agent_id.contains('\\')
            && !agent_id.contains("..")
    }

    /// 根据 cwd 和 agent_id 构建候选路径列表
    ///
    /// 如果 agent_id 包含路径分隔符或 `..`，返回空列表以防止路径遍历。
    pub fn candidate_paths(cwd: &str, agent_id: &str) -> Vec<PathBuf> {
        if !Self::is_safe_agent_id(agent_id) {
            return Vec::new();
        }
        let cwd = Path::new(cwd);
        let mut paths = vec![
            cwd.join(".claude")
                .join("agents")
                .join(agent_id)
                .join("agent.md"),
            cwd.join(".claude")
                .join("agents")
                .join(format!("{}.md", agent_id)),
            cwd.join("agents").join(agent_id).join("agent.md"),
            cwd.join("agents").join(format!("{}.md", agent_id)),
        ];
        // 用户级：~/.claude/agents（项目级优先，用户级次之）
        if let Some(home) = dirs_next::home_dir() {
            let user_agents = home.join(".claude").join("agents");
            paths.push(user_agents.join(agent_id).join("agent.md"));
            paths.push(user_agents.join(format!("{}.md", agent_id)));
        }
        paths
    }

    /// 按优先级找到第一个存在的文件
    fn find_file(cwd: &str, agent_id: &str) -> Option<PathBuf> {
        Self::candidate_paths(cwd, agent_id)
            .into_iter()
            .find(|p| p.is_file())
    }

    /// 同步读取 agent.md，返回可覆盖 system prompt 的各个部分。
    ///
    /// 供 TUI 层在构建 LLM 前提前获取覆盖内容，传入 `build_system_prompt`。
    /// 返回 `None` 表示文件不存在或无有效内容。
    pub fn load_overrides(cwd: &str, agent_id: &str) -> Option<AgentOverrides> {
        let path = Self::find_file(cwd, agent_id)?;
        let content = std::fs::read_to_string(&path).ok()?;
        if content.trim().is_empty() {
            return None;
        }

        if let Some(agent) = parse_agent_file(&content) {
            let persona = if agent.system_prompt.is_empty() {
                None
            } else {
                Some(agent.system_prompt)
            };
            let overrides = AgentOverrides {
                persona,
                tone: agent.frontmatter.tone,
                proactiveness: agent.frontmatter.proactiveness,
            };
            if overrides.is_empty() {
                return None;
            }
            return Some(overrides);
        }

        // 没有有效 frontmatter，把整个文件内容当作 persona
        let text = content.trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(AgentOverrides {
                persona: Some(text),
                ..Default::default()
            })
        }
    }
}

impl Default for AgentDefineMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for AgentDefineMiddleware {
    fn name(&self) -> &str {
        "AgentDefineMiddleware"
    }

    async fn before_agent(&self, _state: &mut S) -> AgentResult<()> {
        // 覆盖注入已在构建 LLM 时通过 build_system_prompt(overrides, cwd) 完成，
        // 中间件层无需再操作消息列表。
        Ok(())
    }
}

#[cfg(test)]
#[path = "agent_define_test.rs"]
mod tests;

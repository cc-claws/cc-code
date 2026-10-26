//! 一级 Skill 工具。ToolContent 承载指令，保持标准 Function Calling 消息配对。

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use cc_agent::{agent::compact::re_inject::InvokedSkillRegistry, tools::BaseTool};
use serde::{Deserialize, Serialize};

use crate::{
    skills::invocation::{load_skill, SkillError},
    subagent::SubAgentTool,
    tool_search::core_tools::TOOL_SKILL,
};

#[derive(Clone)]
pub struct SkillTool {
    dirs: Vec<PathBuf>,
    fork_tool: Option<Arc<SubAgentTool>>,
    session_id: Option<String>,
    invocation_lock: Arc<tokio::sync::Mutex<()>>,
    invoked_skills: InvokedSkillRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillToolInput {
    pub skill: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

/// 对齐 CC 的 camelCase 输出；instruction 为 Peri ToolContent 接口的指令载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillToolOutput {
    pub success: bool,
    pub command_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

impl SkillTool {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self {
            dirs,
            fork_tool: None,
            session_id: None,
            invocation_lock: Arc::new(tokio::sync::Mutex::new(())),
            invoked_skills: InvokedSkillRegistry::default(),
        }
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    pub fn with_fork_tool(mut self, tool: SubAgentTool) -> Self {
        self.fork_tool = Some(Arc::new(tool));
        self
    }

    pub fn with_session_id(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_invoked_skills(mut self, invoked_skills: InvokedSkillRegistry) -> Self {
        self.invoked_skills = invoked_skills;
        self
    }

    /// 用户显式 Slash 可以加载 disable-model-invocation 技能。
    pub async fn invoke_for_user(&self, input: SkillToolInput) -> Result<SkillToolOutput, SkillError> {
        self.execute(input, true).await
    }

    async fn execute(&self, input: SkillToolInput, user_invoked: bool) -> Result<SkillToolOutput, SkillError> {
        let _invocation_guard = self.invocation_lock.lock().await;
        let dirs = self.dirs.clone();
        let request = input.clone();
        let mut skill = tokio::task::spawn_blocking(move || {
            load_skill(&dirs, &request.skill, request.args.as_deref())
        }).await.map_err(|error| format!("Failed to load skill: {error}"))??;
        if !user_invoked && skill.frontmatter.disable_model_invocation {
            return Err(format!("Skill {} cannot be used with the Skill tool due to disable-model-invocation", skill.name).into());
        }
        if user_invoked && !skill.frontmatter.user_invocable {
            return Err(format!("Skill {} is not user-invocable", skill.name).into());
        }
        if let Some(session_id) = &self.session_id {
            skill.instruction = skill.instruction.replace("${CLAUDE_SESSION_ID}", session_id);
        }
        let allowed_tools = skill.frontmatter.allowed_tools.as_ref()
            .map(|tools| tools.to_vec()).filter(|tools| !tools.is_empty());
        let model = skill.frontmatter.model.clone().filter(|model| !model.is_empty() && model != "inherit");
        let mut output = SkillToolOutput {
            success: true,
            command_name: skill.name,
            status: "inline".to_string(),
            allowed_tools,
            model,
            instruction: Some(skill.instruction),
            agent_id: None,
            result: None,
            args: input.args,
        };
        if skill.frontmatter.context.as_deref() == Some("fork") {
            let runner = self.fork_tool.as_ref().ok_or("Forked skill execution is not available in this agent; delegate this skill to the parent agent")?;
            let (agent_id, result) = runner.invoke_skill(
                &output.command_name,
                output.instruction.as_deref().unwrap_or_default(),
                skill.frontmatter.agent.as_deref(),
                output.model.as_deref(),
            ).await?;
            output.status = "forked".to_string();
            output.instruction = None;
            output.agent_id = Some(agent_id);
            output.result = Some(result);
        }
        self.invoked_skills.record(&serde_json::to_value(&output)?);
        Ok(output)
    }
}

#[async_trait]
impl BaseTool for SkillTool {
    fn name(&self) -> &str {
        TOOL_SKILL
    }

    fn description(&self) -> &str {
        "Execute a skill within the main conversation. When a skill matches the user's request, this is a BLOCKING REQUIREMENT: invoke Skill BEFORE generating any other response about the task. Use the exact available skill name and optional args. NEVER read SKILL.md directly with Read or claim to use a skill without invoking it. Do not invoke a skill already loaded in the current turn or use this tool for built-in CLI commands."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "REQUIRED. The skill name, e.g. commit, review-pr, or ms-office-suite:pdf."
                },
                "args": {
                    "type": "string",
                    "description": "Optional arguments for the skill."
                }
            },
            "required": ["skill"]
        })
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<String, SkillError> {
        let input: SkillToolInput = serde_json::from_value(input)
            .map_err(|error| format!("Invalid Skill input: {error}"))?;
        let output = self.execute(input, false).await?;
        Ok(serde_json::to_string(&output)?)
    }
}

#[cfg(test)]
#[path = "skill_test.rs"]
mod tests;

#[cfg(test)]
#[path = "skill_integration_test.rs"]
mod integration_tests;

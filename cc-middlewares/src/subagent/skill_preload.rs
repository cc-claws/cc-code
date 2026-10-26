use std::collections::HashSet;

use async_trait::async_trait;
use cc_agent::{
    agent::{
        compact::re_inject::{skill_invocation_message, skill_invocation_payload},
        state::State,
    },
    error::{AgentError, AgentResult},
    messages::BaseMessage,
    middleware::r#trait::Middleware,
};

use crate::{
    skills::{invocation::find_skill, list_skills, SkillMetadata, SkillsMiddleware},
    tools::skill::{SkillTool, SkillToolInput},
};

fn skill_name_from_token(token: &str) -> Option<&str> {
    let name = token.strip_prefix('/')?;
    (!name.is_empty()
        && name.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '-' | '_' | ':' | '.')
        }))
    .then_some(name)
}

/// 保留原始参数的引号、内部空白与 Unicode；下一个已知技能 token 是参数边界。
fn extract_skill_requests(text: &str, skills: &[SkillMetadata]) -> Vec<SkillToolInput> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            start.get_or_insert(index);
            continue;
        }
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            start.get_or_insert(index);
            continue;
        }
        if character.is_whitespace() {
            if let Some(token_start) = start.take() {
                tokens.push((token_start, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        tokens.push((token_start, text.len()));
    }
    let matches: Vec<_> = tokens
        .into_iter()
        .filter_map(|(start, end)| {
            let name = skill_name_from_token(&text[start..end])?;
            let skill = find_skill(skills, name)?;
            Some((start, end, skill.name.clone()))
        })
        .collect();
    matches
        .iter()
        .enumerate()
        .map(|(index, (_, end, name))| {
            let args_end = matches
                .get(index + 1)
                .map_or(text.len(), |(start, _, _)| *start);
            let args = text[*end..args_end].trim();
            SkillToolInput {
                skill: name.clone(),
                args: (!args.is_empty()).then(|| args.to_string()),
            }
        })
        .collect()
}

/// 显式 Slash 与 SubAgent 技能预加载。
///
/// 与 Skill 工具共用查找、frontmatter、参数插值和 fork 执行，加载结果仅作为
/// Human 元消息追加到用户输入之后。不会伪造缺少 thinking 的 Ai/Tool 消息，
/// 不改变系统提示词；TUI 根据 display 字段投影 Skill 卡片。
pub struct SkillPreloadMiddleware {
    skill_names: Vec<String>,
    skill_tool: SkillTool,
}

impl SkillPreloadMiddleware {
    pub fn new(skill_names: Vec<String>, cwd: &str) -> Self {
        Self {
            skill_names,
            skill_tool: SkillTool::new(SkillsMiddleware::resolve_dirs_static(cwd, &[])),
        }
    }

    /// 共享主 Agent 的技能目录、权限及 fork 执行上下文。
    pub fn with_skill_tool(mut self, skill_tool: SkillTool) -> Self {
        self.skill_tool = skill_tool;
        self
    }

    fn error(reason: impl Into<String>) -> AgentError {
        AgentError::MiddlewareError {
            middleware: "SkillPreloadMiddleware".to_string(),
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl<S: State> Middleware<S> for SkillPreloadMiddleware {
    fn name(&self) -> &str {
        "SkillPreloadMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 不把已经加载的技能正文当作新的用户输入，防止正文中的 /token 递归触发。
        let last_input = state
            .messages()
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, message)| match message {
                BaseMessage::Human { content, .. } => {
                    let text = content.text_content();
                    skill_invocation_payload(&text)
                        .is_none()
                        .then_some((index, text))
                }
                _ => None,
            });
        if self.skill_names.is_empty() && last_input.is_none() {
            return Ok(());
        }
        let dirs = self.skill_tool.dirs().to_vec();
        let skills = tokio::task::spawn_blocking(move || list_skills(&dirs))
            .await
            .map_err(|error| Self::error(format!("扫描技能失败: {error}")))?;
        let requests = if self.skill_names.is_empty() {
            extract_skill_requests(
                last_input.as_ref().map_or("", |(_, text)| text.as_str()),
                &skills,
            )
        } else {
            self.skill_names
                .iter()
                .filter_map(|name| {
                    let skill = find_skill(&skills, name.strip_prefix('/').unwrap_or(name))?;
                    Some(SkillToolInput {
                        skill: skill.name.clone(),
                        args: None,
                    })
                })
                .collect()
        };
        // 同一轮重复 before_agent 不重复加载；新的用户指令仍可显式重新执行技能。
        let loaded_start = last_input.map_or(0, |(index, _)| index + 1);
        let mut loaded: HashSet<(String, Option<String>)> = state.messages()[loaded_start..]
            .iter()
            .filter_map(|message| {
                let BaseMessage::Human { content, .. } = message else {
                    return None;
                };
                let text = content.text_content();
                let output: serde_json::Value =
                    serde_json::from_str(skill_invocation_payload(&text)?).ok()?;
                if output.get("success").and_then(serde_json::Value::as_bool) != Some(true) {
                    return None;
                }
                Some((
                    output.get("commandName")?.as_str()?.to_string(),
                    output
                        .get("args")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                ))
            })
            .collect();
        let mut messages = Vec::new();
        for request in requests {
            if !loaded.insert((request.skill.clone(), request.args.clone())) {
                continue;
            }
            let args = request.args.clone();
            let output = self
                .skill_tool
                .invoke_for_user(request)
                .await
                .map_err(|error| Self::error(format!("加载技能失败: {error}")))?;
            let mut payload = serde_json::to_value(output)
                .map_err(|error| Self::error(format!("序列化技能结果失败: {error}")))?;
            if payload.get("success").and_then(serde_json::Value::as_bool) != Some(true) {
                return Err(Self::error("技能未成功加载"));
            }
            payload["display"] = serde_json::json!(true);
            if let Some(args) = args {
                payload["args"] = serde_json::json!(args);
            }
            messages.push(skill_invocation_message(&payload));
        }
        // 全部加载成功后再修改 state，失败时保留原始输入，且不会产生孤儿工具消息。
        for message in messages {
            state.add_message(message);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "skill_preload_test.rs"]
mod tests;

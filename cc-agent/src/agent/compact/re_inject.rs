use crate::{
    agent::{compact::config::CompactConfig, events::CompactFileInfo},
    messages::BaseMessage,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, warn};

/// 技能正文通过 Human 元消息进入模型上下文，不修改会话冻结的系统提示词。
pub const SKILL_INVOCATION_PREFIX: &str = "<skill-invocation>\n";
pub const SKILL_INVOCATION_SUFFIX: &str = "\n</skill-invocation>";
const SKILL_TRUNCATION_MARKER: &str = "\n\n[... skill content truncated for compaction; use Read on the skill path if you need the full text]";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillInvocation {
    success: bool,
    command_name: String,
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    instruction: Option<String>,
}

#[derive(Debug, Default)]
struct InvokedSkills {
    initialized: bool,
    // 保持调用时间顺序，同名调用会移到末尾。
    skills: Vec<SkillInvocation>,
}

/// 当前 agent 会话的完整技能指令，与可被压缩的消息历史分开保存。
///
/// Clone 只用于同一 agent 的工具、Slash 与 compact 共享；创建子 agent 时
/// 必须使用新的 default 实例，不可继承父 agent 的 registry。
#[derive(Debug, Clone, Default)]
pub struct InvokedSkillRegistry {
    inner: Arc<parking_lot::Mutex<InvokedSkills>>,
}

impl InvokedSkillRegistry {
    /// 仅记录实际成功的内联加载，forked 技能属于独立子 agent。
    /// compact 的短载荷永远不能覆盖本 registry 保存的完整正文。
    pub fn record(&self, output: &serde_json::Value) -> bool {
        if output.get("restored").and_then(serde_json::Value::as_bool) == Some(true) {
            return false;
        }
        let Ok(invocation) = serde_json::from_value::<SkillInvocation>(output.clone()) else {
            return false;
        };
        if !is_inline_skill(&invocation) {
            return false;
        }
        let mut inner = self.inner.lock();
        inner.initialized = true;
        inner
            .skills
            .retain(|skill| skill.command_name != invocation.command_name);
        inner.skills.push(invocation);
        true
    }

    /// 每个 registry 仅从自己的初始历史重建一次；空历史也标记为已初始化，
    /// 防止下一轮重复扫描 ancestor 或 compact 摘要并污染 agent 隔离。
    pub fn seed_from_messages(&self, messages: &[BaseMessage]) {
        let mut inner = self.inner.lock();
        if inner.initialized {
            return;
        }
        inner.skills = extract_active_skills(messages);
        inner.initialized = true;
    }

    /// clear/rewind 显式重建；不在模型切换或普通 compact 时调用。
    pub fn reset_from_messages(&self, messages: &[BaseMessage]) {
        let skills = extract_active_skills(messages);
        let mut inner = self.inner.lock();
        inner.skills = skills;
        inner.initialized = true;
    }

    fn snapshot(&self) -> Vec<SkillInvocation> {
        self.inner.lock().skills.clone()
    }
}

/// Slash 加载与 compact 恢复共用的 Human 元消息格式，不伪造模型工具调用。
pub fn format_skill_invocation(output: &serde_json::Value) -> String {
    format!("{SKILL_INVOCATION_PREFIX}{output}{SKILL_INVOCATION_SUFFIX}")
}

pub fn skill_invocation_message(output: &serde_json::Value) -> BaseMessage {
    BaseMessage::human(format_skill_invocation(output))
}

/// 只接受完整的技能元消息；载荷的 JSON 字段校验由调用方负责。
pub fn skill_invocation_payload(text: &str) -> Option<&str> {
    text.strip_prefix(SKILL_INVOCATION_PREFIX)?
        .strip_suffix(SKILL_INVOCATION_SUFFIX)
}

#[derive(Debug, Clone)]
pub struct ReInjectResult {
    pub messages: Vec<BaseMessage>,
    pub files_injected: usize,
    pub skills_injected: usize,
}

/// 从消息历史中提取最近通过 Read 工具读取的文件路径（去重，保留最新）
fn extract_recent_files(messages: &[BaseMessage], max_files: usize) -> Vec<String> {
    if max_files == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::<String>::new();
    let mut paths = Vec::new();

    for msg in messages.iter().rev() {
        for tc in msg.tool_calls() {
            if tc.name == "Read" {
                if let Some(path) = tc.arguments.get("path").and_then(|v| v.as_str()) {
                    if seen.insert(path.to_string()) {
                        paths.push(path.to_string());
                        if paths.len() >= max_files {
                            return paths;
                        }
                    }
                }
            }
        }
    }

    paths
}

fn parse_skill_invocation(text: &str) -> Option<SkillInvocation> {
    let invocation: SkillInvocation = serde_json::from_str(text).ok()?;
    (invocation.success && !invocation.command_name.trim().is_empty()).then_some(invocation)
}

fn is_inline_skill(invocation: &SkillInvocation) -> bool {
    invocation.success
        && !invocation.command_name.trim().is_empty()
        && invocation.status == "inline"
        && invocation
            .instruction
            .as_ref()
            .is_some_and(|instruction| !instruction.trim().is_empty())
}

fn parse_skill_message(text: &str) -> Option<SkillInvocation> {
    parse_skill_invocation(skill_invocation_payload(text)?)
}

/// 恢复当前消息历史中成功加载到主上下文的技能，同名保留最近一次调用。
///
/// 按真实 Skill 调用与结果的 ID 配对，不从路径猜测技能，也不重新读取磁盘。
/// compact 恢复的 Human 元消息沿用相同协议，因此连续 compact 仍能保留正文。
fn extract_active_skills(messages: &[BaseMessage]) -> Vec<SkillInvocation> {
    let mut pending = HashSet::new();
    let mut skills: Vec<SkillInvocation> = Vec::new();
    for message in messages {
        for call in message.tool_calls() {
            if call.name == "Skill" {
                pending.insert(call.id.as_str());
            }
        }
        let invocation = match message {
            BaseMessage::Tool {
                tool_call_id,
                content,
                is_error,
                ..
            } => {
                if !pending.remove(tool_call_id.as_str()) {
                    continue;
                }
                if *is_error {
                    continue;
                }
                parse_skill_invocation(&content.text_content())
            }
            BaseMessage::Human { content, .. } => parse_skill_message(&content.text_content()),
            _ => None,
        };
        if let Some(invocation) = invocation {
            if is_inline_skill(&invocation) {
                skills.retain(|skill| skill.command_name != invocation.command_name);
                skills.push(invocation);
            }
        }
    }
    skills
}

/// 异步读取文件并截断到指定 token 预算（字符数 / 4 估算）
async fn read_file_with_budget(path: &str, max_tokens: u32) -> Option<String> {
    let path_owned = path.to_string();
    let content = tokio::task::spawn_blocking(move || std::fs::read_to_string(&path_owned))
        .await
        .ok()?
        .ok()?;

    let max_chars = max_tokens as usize * 4;
    if content.chars().count() > max_chars {
        let truncated: String = content.chars().take(max_chars).collect();
        debug!(path, max_tokens, "文件内容截断到 {} 字符", max_chars);
        Some(format!("{}...(已截断)", truncated))
    } else {
        Some(content)
    }
}

/// 按总 token 预算截断内容列表，返回保留的条目数
fn truncate_to_budget(contents: &mut Vec<(String, String)>, budget: u32) -> usize {
    let budget_chars = budget as usize * 4;
    let mut used_chars = 0;
    let mut keep_count = 0;

    for (_, content) in contents.iter() {
        let chars = content.chars().count();
        if used_chars + chars > budget_chars {
            break;
        }
        used_chars += chars;
        keep_count += 1;
    }

    contents.truncate(keep_count);
    keep_count
}

/// 执行重新注入：重新读取最近的代码文件，恢复当前 agent 已调用的技能正文。
/// 技能使用 Human 元消息保存，避免改变会话的系统提示词。
pub async fn re_inject(
    messages: &[BaseMessage],
    config: &CompactConfig,
    cwd: &str,
) -> ReInjectResult {
    let registry = InvokedSkillRegistry::default();
    re_inject_with_registry(messages, config, cwd, &registry).await
}

/// 使用 agent 会话的独立 registry 恢复技能；只截断快照，不修改完整指令。
pub async fn re_inject_with_registry(
    messages: &[BaseMessage],
    config: &CompactConfig,
    cwd: &str,
    registry: &InvokedSkillRegistry,
) -> ReInjectResult {
    registry.seed_from_messages(messages);
    let mut result_messages: Vec<BaseMessage> = Vec::new();

    // 1. 提取并注入最近读取的文件
    let file_paths = extract_recent_files(messages, config.re_inject_max_files);
    let mut files_injected = 0;

    if !file_paths.is_empty() {
        let resolved_paths: Vec<String> = file_paths
            .iter()
            .map(|p| {
                if Path::new(p).is_absolute() {
                    p.clone()
                } else {
                    let abs = Path::new(cwd).join(p);
                    abs.to_string_lossy().to_string()
                }
            })
            .collect();

        let mut file_futures = Vec::new();
        for path in &resolved_paths {
            file_futures.push(read_file_with_budget(
                path,
                config.re_inject_max_tokens_per_file,
            ));
        }
        let file_contents: Vec<Option<String>> = futures::future::join_all(file_futures).await;

        let mut valid_files: Vec<(String, String)> = Vec::new();
        for (path, content) in file_paths.iter().zip(file_contents) {
            if let Some(content) = content {
                valid_files.push((path.clone(), content));
            } else {
                debug!(path, "文件读取失败或不存在，跳过重新注入");
            }
        }

        truncate_to_budget(&mut valid_files, config.re_inject_file_budget);

        for (path, content) in &valid_files {
            let system_content = format!("[最近读取的文件: {}]\n{}", path, content);
            result_messages.push(BaseMessage::system(system_content));
        }
        files_injected = valid_files.len();
    }

    // 2. 对齐 参考实现：最近调用优先，逐技能截断，再按技能总预算筛选。
    let active_skills = registry.snapshot();
    let mut skills_injected = 0;
    let mut skill_chars = 0usize;
    let skills_budget = (config.re_inject_skills_budget as usize).saturating_mul(4);
    let max_skill_chars = (config.re_inject_max_tokens_per_file as usize).saturating_mul(4);
    let mut restored_skills = Vec::new();
    for mut invocation in active_skills.into_iter().rev() {
        let Some(instruction) = invocation.instruction.as_mut() else {
            continue;
        };
        if instruction.chars().count() > max_skill_chars {
            let marker_chars = SKILL_TRUNCATION_MARKER.chars().count();
            // 极小的自定义预算仍保持有效 UTF-8，不产生下标溢出。
            if max_skill_chars > marker_chars {
                *instruction = format!(
                    "{}{}",
                    instruction
                        .chars()
                        .take(max_skill_chars - marker_chars)
                        .collect::<String>(),
                    SKILL_TRUNCATION_MARKER
                );
            } else {
                *instruction = instruction.chars().take(max_skill_chars).collect();
            }
            debug!(
                skill = invocation.command_name,
                "技能指令截断到 compact 单项预算"
            );
        }
        let chars = instruction.chars().count();
        if chars == 0 {
            continue;
        }
        if skill_chars.saturating_add(chars) > skills_budget {
            warn!(
                skill = invocation.command_name,
                "技能指令超过 compact 总预算，跳过较早技能"
            );
            continue;
        }
        let Ok(mut output) = serde_json::to_value(&invocation) else {
            continue;
        };
        output["restored"] = serde_json::json!(true);
        skill_chars += chars;
        restored_skills.push(skill_invocation_message(&output));
        skills_injected += 1;
    }
    // 保留调用时间顺序，使连续 compact 不会反转技能的新旧优先级。
    result_messages.extend(restored_skills.into_iter().rev());

    debug!(
        files_injected,
        skills_injected,
        total_messages = result_messages.len(),
        "重新注入完成"
    );

    ReInjectResult {
        messages: result_messages,
        files_injected,
        skills_injected,
    }
}

/// Extract file info from re_inject messages (e.g., "[最近读取的文件: ...")
pub fn extract_file_info(messages: &[BaseMessage]) -> Vec<CompactFileInfo> {
    let mut files = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(rest) = content.strip_prefix("[最近读取的文件: ") {
            let path = rest.lines().next().unwrap_or("");
            let line_count = rest.lines().count().saturating_sub(1);
            if !path.is_empty() {
                files.push(CompactFileInfo {
                    path: path.to_string(),
                    lines: line_count,
                });
            }
        }
    }
    files
}

/// Extract skill names from re_inject messages (e.g., "[激活的 Skill 指令: ...")
pub fn extract_skill_names(messages: &[BaseMessage]) -> Vec<String> {
    let mut skills = Vec::new();
    for msg in messages {
        let content = msg.content();
        if let Some(invocation) = parse_skill_message(&content) {
            skills.push(invocation.command_name);
            continue;
        }
        if let Some(rest) = content.strip_prefix("[激活的 Skill 指令: ") {
            let name = rest.lines().next().unwrap_or("");
            if !name.is_empty() {
                skills.push(name.to_string());
            }
        }
    }
    skills
}

/// 构造 compact 后的消息列表：摘要 Human 消息 + re_inject 注入消息。
///
/// 摘要正文必须用 `<system-reminder>` 包裹——TUI 依赖该标签把摘要折叠成一行
/// 提示（见 `cc-tui/src/ui/message_view/mod.rs`）。手动 `/compact`（`cc-acp`）
/// 与自动 compact（`cc-middlewares`）**必须共用此函数**，否则两条路径会再次漂移：
/// 手动路径缺少标签时，摘要会被当作普通用户消息整段铺开。
pub fn build_compacted_messages(summary: &str, re_injected: Vec<BaseMessage>) -> Vec<BaseMessage> {
    let summary_content = format!(
        "<system-reminder>\n{summary}\n\n[Context compacted; continue based on the summary]\n</system-reminder>"
    );
    let mut messages = vec![BaseMessage::human(summary_content)];
    messages.extend(re_injected);
    messages
}

#[cfg(test)]
#[path = "re_inject_test.rs"]
mod tests;

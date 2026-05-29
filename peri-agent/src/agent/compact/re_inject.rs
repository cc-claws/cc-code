use crate::{agent::compact::config::CompactConfig, messages::BaseMessage};
use std::path::Path;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct ReInjectResult {
    pub messages: Vec<BaseMessage>,
    pub files_injected: usize,
    pub skills_injected: usize,
}

/// 判断路径是否为 Skills 目录下的 SKILL.md 文件
fn is_skills_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.contains("/.claude/skills/")
        || (normalized.contains("/skills/") && normalized.ends_with("SKILL.md"))
}

/// 从消息历史中提取最近通过 Read 工具读取的文件路径（去重，保留最新）
fn extract_recent_files(messages: &[BaseMessage], max_files: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut paths = Vec::new();

    for msg in messages.iter().rev() {
        for tc in msg.tool_calls() {
            if tc.name == "Read" {
                if let Some(path) = tc.arguments.get("path").and_then(|v| v.as_str()) {
                    if is_skills_path(path) {
                        continue;
                    }
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

/// 从消息历史中提取 SkillPreloadMiddleware 注入的 Skills 路径（去重，保留出现顺序）
///
/// 支持三种来源：
/// - Ai 消息的 tool_calls（旧格式：fake Human/Ai/Tool 序列）
/// - System 消息中的 `[Skill: path]` 标记（中间格式）
/// - Human 消息中的 `[Skill: path]` 标记（当前格式）
fn extract_skills_paths(messages: &[BaseMessage]) -> Vec<String> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut paths = Vec::new();

    for msg in messages.iter() {
        // 路径 1：扫描 Ai 消息的 tool_calls（旧格式兼容）
        for tc in msg.tool_calls() {
            if tc.name == "Read" {
                if let Some(path) = tc.arguments.get("path").and_then(|v| v.as_str()) {
                    if is_skills_path(path) && seen.insert(path.to_string()) {
                        paths.push(path.to_string());
                    }
                }
            }
        }

        // 路径 2：扫描 System/Human 消息中的 [Skill: path] 标记
        let text = match msg {
            BaseMessage::System { content, .. } | BaseMessage::Human { content, .. } => {
                content.text_content()
            }
            _ => continue,
        };
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("[Skill: ") {
                if let Some(path) = rest.strip_suffix(']') {
                    let trimmed = path.trim();
                    if is_skills_path(trimmed) && seen.insert(trimmed.to_string()) {
                        paths.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    paths
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

/// 执行重新注入：从压缩前消息中提取文件路径和 Skills 路径，
/// 异步读取内容，以 System 消息形式返回注入列表
pub async fn re_inject(
    messages: &[BaseMessage],
    config: &CompactConfig,
    cwd: &str,
) -> ReInjectResult {
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

    // 2. 提取并注入激活的 Skills
    let skills_paths = extract_skills_paths(messages);
    let mut skills_injected = 0;

    if !skills_paths.is_empty() {
        let resolved_skill_paths: Vec<String> = skills_paths
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

        let mut skill_futures = Vec::new();
        for path in &resolved_skill_paths {
            skill_futures.push(read_file_with_budget(
                path,
                config.re_inject_max_tokens_per_file,
            ));
        }
        let skill_contents: Vec<Option<String>> = futures::future::join_all(skill_futures).await;

        let mut valid_skills: Vec<(String, String)> = Vec::new();
        for (path, content) in skills_paths.iter().zip(skill_contents) {
            if let Some(content) = content {
                valid_skills.push((path.clone(), content));
            } else {
                warn!(path, "Skill 文件读取失败，跳过重新注入");
            }
        }

        truncate_to_budget(&mut valid_skills, config.re_inject_skills_budget);

        for (path, content) in &valid_skills {
            let system_content = format!("[激活的 Skill 指令: {}]\n{}", path, content);
            result_messages.push(BaseMessage::system(system_content));
        }
        skills_injected = valid_skills.len();
    }

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

#[cfg(test)]
#[path = "re_inject_test.rs"]
mod tests;

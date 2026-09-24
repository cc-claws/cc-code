//! `/recap` 命令 — 生成当前会话的一句话回顾。
//!
//! 移植自 Claude Code 的 away-summary 实现（参考 `claude-code-best/claude-code`
//! 的 `src/commands/recap/generateRecap.ts`），核心语义对齐：
//! - 单轮 fork，禁用工具调用
//! - 输出 ≤60 字（中文）/ ≤40 字（英文）plain text，无 markdown
//! - 结构：高层目标 + 当前任务 → 下一步行动
//! - 判别联合返回（Ok / ApiError / NoTurn / Aborted / Failed）
//! - 不写 history（对应 CCB 的 skipTranscript）
//!
//! 与 CCB 的差异：CCB 通过 CacheSafeParams 共享主循环 prompt cache 前缀，
//! peri 的 slash 命令拦截点在 agent 构建前，暂不共享 cache。为降低成本，
//! 这里把历史消息压缩为纯文本摘要格式（跳过 System、截断长文本、忽略工具结果），
//! 使 input token 数远小于完整 history。

use crate::{
    agent::AgentCancellationToken,
    llm::{types::LlmRequest, BaseModel},
    messages::{BaseMessage, ContentBlock},
};
use tracing::warn;

/// recap system prompt
const SYSTEM_PROMPT: &str = "你是一个会话回顾工具，负责生成简短的会话进展摘要。";

/// 中文 recap prompt（对应 CCB RECAP_PROMPT_ZH）
const RECAP_PROMPT_ZH: &str = "用户离开后回来了。用中文写 1-2 句话，不超过 60 字，无 markdown。先说明高层目标和当前任务，再说明下一步操作。跳过根因分析和次要待办。";

/// 英文 recap prompt（对应 CCB RECAP_PROMPT_EN），预留多语言切换
#[allow(dead_code)]
const RECAP_PROMPT_EN: &str = "The user stepped away and is coming back. Recap in under 40 words, 1-2 plain sentences, no markdown. Lead with the overall goal and current task, then the one next action. Skip root-cause narrative, fix internals, secondary to-dos, and em-dash tangents.";

/// 单条消息文本截断上限（字符数）
const TRUNCATE_PER_MESSAGE: usize = 500;

/// 对话总长度截断上限（字符数）
const TRUNCATE_TOTAL: usize = 20_000;

/// recap 生成结果判别联合（对应 CCB RecapResult）
#[derive(Debug, Clone, PartialEq)]
pub enum RecapResult {
    /// 成功生成摘要
    Ok { text: String },
    /// LLM 返回 API 错误
    ApiError { text: String },
    /// 无历史消息可回顾
    NoTurn,
    /// 用户取消
    Aborted,
    /// 生成失败（LLM 调用异常、空输出等）
    Failed,
}

/// 按字符数截断，超出时添加 "…" 后缀（CJK 安全：字符级操作）
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let end: String = s.chars().take(max).collect();
        format!("{}…", end)
    } else {
        s.to_string()
    }
}

/// 将消息内容压缩为纯文本（Image → `[image]`，Reasoning 保留，其他 block 转文本）
fn flatten_content(msg: &BaseMessage) -> String {
    let blocks = msg.message_content().content_blocks();
    let parts: Vec<String> = blocks
        .iter()
        .map(|b| match b {
            ContentBlock::Text { text } => text.to_string(),
            ContentBlock::Image { .. } => "[image]".to_string(),
            ContentBlock::Reasoning { text, .. } => text.clone(),
            ContentBlock::ToolUse { name, .. } => format!("调用 {}", name),
            ContentBlock::ToolResult { .. } => "[工具结果]".to_string(),
            _ => String::new(),
        })
        .filter(|s| !s.is_empty())
        .collect();
    truncate_chars(&parts.join("\n"), TRUNCATE_PER_MESSAGE)
}

/// 将历史消息压缩为 recap 输入格式
///
/// - 跳过 System（避免 system prompt 占用 input）
/// - 跳过 Tool（工具结果冗长，对 recap 价值低）
/// - Human → `[用户] ...`，Ai → `[助手] ...`
fn preprocess_messages(messages: &[BaseMessage]) -> String {
    let mut lines = Vec::new();
    for msg in messages {
        match msg {
            BaseMessage::System { .. } | BaseMessage::Tool { .. } => {}
            BaseMessage::Human { .. } => {
                lines.push(format!("[用户] {}", flatten_content(msg)));
            }
            BaseMessage::Ai { tool_calls, .. } => {
                let text = flatten_content(msg);
                let tool_names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                let line = if tool_names.is_empty() {
                    format!("[助手] {}", text)
                } else {
                    format!("[助手] {}（调用了工具: {}）", text, tool_names.join(", "))
                };
                lines.push(line);
            }
        }
    }
    truncate_chars(&lines.join("\n"), TRUNCATE_TOTAL)
}

/// 生成单句 recap
///
/// 与 CCB `generateRecap` 对齐：
/// - 禁用工具（[`LlmRequest`] 不带 tools）
/// - 单轮（一次 invoke，无 ReAct 循环）
/// - 跳过 history 写入（调用方不写 history）
///
/// 返回 [`RecapResult`] 判别联合。
pub async fn generate_recap(
    messages: &[BaseMessage],
    model: &dyn BaseModel,
    cancel_token: &AgentCancellationToken,
) -> RecapResult {
    // no-turn：无历史消息可回顾
    if messages.is_empty() {
        return RecapResult::NoTurn;
    }

    let conversation = preprocess_messages(messages);
    if conversation.trim().is_empty() {
        // 历史全是 System/Tool 消息，压缩后为空
        return RecapResult::NoTurn;
    }

    let user_content = format!(
        "以下是对话历史：\n<conversation>\n{}\n</conversation>\n\n{}",
        conversation, RECAP_PROMPT_ZH
    );

    let request =
        LlmRequest::new(vec![BaseMessage::human(user_content)]).with_system(SYSTEM_PROMPT);

    if cancel_token.is_cancelled() {
        tracing::info!("recap: 已被用户取消");
        return RecapResult::Aborted;
    }

    // 支持 Ctrl+C 取消（对齐 CCB 的 AbortSignal）
    let result = tokio::select! {
        biased;
        _ = cancel_token.cancelled() => {
            tracing::info!("recap: 已被用户取消");
            return RecapResult::Aborted;
        }
        r = model.invoke(request) => r,
    };

    match result {
        Ok(response) => {
            let text = response.message.message_content().text_content();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                warn!("recap: LLM 返回空文本");
                RecapResult::Failed
            } else {
                RecapResult::Ok {
                    text: trimmed.to_string(),
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "recap: LLM 调用失败");
            RecapResult::ApiError {
                text: e.to_string(),
            }
        }
    }
}

#[cfg(test)]
#[path = "recap_test.rs"]
mod tests;

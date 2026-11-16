//! 终端标题：底层 OSC 0 设置与状态机来自 cc-terminal，
//! LLM 会话主题提炼属于会话业务层，保留在 TUI。

use cc_agent::llm::types::{LlmRequest, StopReason};
use cc_agent::messages::BaseMessage;

pub use cc_terminal::terminal_title::*;

/// 标题提炼的 system 指令：只输出标题本身。
const TITLE_SYSTEM_INSTRUCTION: &str = "你是一个会话主题生成工具。请根据用户首轮输入，概括提炼出最精准、简明的话题短标题（3~6个中文汉字或3~5个英文单词）。\
严格要求：只输出标题本身，严禁任何标点符号、引号、冒号、换行或解释性文字。";

/// 标题提炼请求的输入字符上限，避免大文本输入拖慢标题生成。
const TITLE_INPUT_CHAR_LIMIT: usize = 500;

/// 标题字符上限。
const TITLE_MAX_CHARS: usize = 48;

/// 构造标题提炼请求。
///
/// **刻意不设 `max_tokens`**：它继承 provider 上限（32k）。早前硬编码 30 是个坑——
/// 推理模型的**思考与正文共享这份预算**，预算被思考吃光后 `content` 只剩思考残片
/// （`stop_reason = max_tokens`），残片会被当成标题渲染并落库。同一教训见
/// `cc-middlewares/src/hitl/jev/rules.rs` 的规则提炼。`max_tokens` 是上限而非预留，
/// 调大不额外花钱。
fn build_title_request(truncated_input: &str) -> LlmRequest {
    LlmRequest::new(vec![BaseMessage::human(format!(
        "用户任务输入：\n{truncated_input}"
    ))])
    .with_system(TITLE_SYSTEM_INSTRUCTION)
}

/// 从 LLM 响应中提取标题。
///
/// 命中输出上限（`MaxTokens`）的回复一律丢弃：此时 `content` 往往是思考残片
/// （如「我们需要回答用户。用户要求是…」）。宁可回退本地保底标题，也不把半截推理
/// 当标题展示。
fn extract_title_from_response(stop_reason: &StopReason, raw: &str) -> Option<String> {
    if *stop_reason == StopReason::MaxTokens {
        return None;
    }
    // 剥离大模型可能输出的包裹符号
    let cleaned = raw
        .trim()
        .trim_matches(['"', '\'', '`', '“', '”', '《', '》', '：', ':', '.', '。']);
    let sanitized = sanitize_title(cleaned);
    if sanitized.is_empty() {
        None
    } else {
        Some(truncate_terminal_title_part(&sanitized, TITLE_MAX_CHARS))
    }
}

/// 异步调用 LLM 从首轮 Prompt 中智能概括提炼 3~5 个词的精准会话主题（对齐 Codex: ThreadMetadataGenerationService）
pub async fn generate_thread_title_llm(
    provider: cc_acp::provider::LlmProvider,
    prompt: &str,
) -> Option<String> {
    let prompt_trimmed = prompt.trim();
    if prompt_trimmed.is_empty() {
        return None;
    }

    let truncated_input: String = prompt_trimmed
        .chars()
        .take(TITLE_INPUT_CHAR_LIMIT)
        .collect();

    let model = provider.into_model();
    let request = build_title_request(&truncated_input);

    // 8 秒硬超时，超时或失败自动返回 None（回退本地保底标题）
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(8), model.invoke(request)).await;

    match result {
        Ok(Ok(response)) => {
            let title =
                extract_title_from_response(&response.stop_reason, &response.message.content());
            if title.is_none() {
                tracing::debug!(
                    stop_reason = ?response.stop_reason,
                    "LLM thread title generation unusable, using fallback"
                );
            }
            title
        }
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "LLM thread title generation failed");
            None
        }
        Err(_) => {
            tracing::debug!("LLM thread title generation timed out, using fallback");
            None
        }
    }
}

#[cfg(test)]
#[path = "terminal_title_test.rs"]
mod tests;

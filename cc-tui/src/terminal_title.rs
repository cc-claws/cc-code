//! 终端标题：底层 OSC 0 设置与状态机来自 cc-terminal，
//! LLM 会话主题提炼属于会话业务层，保留在 TUI。

pub use cc_terminal::terminal_title::*;

/// 异步调用 LLM 从首轮 Prompt 中智能概括提炼 3~5 个词的精准会话主题（对齐 Codex: ThreadMetadataGenerationService）
pub async fn generate_thread_title_llm(
    provider: cc_acp::provider::LlmProvider,
    prompt: &str,
) -> Option<String> {
    let prompt_trimmed = prompt.trim();
    if prompt_trimmed.is_empty() {
        return None;
    }

    // 截取前 500 个字符送给大模型，避免大文本输入拖慢标题生成
    let truncated_input: String = prompt_trimmed.chars().take(500).collect();

    let model = provider.into_model();
    let system_instruction = "你是一个会话主题生成工具。请根据用户首轮输入，概括提炼出最精准、简明的话题短标题（3~6个中文汉字或3~5个英文单词）。\
严格要求：只输出标题本身，严禁任何标点符号、引号、冒号、换行或解释性文字。";

    let request =
        cc_agent::llm::types::LlmRequest::new(vec![cc_agent::messages::BaseMessage::human(
            format!("用户任务输入：\n{truncated_input}"),
        )])
        .with_system(system_instruction)
        .with_max_tokens(30);

    // 8 秒硬超时，超时或失败自动返回 None（回退本地保底标题）
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(8), model.invoke(request)).await;

    match result {
        Ok(Ok(response)) => {
            let raw = response.message.content();
            let mut cleaned = raw.trim();
            // 剥离大模型可能输出的包裹符号
            cleaned =
                cleaned.trim_matches(['"', '\'', '`', '“', '”', '《', '》', '：', ':', '.', '。']);
            let sanitized = sanitize_title(cleaned);
            if sanitized.is_empty() {
                None
            } else {
                Some(truncate_terminal_title_part(&sanitized, 48))
            }
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

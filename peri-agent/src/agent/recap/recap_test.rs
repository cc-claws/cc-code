//! recap 模块测试

use crate::{
    agent::AgentCancellationToken,
    error::{AgentError, AgentResult},
    llm::{
        types::{LlmRequest, LlmResponse, StopReason},
        BaseModel,
    },
    messages::BaseMessage,
};
use async_trait::async_trait;

use super::*;

// ── Mock BaseModel ─────────────────────────────────────────────────────────

/// Mock LLM — 按预设响应或错误返回
struct MockBaseModel {
    response: Option<String>,
    error: Option<String>,
}

/// 构造返回指定文本的 Mock 模型
fn make_mock_model_ok(text: &str) -> MockBaseModel {
    MockBaseModel {
        response: Some(text.to_string()),
        error: None,
    }
}

/// 构造返回空文本的 Mock 模型
fn make_mock_model_empty() -> MockBaseModel {
    MockBaseModel {
        response: Some(String::new()),
        error: None,
    }
}

/// 构造返回错误的 Mock 模型
fn make_mock_model_error(msg: &str) -> MockBaseModel {
    MockBaseModel {
        response: None,
        error: Some(msg.to_string()),
    }
}

#[async_trait]
impl BaseModel for MockBaseModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        if let Some(e) = &self.error {
            return Err(AgentError::Other(anyhow::anyhow!("{}", e)));
        }
        Ok(LlmResponse {
            message: BaseMessage::ai(self.response.clone().unwrap_or_default()),
            stop_reason: StopReason::EndTurn,
            usage: None,
            request_id: None,
        })
    }
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_id(&self) -> &str {
        "mock-model"
    }
}

// ── truncate_chars 测试 ────────────────────────────────────────────────────

#[test]
fn test_truncate_chars_short_string_unchanged() {
    // Arrange: 短字符串
    let s = "你好";

    // Act
    let result = truncate_chars(s, 10);

    // Assert: 未截断
    assert_eq!(result, "你好");
}

#[test]
fn test_truncate_chars_long_cjk_string_truncated() {
    // Arrange: 超长 CJK 字符串（CJK 必须字符级操作，字节切片会 panic）
    let s = "这是一个很长的中文字符串";

    // Act
    let result = truncate_chars(s, 5);

    // Assert: 截断到 5 字符 + 省略号
    assert_eq!(result, "这是一个很…", "应截断到 5 字符并以省略号闭合");
}

// ── preprocess_messages 测试 ───────────────────────────────────────────────

#[test]
fn test_preprocess_messages_skips_system_and_tool() {
    // Arrange: System + Tool 消息（均应跳过）
    let msgs = vec![
        BaseMessage::system("system prompt"),
        BaseMessage::human("用户消息"),
    ];

    // Act
    let result = preprocess_messages(&msgs);

    // Assert: 只保留 Human
    assert!(result.contains("[用户] 用户消息"), "应保留用户消息");
    assert!(!result.contains("system prompt"), "应跳过 System 消息");
}

#[test]
fn test_preprocess_messages_formats_human_and_ai() {
    // Arrange: Human + Ai 消息
    let msgs = vec![
        BaseMessage::human("你好"),
        BaseMessage::ai("你好！有什么可以帮你？"),
    ];

    // Act
    let result = preprocess_messages(&msgs);

    // Assert: 格式化为 [用户]/[助手] 前缀
    assert!(result.contains("[用户] 你好"), "用户消息应加 [用户] 前缀");
    assert!(
        result.contains("[助手] 你好！有什么可以帮你？"),
        "助手消息应加 [助手] 前缀"
    );
}

#[test]
fn test_preprocess_messages_empty_input() {
    // Arrange: 空消息列表
    let msgs: Vec<BaseMessage> = vec![];

    // Act
    let result = preprocess_messages(&msgs);

    // Assert: 空字符串
    assert!(result.is_empty(), "空历史应返回空字符串");
}

// ── generate_recap 测试 ────────────────────────────────────────────────────

#[tokio::test]
async fn test_generate_recap_empty_messages_returns_no_turn() {
    // Arrange: 空历史 + Mock 模型
    let model = make_mock_model_ok("不应被调用");
    let cancel = AgentCancellationToken::new();

    // Act
    let result = generate_recap(&[], &model, &cancel).await;

    // Assert: 返回 NoTurn
    assert_eq!(result, RecapResult::NoTurn, "空历史应返回 NoTurn");
}

#[tokio::test]
async fn test_generate_recap_success_returns_ok_with_trimmed_text() {
    // Arrange: 一条历史 + 返回带空白的文本
    let msgs = vec![BaseMessage::human("排查订单 IM20260910361758")];
    let model =
        make_mock_model_ok("  正在排查订单出库仓问题。下一步：确认应走官方仓还是自发仓。  ");
    let cancel = AgentCancellationToken::new();

    // Act
    let result = generate_recap(&msgs, &model, &cancel).await;

    // Assert: 返回 Ok + trim 后的文本
    match result {
        RecapResult::Ok { text } => {
            assert_eq!(
                text, "正在排查订单出库仓问题。下一步：确认应走官方仓还是自发仓。",
                "应 trim 前后空白"
            );
        }
        other => panic!("应返回 Ok，实际: {:?}", other),
    }
}

#[tokio::test]
async fn test_generate_recap_empty_response_returns_failed() {
    // Arrange: 一条历史 + 返回空文本
    let msgs = vec![BaseMessage::human("你好")];
    let model = make_mock_model_empty();
    let cancel = AgentCancellationToken::new();

    // Act
    let result = generate_recap(&msgs, &model, &cancel).await;

    // Assert: 空输出应返回 Failed
    assert_eq!(result, RecapResult::Failed, "LLM 空输出应返回 Failed");
}

#[tokio::test]
async fn test_generate_recap_llm_error_returns_api_error() {
    // Arrange: 一条历史 + Mock 模型返回错误
    let msgs = vec![BaseMessage::human("你好")];
    let model = make_mock_model_error("rate limit exceeded");
    let cancel = AgentCancellationToken::new();

    // Act
    let result = generate_recap(&msgs, &model, &cancel).await;

    // Assert: 返回 ApiError + 错误文本
    match result {
        RecapResult::ApiError { text } => {
            assert!(
                text.contains("rate limit exceeded"),
                "错误文本应包含原始错误"
            );
        }
        other => panic!("应返回 ApiError，实际: {:?}", other),
    }
}

#[tokio::test]
async fn test_generate_recap_cancelled_returns_aborted() {
    // Arrange: 历史 + 已取消的 cancel_token
    let msgs = vec![BaseMessage::human("你好")];
    let model = make_mock_model_ok("不应被使用");
    let cancel = AgentCancellationToken::new();
    cancel.cancel();

    // Act
    let result = generate_recap(&msgs, &model, &cancel).await;

    // Assert: 已取消应返回 Aborted
    assert_eq!(result, RecapResult::Aborted, "已取消应返回 Aborted");
}

#[tokio::test]
async fn test_generate_recap_only_system_messages_returns_no_turn() {
    // Arrange: 只有 System 消息（压缩后为空）
    let msgs = vec![BaseMessage::system("system prompt")];
    let model = make_mock_model_ok("不应被调用");
    let cancel = AgentCancellationToken::new();

    // Act
    let result = generate_recap(&msgs, &model, &cancel).await;

    // Assert: 全 System 历史应返回 NoTurn
    assert_eq!(
        result,
        RecapResult::NoTurn,
        "全 System 历史压缩后为空，应返回 NoTurn"
    );
}

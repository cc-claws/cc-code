use super::*;
use crate::{
    error::AgentError,
    llm::types::{LlmResponse, StopReason},
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

struct MockBaseModel {
    response: String,
    fail_with_ptl: usize,
    call_count: AtomicUsize,
}

impl MockBaseModel {
    fn new(response: &str) -> Self {
        Self {
            response: response.to_string(),
            fail_with_ptl: 0,
            call_count: AtomicUsize::new(0),
        }
    }
    fn new_with_ptl_fail(response: &str, ptl_fails: usize) -> Self {
        Self {
            response: response.to_string(),
            fail_with_ptl: ptl_fails,
            call_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl BaseModel for MockBaseModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        if count < self.fail_with_ptl {
            return Err(AgentError::LlmError(
                "prompt_too_long: input tokens exceed context window".to_string(),
            ));
        }
        Ok(LlmResponse {
            message: BaseMessage::ai(self.response.clone()),
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

// preprocess_messages tests

#[test]
fn test_preprocess_skips_system() {
    let msgs = vec![
        BaseMessage::system("old summary"),
        BaseMessage::human("hello"),
        BaseMessage::ai("hi"),
    ];
    let result = preprocess_messages(&msgs, 2000);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_preprocess_truncates_long_text() {
    let long_text = "x".repeat(3000);
    let msgs = vec![BaseMessage::human(long_text)];
    let result = preprocess_messages(&msgs, 2000);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("...(已截断)"));
}

#[test]
fn test_preprocess_replaces_image() {
    let msgs = vec![BaseMessage::human(MessageContent::blocks(vec![
        ContentBlock::text("see"),
        ContentBlock::image_base64("image/png", "data..."),
    ]))];
    let result = preprocess_messages(&msgs, 2000);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("[image]"));
    assert!(result[0].contains("see"));
}

#[test]
fn test_preprocess_formats_tool_calls() {
    use crate::messages::ToolCallRequest;
    use serde_json::json;
    let msgs = vec![BaseMessage::ai_with_tool_calls(
        MessageContent::text("thinking"),
        vec![
            ToolCallRequest::new("tc1", "Bash", json!({})),
            ToolCallRequest::new("tc2", "Read", json!({})),
        ],
    )];
    let result = preprocess_messages(&msgs, 2000);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("（调用了工具: Bash, Read）"));
}

#[test]
fn test_preprocess_formats_tool_result() {
    let msgs = vec![BaseMessage::tool_result("tc1", "output text")];
    let result = preprocess_messages(&msgs, 2000);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("[工具结果:tc1]"));
    assert!(result[0].contains("output text"));
}

#[test]
fn test_preprocess_empty_messages() {
    let result: Vec<String> = preprocess_messages(&[], 2000);
    assert!(result.is_empty());
}

// postprocess_summary tests

#[test]
fn test_postprocess_removes_analysis() {
    let input = "<analysis>detailed analysis here</analysis>\n\n## 摘要\ncontent";
    let result = postprocess_summary(input);
    assert!(!result.contains("<analysis>"));
    assert!(!result.contains("</analysis>"));
    assert!(result.contains("此会话从之前的对话延续"));
}

#[test]
fn test_postprocess_extracts_summary_tag() {
    let input = "<analysis>思考</analysis>\n<summary>\n## 核心摘要\n实际内容\n</summary>";
    let result = postprocess_summary(input);
    assert!(result.contains("## 核心摘要"));
    assert!(result.contains("实际内容"));
    assert!(!result.contains("<summary>"));
}

#[test]
fn test_postprocess_no_tags() {
    let input = "## 摘要\n这是直接输出的摘要文本";
    let result = postprocess_summary(input);
    assert!(result.contains("此会话从之前的对话延续"));
    assert!(result.contains("这是直接输出的摘要文本"));
}

#[test]
fn test_postprocess_cleans_blank_lines() {
    let input = "## 摘要\n\n\n\n内容\n\n\n\n结尾";
    let result = postprocess_summary(input);
    assert!(!result.contains("\n\n\n"));
}

#[test]
fn test_postprocess_multiple_analysis_blocks() {
    let input = "<analysis>块1</analysis>中间文本<analysis>块2</analysis>剩余";
    let result = postprocess_summary(input);
    assert!(result.contains("中间文本"));
    assert!(result.contains("剩余"));
    assert!(!result.contains("块1"));
    assert!(!result.contains("块2"));
}

// truncate_for_ptl tests

#[test]
fn test_ptl_truncate_single_round() {
    let msgs = vec![BaseMessage::human("q"), BaseMessage::ai("a")];
    let rounds = group_messages_by_round(&msgs);
    // Human and Ai are separate rounds, but only 1 round can be dropped at most
    let result = truncate_for_ptl(&msgs, &rounds, 1);
    // With 2 rounds, dropping 1 should leave 1 round
    assert!(result.len() < msgs.len());
}

#[test]
fn test_ptl_truncate_drops_oldest() {
    use crate::messages::ToolCallRequest;
    use serde_json::json;
    let msgs = vec![
        BaseMessage::human("q1"),
        BaseMessage::ai("a1"),
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("using"),
            vec![ToolCallRequest::new("tc1", "Bash", json!({}))],
        ),
        BaseMessage::tool_result("tc1", "output"),
        BaseMessage::human("q2"),
        BaseMessage::ai("a2"),
    ];
    let rounds = group_messages_by_round(&msgs);
    assert!(rounds.len() >= 4);
    let result = truncate_for_ptl(&msgs, &rounds, 1);
    assert!(result.len() < msgs.len());
    assert!(result[0].content().contains("a1") || result[0].content().contains("using"));
}

#[test]
fn test_ptl_truncate_drops_multiple() {
    let msgs: Vec<BaseMessage> = (0..5)
        .flat_map(|i| {
            vec![
                BaseMessage::human(format!("q{}", i)),
                BaseMessage::ai(format!("a{}", i)),
            ]
        })
        .collect();
    let rounds = group_messages_by_round(&msgs);
    assert_eq!(rounds.len(), 10);
    let result = truncate_for_ptl(&msgs, &rounds, 3);
    assert!(result.len() < msgs.len());
}

#[test]
fn test_ptl_truncate_preserves_at_least_one() {
    let msgs: Vec<BaseMessage> = (0..3)
        .flat_map(|i| {
            vec![
                BaseMessage::human(format!("q{}", i)),
                BaseMessage::ai(format!("a{}", i)),
            ]
        })
        .collect();
    let rounds = group_messages_by_round(&msgs);
    let result = truncate_for_ptl(&msgs, &rounds, 5);
    assert!(!result.is_empty());
}

#[test]
fn test_ptl_truncate_drop_count_zero() {
    let msgs = vec![BaseMessage::human("q"), BaseMessage::ai("a")];
    let rounds = group_messages_by_round(&msgs);
    let result = truncate_for_ptl(&msgs, &rounds, 0);
    assert_eq!(result.len(), msgs.len(), "drop_count=0 应返回原样消息");
}

// is_ptl_error tests

#[test]
fn test_is_ptl_error_variants() {
    for msg in &[
        "prompt_too_long",
        "context_length_exceeded",
        "max_context_window",
        "token limit exceeded",
        "too many tokens",
    ] {
        let err = AgentError::LlmError(msg.to_string());
        assert!(is_ptl_error(&err), "expected '{}' to be PTL error", msg);
    }
}

#[test]
fn test_is_not_ptl_error() {
    let err = AgentError::LlmError("connection timeout".to_string());
    assert!(!is_ptl_error(&err));
}

// full_compact integration tests

#[tokio::test]
async fn test_full_compact_basic() {
    use crate::messages::ToolCallRequest;
    use serde_json::json;
    let msgs = vec![
        BaseMessage::human("帮我写个函数"),
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("using bash"),
            vec![ToolCallRequest::new(
                "tc1",
                "Bash",
                json!({"command": "echo"}),
            )],
        ),
        BaseMessage::tool_result("tc1", "编译成功"),
    ];
    let model = MockBaseModel::new("## 摘要\n用户请求编写函数");
    let config = CompactConfig::default();
    let result = full_compact(&msgs, &model, &config, "").await.unwrap();
    assert!(result.summary.contains("此会话从之前的对话延续"));
    assert_eq!(result.messages_used, 3);
}

#[tokio::test]
async fn test_full_compact_empty_messages() {
    let model = MockBaseModel::new("summary");
    let config = CompactConfig::default();
    let result = full_compact(&[], &model, &config, "").await.unwrap();
    assert!(result.summary.contains("无有效对话历史"));
    assert_eq!(result.messages_used, 0);
}

#[tokio::test]
async fn test_full_compact_system_only() {
    let msgs = vec![BaseMessage::system("old summary")];
    let model = MockBaseModel::new("summary");
    let config = CompactConfig::default();
    let result = full_compact(&msgs, &model, &config, "").await.unwrap();
    assert!(result.summary.contains("无有效对话历史"));
    assert_eq!(result.messages_used, 1);
}

#[tokio::test]
async fn test_full_compact_with_instructions() {
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("hi")];
    let model = MockBaseModel::new("summary with instructions");
    let config = CompactConfig::default();
    let result = full_compact(&msgs, &model, &config, "请特别关注文件路径信息")
        .await
        .unwrap();
    assert!(result.summary.contains("此会话从之前的对话延续"));
}

#[tokio::test]
async fn test_full_compact_ptl_retry_succeeds() {
    let msgs: Vec<BaseMessage> = (0..5)
        .flat_map(|i| {
            vec![
                BaseMessage::human(format!("q{}", i)),
                BaseMessage::ai(format!("a{}", i)),
            ]
        })
        .collect();
    let model = MockBaseModel::new_with_ptl_fail("摘要", 2);
    let config = CompactConfig {
        ptl_max_retries: 3,
        ..Default::default()
    };
    let result = full_compact(&msgs, &model, &config, "").await.unwrap();
    assert!(result.summary.contains("摘要"));
    assert!(result.messages_used < msgs.len());
}

#[tokio::test]
async fn test_full_compact_ptl_retry_exhausted() {
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("hi")];
    let model = MockBaseModel::new_with_ptl_fail("摘要", 5);
    let config = CompactConfig {
        ptl_max_retries: 3,
        ..Default::default()
    };
    let result = full_compact(&msgs, &model, &config, "").await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("PTL"),
        "错误消息应提及 PTL，实际: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_full_compact_non_ptl_error() {
    let msgs = vec![BaseMessage::human("hello")];
    struct FailModel;
    #[async_trait]
    impl BaseModel for FailModel {
        async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
            Err(AgentError::LlmError("connection refused".to_string()))
        }
        fn provider_name(&self) -> &str {
            "fail"
        }
        fn model_id(&self) -> &str {
            "fail-model"
        }
    }
    let config = CompactConfig::default();
    let result = full_compact(&msgs, &FailModel, &config, "").await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("connection refused"));
}

#[tokio::test]
async fn test_full_compact_empty_summary_rejected() {
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("hi")];
    let model = MockBaseModel::new("");
    let config = CompactConfig::default();
    let result = full_compact(&msgs, &model, &config, "").await;
    assert!(result.is_err(), "空摘要应被拒绝");
    assert!(
        result.unwrap_err().to_string().contains("空摘要"),
        "错误消息应提及空摘要"
    );
}

#[tokio::test]
async fn test_full_compact_whitespace_only_summary_rejected() {
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("hi")];
    let model = MockBaseModel::new("   \n  \t  ");
    let config = CompactConfig::default();
    let result = full_compact(&msgs, &model, &config, "").await;
    assert!(result.is_err(), "纯空白摘要应被拒绝");
}

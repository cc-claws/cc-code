//! 提炼部分失败的状态与缓存回归，不调用真实模型。

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use cc_agent::{
    error::AgentResult,
    llm::types::{LlmRequest, LlmResponse, StopReason},
    messages::BaseMessage,
};

use super::*;

struct MockRuleStatusModel {
    responses: Vec<String>,
    calls: AtomicUsize,
}

#[async_trait]
impl BaseModel for MockRuleStatusModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        let index = self.calls.fetch_add(1, Ordering::Relaxed);
        let response = self.responses.get(index).expect("应只进行预期的提炼调用");
        Ok(LlmResponse {
            message: BaseMessage::ai(response.clone()),
            stop_reason: StopReason::EndTurn,
            usage: None,
            request_id: None,
        })
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-rule-status"
    }
}

fn make_model(responses: &[&str]) -> Arc<MockRuleStatusModel> {
    Arc::new(MockRuleStatusModel {
        responses: responses
            .iter()
            .map(|response| (*response).to_string())
            .collect(),
        calls: AtomicUsize::new(0),
    })
}

fn make_loader(
    source: &str,
    model: Arc<MockRuleStatusModel>,
    chunk_len: usize,
    max_chunks: usize,
) -> JevRuleLoader {
    JevRuleLoader::new(
        source.to_string(),
        model,
        Duration::from_secs(1),
        chunk_len,
        max_chunks,
        empty_slot(),
    )
}

fn make_two_chunks(label: &str) -> String {
    format!("{label}-{}\n\n{label}-{}", "A".repeat(100), "B".repeat(100))
}

fn make_recursive_source(label: &str) -> String {
    format!(
        "{label}-{}-{}{label}-{}-{}",
        uuid::Uuid::new_v4(),
        "R".repeat(600),
        uuid::Uuid::new_v4(),
        "S".repeat(600)
    )
}

const COMPLETE_RULE: &str = r#"{"rules":[{"text":"禁止发布","source":"project"}]}"#;
const INVALID_RULE: &str = "invalid-rule-json";

#[tokio::test]
async fn test_loader_partial_chunk_failure_preserves_rules_and_marks_incomplete() {
    let source = make_two_chunks(&format!("partial-{}", uuid::Uuid::new_v4()));
    let model = make_model(&[COMPLETE_RULE, INVALID_RULE]);
    let loader = make_loader(&source, model.clone(), 160, 4);
    loader.ensure_loaded().await;
    assert!(
        !loader.rules_unavailable(),
        "父门仍可使用已提炼出的部分规则"
    );
    assert!(loader.rules_incomplete(), "丢失分块必须向子门报告不完整");
    assert!(loader.slot().read().is_some());
    assert!(
        cached_rules(&source).is_none(),
        "部分规则不能作为完整来源缓存"
    );
    loader.ensure_loaded().await;
    assert_eq!(model.calls.load(Ordering::Relaxed), 2, "会话内不应重复提炼");
    assert!(loader.rules_incomplete(), "重复访问不能丢失失败事实");
}

#[tokio::test]
async fn test_loader_recursive_half_failure_marks_incomplete() {
    let source = make_recursive_source("recursive");
    let model = make_model(&[INVALID_RULE, COMPLETE_RULE, INVALID_RULE]);
    let loader = make_loader(&source, model.clone(), 2000, 4);
    loader.ensure_loaded().await;
    assert!(!loader.rules_unavailable());
    assert!(
        loader.rules_incomplete(),
        "递归拆分仅成功一半仍是不完整提炼"
    );
    assert!(cached_rules(&source).is_none());
    assert_eq!(model.calls.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn test_loader_recursive_retry_recovers_complete_rules() {
    let source = make_recursive_source("recovered");
    let model = make_model(&[INVALID_RULE, COMPLETE_RULE, COMPLETE_RULE]);
    let loader = make_loader(&source, model.clone(), 2000, 4);
    loader.ensure_loaded().await;
    assert!(!loader.rules_unavailable());
    assert!(
        !loader.rules_incomplete(),
        "重试覆盖全部来源后应恢复完整状态"
    );
    assert!(cached_rules(&source).is_some());
    assert_eq!(model.calls.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn test_loader_truncated_salvage_is_incomplete_and_not_cached() {
    let source = format!("salvage-{}", uuid::Uuid::new_v4());
    let response = r#"{"rules":[{"text":"禁止发布","source":"project"},{"text":"截断"#;
    let model = make_model(&[response, response]);
    let first = make_loader(&source, model.clone(), 1000, 4);
    first.ensure_loaded().await;
    assert!(!first.rules_unavailable());
    assert!(first.rules_incomplete());
    assert!(
        cached_rules(&source).is_none(),
        "分块层也不能缓存截断抢救结果"
    );
    let second = make_loader(&source, model.clone(), 1000, 4);
    second.ensure_loaded().await;
    assert!(
        second.rules_incomplete(),
        "新会话不能把抢救结果误当作完整缓存"
    );
    assert_eq!(model.calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn test_loader_chunk_limit_reports_incomplete_and_can_recover_next_session() {
    let source = make_two_chunks(&format!("limited-{}", uuid::Uuid::new_v4()));
    let model = make_model(&[COMPLETE_RULE, COMPLETE_RULE]);
    let limited = make_loader(&source, model.clone(), 160, 1);
    limited.ensure_loaded().await;
    assert!(!limited.rules_unavailable());
    assert!(limited.rules_incomplete(), "分块上限丢弃尾部来源必须可观测");
    assert!(cached_rules(&source).is_none());
    let complete = make_loader(&source, model.clone(), 160, 4);
    complete.ensure_loaded().await;
    assert!(
        !complete.rules_incomplete(),
        "提高上限后不能继续命中残缺来源缓存"
    );
    assert!(cached_rules(&source).is_some());
    assert_eq!(model.calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn test_loader_complete_cache_preserves_complete_status() {
    let source = format!("complete-{}", uuid::Uuid::new_v4());
    let model = make_model(&[COMPLETE_RULE]);
    let first = make_loader(&source, model.clone(), 1000, 4);
    first.ensure_loaded().await;
    let second = make_loader(&source, model.clone(), 1000, 4);
    second.ensure_loaded().await;
    assert!(!first.rules_incomplete());
    assert!(!second.rules_incomplete());
    assert!(second.slot().read().is_some());
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_loader_total_failure_keeps_existing_unavailable_status() {
    let source = format!("unavailable-{}", uuid::Uuid::new_v4());
    let model = make_model(&[INVALID_RULE]);
    let loader = make_loader(&source, model, 1000, 4);
    loader.ensure_loaded().await;
    assert!(loader.rules_unavailable(), "全部失败保持既有父门状态");
    assert!(!loader.rules_incomplete(), "部分结果与全部失败应分开报告");
    assert!(loader.slot().read().is_none());
}

struct MockPendingRulesModel {
    started: tokio::sync::Notify,
    calls: AtomicUsize,
}

#[async_trait]
impl BaseModel for MockPendingRulesModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.started.notify_one();
        std::future::pending().await
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-pending-rules"
    }
}

#[tokio::test]
async fn test_loader_aborted_extraction_stays_incomplete_without_retrying() {
    let model = Arc::new(MockPendingRulesModel {
        started: tokio::sync::Notify::new(),
        calls: AtomicUsize::new(0),
    });
    let loader = Arc::new(JevRuleLoader::new(
        format!("aborted-{}", uuid::Uuid::new_v4()),
        model.clone(),
        Duration::from_secs(5),
        1000,
        4,
        empty_slot(),
    ));
    let task_loader = loader.clone();
    let task = tokio::spawn(async move { task_loader.ensure_loaded().await });
    model.started.notified().await;
    task.abort();
    assert!(task.await.expect_err("任务应已取消").is_cancelled());
    loader.ensure_loaded().await;
    assert!(
        loader.rules_incomplete(),
        "取消不能把未完成提炼误判为完整结果"
    );
    assert!(!loader.rules_unavailable(), "父门的原失败语义保持兼容");
    assert!(loader.slot().read().is_none());
    assert_eq!(
        model.calls.load(Ordering::Relaxed),
        1,
        "取消后不应重复烧提炼请求"
    );
}

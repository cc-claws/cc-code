//! rules.rs 单元测试（用 Mock 模型，不打网络）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use cc_agent::{
    error::{AgentError, AgentResult},
    llm::types::{LlmRequest, LlmResponse, StopReason},
    messages::BaseMessage,
};

use super::*;

struct MockRulesModel {
    response: String,
    calls: AtomicUsize,
}

impl MockRulesModel {
    fn new(response: &str) -> Self {
        Self {
            response: response.to_string(),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl BaseModel for MockRulesModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        self.calls.fetch_add(1, Ordering::Relaxed);
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
        "mock-rules"
    }
}

#[test]
fn test_parse_rules_plain_json() {
    let rules = parse_rules(
        r#"{"rules":[{"text":"不要动 CI","source":"project"}],"protected_paths":[".github/workflows/**"]}"#,
    )
    .unwrap();
    assert_eq!(rules.rules.len(), 1);
    assert_eq!(rules.rules[0].text, "不要动 CI");
    assert_eq!(rules.rules[0].source, "project");
    assert_eq!(
        rules.protected_paths,
        vec![".github/workflows/**".to_string()]
    );
}

#[test]
fn test_policy_text_renders_numbered_list() {
    let rules = parse_rules(
        r#"{"rules":[{"text":"甲规则","source":"personal"},{"text":"乙规则","source":"global"}]}"#,
    )
    .unwrap();
    assert_eq!(rules.policy_text(), "1. 甲规则\n2. 乙规则");
    // shell 无关：空文本被跳过
    let only_blank = parse_rules(r#"{"rules":[{"text":"  ","source":"x"}]}"#).unwrap();
    assert!(only_blank.policy_text().is_empty());
    assert!(only_blank.is_empty());
}

#[test]
fn test_parse_rules_ignores_judging_artifacts() {
    // 提炼只产出语义产物；即使模型多吐了命令/工具黑名单，也不会有字段接住它们
    let rules = parse_rules(
        r#"{"rules":[{"text":"不要动 CI","source":"project"}],"disallowed_commands":["git push*"],"denied_tools":["Write"]}"#,
    )
    .unwrap();
    assert_eq!(rules.rules.len(), 1);
    assert!(rules.protected_paths.is_empty());
    assert!(!rules.is_empty());
}

#[test]
fn test_parse_rules_strips_surrounding_noise() {
    // 模型常加代码块和解释，取第一对花括号
    let text = "好的，以下是结果：\n```json\n{\"rules\":[{\"text\":\"规则\",\"source\":\"project\"}]}\n```\n希望有帮助。";
    let rules = parse_rules(text).unwrap();
    assert_eq!(rules.rules[0].text, "规则");
}

#[test]
fn test_parse_rules_invalid_returns_none() {
    assert!(parse_rules("no json here").is_none());
    assert!(parse_rules("{broken").is_none());
    // 空对象能解析，但规则集为空
    assert!(parse_rules("{}").unwrap().is_empty());
}

#[test]
fn test_jev_rules_is_empty() {
    assert!(JevRules::default().is_empty());
    assert!(JevRules {
        rules: vec![JevRule {
            text: "   ".to_string(),
            source: "project".to_string(),
        }],
        ..Default::default()
    }
    .is_empty());
    assert!(!JevRules {
        rules: vec![JevRule {
            text: "x".to_string(),
            source: "project".to_string(),
        }],
        ..Default::default()
    }
    .is_empty());
}

#[tokio::test]
async fn test_extract_rules_parses_and_is_not_empty() {
    let model = MockRulesModel::new(
        r#"{"rules":[{"text":"禁止改 Cargo.lock","source":"project"}],"protected_paths":["Cargo.lock"]}"#,
    );
    let rules = extract_rules(
        &model,
        "独特文本 A: 禁止改 Cargo.lock",
        Duration::from_secs(5),
    )
    .await
    .expect("应提炼出规则");
    assert_eq!(rules.rules[0].text, "禁止改 Cargo.lock");
    assert_eq!(rules.rules[0].source, "project");
    assert_eq!(rules.protected_paths, vec!["Cargo.lock".to_string()]);
    assert_eq!(model.calls(), 1);
}

#[tokio::test]
async fn test_extract_rules_cache_hit_skips_llm() {
    let source = "独特文本 B: 不要 push --force";
    let model =
        MockRulesModel::new(r#"{"rules":[{"text":"不要 push --force","source":"project"}]}"#);
    let first = extract_rules(&model, source, Duration::from_secs(5))
        .await
        .unwrap();
    let second = extract_rules(&model, source, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(model.calls(), 1, "同内容第二次应命中缓存");
    assert_eq!(first.rules, second.rules);
    // 缓存是进程级共享的，断言只针对本用例独有的 source，避免与其他测试互相干扰
    assert!(cached_rules(source).is_some());
}

#[tokio::test]
async fn test_extract_rules_empty_source_returns_none() {
    let model = MockRulesModel::new(r#"{"rules":[{"text":"x","source":"project"}]}"#);
    assert!(extract_rules(&model, "   \n ", Duration::from_secs(5))
        .await
        .is_none());
    assert_eq!(model.calls(), 0, "空输入不应调用 LLM");
}

#[tokio::test]
async fn test_extract_rules_empty_result_returns_none() {
    let model = MockRulesModel::new("{}");
    assert!(extract_rules(&model, "独特文本 C", Duration::from_secs(5))
        .await
        .is_none());
}

#[tokio::test]
async fn test_extract_rules_malformed_response_returns_none() {
    let model = MockRulesModel::new("完全不是 JSON");
    assert!(extract_rules(&model, "独特文本 D", Duration::from_secs(5))
        .await
        .is_none());
}

// ─── 分块提炼：规则来源超长时不得静默丢规则 ──────────────────────────────────

#[test]
fn test_split_chunks_keeps_source_header_with_body() {
    // `## … | source=xxx` 标题必须和它的正文待在同一块——模型靠标题判断规则来源
    let src = "## A | source=project\n规则甲\n\n## B | source=global\n规则乙";
    let chunks = split_chunks(src, 1000);
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].contains("source=project"));
    assert!(chunks[0].contains("source=global"));
}

#[test]
fn test_split_chunks_respects_limit() {
    let src = format!("{}\n\n{}", "甲".repeat(50), "乙".repeat(50));
    let chunks = split_chunks(&src, 60);
    assert!(chunks.len() >= 2, "应切成多块: {chunks:?}");
    for c in &chunks {
        assert!(c.chars().count() <= 60, "块超限: {}", c.chars().count());
    }
}

#[test]
fn test_split_chunks_handles_long_paragraph_by_lines() {
    // 整份 CLAUDE.md 没有空行的情况 → 退化为按行切
    let para: String = (0..40).map(|i| format!("行{i}\n")).collect();
    let chunks = split_chunks(&para, 40);
    assert!(chunks.len() > 1, "超长段落应按行切开: {chunks:?}");
    for c in &chunks {
        assert!(c.chars().count() <= 41, "块超限: {}", c.chars().count());
    }
}

#[test]
fn test_merge_rules_dedupes_across_chunks() {
    // 分块提炼会产生重复规则（同一规则出现在相邻块），合并时必须去重
    let parts = vec![
        JevRules {
            rules: vec![
                JevRule {
                    text: "禁止泄露 MAC".to_string(),
                    source: "project".to_string(),
                },
                JevRule {
                    text: "不要动 CI".to_string(),
                    source: "project".to_string(),
                },
            ],
            protected_paths: vec![".env".to_string()],
        },
        JevRules {
            rules: vec![
                JevRule {
                    text: "禁止泄露 MAC".to_string(),
                    source: "project".to_string(),
                },
                JevRule {
                    text: "个人规则".to_string(),
                    source: "personal".to_string(),
                },
            ],
            protected_paths: vec![".env".to_string(), "Cargo.lock".to_string()],
        },
    ];
    let merged = merge_rules(parts);
    assert_eq!(merged.rules.len(), 3, "重复规则应去重: {:?}", merged.rules);
    assert_eq!(merged.protected_paths, vec![".env", "Cargo.lock"]);
}

#[tokio::test]
async fn test_extract_rules_chunked_covers_long_source() {
    // 关键回归：来源超过单块上限时，**后面的规则也要被提炼到**（早前是静默截断丢掉）
    let mut model = MockRulesModel::new(r#"{"rules":[{"text":"块规则","source":"project"}]}"#);
    model.response = r#"{"rules":[{"text":"块规则","source":"project"}]}"#.to_string();

    let long = format!(
        "{}\n\n{}",
        "丙".repeat(300), // 第一块（避免与 max_chunks 用例共享全局提炼缓存）
        "丁".repeat(300)  // 第二块
    );
    let out = extract_rules_chunked(&model, &long, 400, 6, std::time::Duration::from_secs(5))
        .await
        .expect("应提炼出规则");
    assert!(!out.rules.is_empty());
    assert!(model.calls() >= 2, "应分多块调用, 实际 {}", model.calls());
}

#[tokio::test]
async fn test_extract_rules_chunked_respects_max_chunks() {
    let model = MockRulesModel::new(r#"{"rules":[{"text":"块规则","source":"project"}]}"#);
    let long = vec!["甲".repeat(300); 10].join("\n\n"); // 远超 max_chunks × chunk_len
    let _ = extract_rules_chunked(&model, &long, 400, 2, std::time::Duration::from_secs(5)).await;
    assert!(
        model.calls() <= 2,
        "不得超过 max_chunks, 实际 {}",
        model.calls()
    );
}

#[tokio::test]
async fn test_truncated_response_salvaged_but_not_cached() {
    // 回复被截断（撞输出上限）→ 抢救出完整规则；但**不得缓存**，
    // 否则这份残缺规则会在整个进程生命周期内定格、无法自愈。
    let truncated = r#"{"rules":[{"text":"规则一","source":"project"},{"text":"规则二","source":"project"},{"text":"被截断"#;
    let model = MockRulesModel::new(truncated);

    let first = extract_rules(&model, "截断测试源 ABC", Duration::from_secs(5)).await;
    let r = first.expect("应尽力抢救出部分规则");
    assert_eq!(r.rules.len(), 2, "应抢救出完整的那 2 条: {:?}", r.rules);

    let _ = extract_rules(&model, "截断测试源 ABC", Duration::from_secs(5)).await;
    assert_eq!(model.calls(), 2, "截断抢救的结果不应被缓存");
}

#[tokio::test]
async fn test_empty_content_returns_none() {
    let model = MockRulesModel::new("");
    assert!(
        extract_rules(&model, "空回复测试源", Duration::from_secs(5))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn test_extract_rules_timeout_returns_none() {
    struct SlowModel;
    #[async_trait]
    impl BaseModel for SlowModel {
        async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Err(AgentError::LlmError("never reached".into()))
        }
        fn provider_name(&self) -> &str {
            "slow"
        }
        fn model_id(&self) -> &str {
            "slow"
        }
    }

    let out = extract_rules(&SlowModel, "独特文本 E", Duration::from_millis(20)).await;
    assert!(out.is_none(), "超时应返回 None 而不是挂住");
}

// ─── 超时：只放宽时限重试一次，且绝不触发对半递归 ──────────────────────────────
//
// 回归自实测事故：网关单块要 18–73 秒，而超时被当成"块太大"去对半递归，
// 一块放大成 1+2+4=7 次等待（2 块共 14 次），每次白等一个完整时限。

/// 永远比时限慢的模型；记录被调用次数。
struct AlwaysSlowModel {
    delay: Duration,
    calls: AtomicUsize,
}

#[async_trait]
impl BaseModel for AlwaysSlowModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        tokio::time::sleep(self.delay).await;
        Err(AgentError::LlmError("unreachable".into()))
    }
    fn provider_name(&self) -> &str {
        "slow"
    }
    fn model_id(&self) -> &str {
        "always-slow"
    }
}

#[tokio::test]
async fn test_timeout_retries_once_with_widened_deadline_then_stops() {
    let model = AlwaysSlowModel {
        delay: Duration::from_millis(500),
        calls: AtomicUsize::new(0),
    };
    // 首试 20ms、重试 40ms，模型要 500ms → 两次都超时。
    let out = extract_rules(&model, "超时阶梯测试源", Duration::from_millis(20)).await;
    assert!(out.is_none());
    assert_eq!(
        model.calls.load(Ordering::Relaxed),
        2,
        "超时应恰好尝试 2 次（首试 + 放宽一次），不能无限重试"
    );
}

#[tokio::test]
async fn test_timeout_does_not_trigger_halving_recursion() {
    // 长来源会被切块；若超时误入对半递归，一块会变成 1+2+4=7 次调用。
    // 断言：整块来源（无论切几块）总调用数 == 2 × 块数，且远小于 7 × 块数。
    let long = format!("{}\n\n{}", "甲".repeat(3000), "乙".repeat(3000));
    let model = AlwaysSlowModel {
        delay: Duration::from_millis(500),
        calls: AtomicUsize::new(0),
    };
    let out = extract_rules_chunked(
        &model,
        &long,
        4000, // 不切块（整段 6002 字符 ≤ 4000? 否 → 切成 2 块）
        4,
        Duration::from_millis(20),
    )
    .await;
    assert!(out.is_none(), "全部超时不应产出规则");
    let calls = model.calls.load(Ordering::Relaxed);
    assert!(
        calls <= 4,
        "超时不得对半递归（应 ≤ 2 块 × 2 次 = 4），实际 {calls}"
    );
}

#[tokio::test]
async fn test_malformed_response_still_halves_recursively() {
    // 反向保证：**解析失败**（非超时）仍应保留对半递归的能力——
    // 这是为"思考吃光输出预算"设计的，不能因为删超时递归而一起删掉。
    let long = format!("{}-{}\n\n{}-{}", "丙".repeat(600), uuid::Uuid::new_v4(), "丁".repeat(600), uuid::Uuid::new_v4());
    let model = MockRulesModel::new("不是 JSON");
    let _ = extract_rules_chunked(&model, &long, 2000, 4, Duration::from_secs(5)).await;
    assert!(
        model.calls() > 2,
        "解析失败应对半递归（一块 → 多次调用），实际 {}",
        model.calls()
    );
}

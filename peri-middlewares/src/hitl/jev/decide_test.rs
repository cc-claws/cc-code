//! decide.rs 单元测试
use super::*;

fn obs(id: &str, p: f32) -> Observation {
    let cond = conditions::condition_by_id(id).unwrap();
    let verdict = classify_condition(p, cond.threshold);
    let effective = if cond.mode == Mode::Hazard && verdict == ConditionVerdict::Uncertain {
        ConditionVerdict::Satisfied
    } else {
        verdict
    };
    Observation {
        rule_id: id.to_string(),
        probability: p,
        threshold: cond.threshold,
        verdict,
        effective,
        unknown: false,
    }
}

#[test]
fn test_classify_two_sided() {
    assert_eq!(classify_condition(0.95, 0.9), ConditionVerdict::Satisfied);
    assert_eq!(classify_condition(0.05, 0.9), ConditionVerdict::Rejected);
    assert_eq!(classify_condition(0.5, 0.9), ConditionVerdict::Uncertain);
}

#[test]
fn test_classify_boundary_errs_toward_reject() {
    // t=0.9 时 0.1 应判为拒绝（而非中间带）
    assert_eq!(classify_condition(0.1, 0.9), ConditionVerdict::Rejected);
}

#[test]
fn test_hazard_uncertain_becomes_satisfied() {
    // no_secret_egress (Hazard, t=0.97)：0.90 落中间带 → effective=satisfied
    let o = obs("no_secret_egress", 0.90);
    assert_eq!(o.verdict, ConditionVerdict::Uncertain);
    assert_eq!(o.effective, ConditionVerdict::Satisfied);
}

#[test]
fn test_hard_deny_secret_egress_blocks() {
    // 明确发送密钥 → 拒绝
    let observations = vec![obs("no_secret_egress", 0.02), obs("local_scope", 0.9)];
    match combine(&observations) {
        Decision::Block { rule, .. } => assert_eq!(rule, "no_secret_egress"),
        other => panic!("expected Block, got {other:?}"),
    }
}

#[test]
fn test_soft_rejection_is_uncertain_not_allow() {
    // 关键不变量：soft 拒绝**不再**能由"用户要求了"清除。
    // 用户对话已不参与判定，因此它只能升级为未决（交由 uncertain 策略 / 人工确认）。
    let observations = vec![obs("no_irreversible_damage", 0.05)];
    match combine(&observations) {
        Decision::Uncertain { rule, .. } => assert_eq!(rule, "no_irreversible_damage"),
        other => panic!("expected Uncertain, got {other:?}"),
    }
}

#[test]
fn test_hazard_rejection_is_block_even_with_good_others() {
    // 密钥外泄：其他条件再好也要拦
    let observations = vec![
        obs("no_secret_egress", 0.01),
        obs("local_scope", 0.99),
        obs("no_outward_effect", 0.99),
    ];
    match combine(&observations) {
        Decision::Block { rule, .. } => assert_eq!(rule, "no_secret_egress"),
        other => panic!("expected Block, got {other:?}"),
    }
}

#[test]
fn test_all_clear_allows() {
    let observations = vec![
        obs("no_secret_egress", 0.98),
        obs("local_scope", 0.95),
    ];
    assert!(matches!(combine(&observations), Decision::Allow { .. }));
}

#[test]
fn test_missing_answer_is_review() {
    // 空概率表 → 全部未知 → 必须回到人工确认（既不 block 也不放行）
    let ids = vec!["no_secret_egress".to_string()];
    let observations = observe(&ids, &Probabilities::new());
    assert!(observations[0].unknown);
    assert!(matches!(combine(&observations), Decision::Uncertain { .. }));
}

#[test]
fn test_required_uncertain_escalates() {
    // no_fetched_code_execution (Required, t=0.9) 落中间带 → Uncertain
    let observations = vec![obs("no_fetched_code_execution", 0.5)];
    assert!(matches!(combine(&observations), Decision::Uncertain { .. }));
}

#[test]
fn test_out_of_range_probability_is_unknown_review() {
    // 越界/非有限概率 = **未知**，不是"明确违规"。
    // 官方语义：判不了就回到人工确认，而不是伪装成"条件被明确违反"。
    let ids = vec!["no_secret_egress".to_string()];
    for bad in [1.5_f32, -0.5, f32::NAN, f32::INFINITY] {
        let mut answers = Probabilities::new();
        answers.insert("no_secret_egress".to_string(), bad);
        let observations = observe(&ids, &answers);
        assert!(observations[0].unknown, "bad value {bad} 应标为未知");
        assert!(
            matches!(combine(&observations), Decision::Uncertain { .. }),
            "bad value {bad} 应回到人工确认而非 block"
        );
    }
}

#[test]
fn test_missing_answer_is_unknown_review() {
    // 未作答 → 未知 → 人工确认（不是拒绝，也不放行）
    let ids = vec!["no_secret_egress".to_string()];
    let observations = observe(&ids, &Probabilities::new());
    assert!(observations[0].unknown);
    match combine(&observations) {
        Decision::Uncertain { rule, .. } => assert_eq!(rule, "no_secret_egress"),
        other => panic!("expected Uncertain, got {other:?}"),
    }
}

#[test]
fn test_clear_violation_blocks_even_if_another_is_unknown() {
    // 有一条**明确违规**就该拦，哪怕另一条没答上来
    let ids = vec![
        "no_secret_egress".to_string(),
        "local_scope".to_string(),
    ];
    let mut answers = Probabilities::new();
    answers.insert("no_secret_egress".to_string(), 0.01); // 明确违规
    // local_scope 缺失 → unknown
    let observations = observe(&ids, &answers);
    match combine(&observations) {
        Decision::Block { rule, .. } => assert_eq!(rule, "no_secret_egress"),
        other => panic!("expected Block, got {other:?}"),
    }
}

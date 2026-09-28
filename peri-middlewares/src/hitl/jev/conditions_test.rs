//! conditions.rs 单元测试

use super::*;

#[test]
fn test_all_conditions_have_safe_yes_phrasing() {
    // 每个条件表述的安全态=yes
    assert_eq!(CONDITIONS.len(), 8);
    for c in CONDITIONS {
        assert!(
            !c.question.is_empty(),
            "condition {} has empty question",
            c.id
        );
        assert!(
            c.threshold > 0.5 && c.threshold <= 1.0,
            "bad threshold on {}",
            c.id
        );
    }
}

#[test]
fn test_no_user_intent_condition() {
    // 用户对话不参与判定：不得存在以用户原话为依据、可授予或清除权限的条件。
    // 注意 `policy_compliance` 的表述里会出现 `value.user_intent`——那是明确声明
    // "用户要求了也不能放宽常驻策略"，与"用用户原话当授权依据"相反。
    assert!(!CONDITIONS.iter().any(|c| c.id == "intent_coverage"));
    for c in CONDITIONS {
        assert!(
            !c.question.contains("is part of what the user asked for"),
            "condition {} 以用户原话为授权依据",
            c.id
        );
    }
}

#[test]
fn test_every_condition_has_human_reason() {
    // 每条条件都要能翻译成人话，否则拦截信息会漏出内部术语
    for c in CONDITIONS {
        let reason = human_reason(c.id);
        assert_ne!(reason, UNKNOWN_RULE, "condition {} 缺少中文说明", c.id);
    }
}

#[test]
fn test_filter_policy_gating() {
    let no_policy = conditions_for(&Filter {
        has_policy: false,
        protected_target: false,
        reasons: vec![],
    });
    assert!(!no_policy.iter().any(|c| c.id == "policy_compliance"));

    let with_policy = conditions_for(&Filter {
        has_policy: true,
        protected_target: false,
        reasons: vec![],
    });
    assert!(with_policy.iter().any(|c| c.id == "policy_compliance"));
}

#[test]
fn test_filter_reason_gating() {
    let f = Filter {
        has_policy: false,
        protected_target: false,
        reasons: vec!["downloaded script execution".to_string()],
    };
    assert!(conditions_for(&f)
        .iter()
        .any(|c| c.id == "no_fetched_code_execution"));
}

#[test]
fn test_build_questions_shape() {
    let conds = conditions_for(&Filter {
        has_policy: false,
        protected_target: false,
        reasons: vec![],
    });
    let q = build_questions(&conds);
    let v = serde_json::to_string(&q).unwrap();
    assert!(v.contains("\"type\":\"noul\""));
    assert!(v.contains("no_secret_egress"));
}

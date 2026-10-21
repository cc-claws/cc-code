use super::*;

fn release(tag: &str, prerelease: bool) -> Release {
    Release {
        tag_name: tag.to_string(),
        prerelease,
    }
}

#[test]
fn test_parse_version_accepts_npm_v_tags() {
    assert_eq!(parse_version("npm-v0.6.171"), Some((0, 6, 171)));
    assert_eq!(parse_version("npm-v1.0.0"), Some((1, 0, 0)));
    // 预发布 / 构建后缀只剥掉，不参与比较
    assert_eq!(parse_version("npm-v0.6.171-alpha.1"), Some((0, 6, 171)));
    assert_eq!(parse_version("npm-v0.6.171+build5"), Some((0, 6, 171)));
}

#[test]
fn test_parse_version_rejects_foreign_and_malformed_tags() {
    // 非 npm-v 前缀（agent-v / v 前缀 / 空）一律拒绝
    assert_eq!(parse_version("agent-v0.6.171"), None);
    assert_eq!(parse_version("v0.6.171"), None);
    assert_eq!(parse_version(""), None);
    // 段数不足或过多
    assert_eq!(parse_version("npm-v0.6"), None);
    assert_eq!(parse_version("npm-v0.6.1.2"), None);
    // 非数字段
    assert_eq!(parse_version("npm-vx.6.1"), None);
}

#[test]
fn test_compare_versions_supports_all_three_orders() {
    assert_eq!(
        compare_versions(parse_version("npm-v0.6.172"), "0.6.171"),
        Verdict::Newer
    );
    assert_eq!(
        compare_versions(parse_version("npm-v0.6.171"), "0.6.171"),
        Verdict::Same
    );
    assert_eq!(
        compare_versions(parse_version("npm-v0.6.170"), "0.6.171"),
        Verdict::Older
    );
    // 本地版本比线上新（本地自建）不能报「有更新」
    assert_eq!(
        compare_versions(parse_version("npm-v0.6.170"), "0.7.0"),
        Verdict::Older
    );
    // 无法解析的 tag 视为不更新
    assert_eq!(compare_versions(None, "0.6.171"), Verdict::Older);
}

#[test]
fn test_pick_latest_tag_prefers_highest_semver_not_list_order() {
    // GitHub 列表顺序不保证严格降序：0.6.9 排在 0.6.171 前面时必须仍取 171
    let releases = [
        release("npm-v0.6.9", false),
        release("npm-v0.6.171", false),
        release("npm-v0.6.17", false),
    ];
    assert_eq!(pick_latest_tag(&releases), Some("npm-v0.6.171".to_string()));
}

#[test]
fn test_pick_latest_tag_skips_prerelease_and_foreign_prefixes() {
    let releases = [
        release("npm-v0.7.0-beta.1", true),
        release("agent-v9.9.9", false),
        release("npm-v0.6.171", false),
    ];
    assert_eq!(pick_latest_tag(&releases), Some("npm-v0.6.171".to_string()));
}

#[test]
fn test_pick_latest_tag_returns_none_when_nothing_matches() {
    assert_eq!(pick_latest_tag(&[]), None);
    assert_eq!(
        pick_latest_tag(&[release("agent-v1.0.0", false)]),
        None,
        "没有 npm-v* tag 时不应挑出任何版本"
    );
}

#[test]
fn test_evaluate_reports_available_without_tag_prefix() {
    let releases = [release("npm-v0.6.172", false)];
    assert_eq!(
        evaluate(&releases, "0.6.171"),
        UpdateStatus::Available {
            latest: "0.6.172".to_string()
        },
        "提示里应只出现版本号，不带 npm-v 前缀"
    );
}

#[test]
fn test_evaluate_is_up_to_date_when_version_matches_or_ahead() {
    assert_eq!(
        evaluate(&[release("npm-v0.6.171", false)], "0.6.171"),
        UpdateStatus::UpToDate
    );
    // 本地比线上新：本地自建版本不该被劝降级
    assert_eq!(
        evaluate(&[release("npm-v0.6.171", false)], "0.7.0"),
        UpdateStatus::UpToDate
    );
}

#[test]
fn test_evaluate_is_up_to_date_on_empty_response() {
    // 空列表常见于限流后的异常响应；此时不该显示任何更新提示
    assert_eq!(evaluate(&[], "0.6.171"), UpdateStatus::UpToDate);
}

#[test]
fn test_checker_defaults_to_unknown_until_polled() {
    let checker = UpdateChecker::new();
    assert_eq!(*checker.status(), UpdateStatus::Unknown);
}

#[tokio::test]
async fn test_checker_poll_returns_false_while_request_in_flight() {
    let mut checker = UpdateChecker::new();
    // forbidden 的 receiver：poll 时 try_recv 会返回 Empty，模拟「还在查」
    let (_tx, rx) = tokio::sync::oneshot::channel();
    checker.rx = Some(rx);
    assert!(!checker.poll(), "在途时不应报告状态变化");
    assert_eq!(*checker.status(), UpdateStatus::Unknown);
    assert!(checker.rx.is_some(), "在途中的接收器必须保留");
}

#[tokio::test]
async fn test_checker_poll_consumes_result_and_clears_in_flight() {
    let mut checker = UpdateChecker::new();
    let (tx, rx) = tokio::sync::oneshot::channel();
    checker.rx = Some(rx);
    tx.send(UpdateStatus::Available {
        latest: "0.6.172".to_string(),
    })
    .expect("接收器仍存活");

    assert!(checker.poll(), "收到新结果应报告需要重绘");
    assert_eq!(
        *checker.status(),
        UpdateStatus::Available {
            latest: "0.6.172".to_string()
        }
    );
    assert!(checker.rx.is_none(), "取到结果后应退出在途状态");
    assert!(!checker.poll(), "没有在途任务时 poll 应返回 false");
}

#[tokio::test]
async fn test_checker_poll_treats_dropped_sender_as_unknown_and_releases_slot() {
    let mut checker = UpdateChecker::new();
    let (tx, rx) = tokio::sync::oneshot::channel::<UpdateStatus>();
    checker.rx = Some(rx);
    drop(tx);

    // 后台任务被取消 / panic 时 sender 会被 drop。此时必须释放槽位，
    // 否则「在途」标记会永久卡住。
    checker.poll();
    assert!(checker.rx.is_none(), "sender 被丢弃后应退出在途状态");
    assert_eq!(*checker.status(), UpdateStatus::Unknown);
}

#[tokio::test]
async fn test_checker_request_is_skipped_when_disabled() {
    let mut checker = UpdateChecker::new();
    checker.request_if_due(false);
    assert!(!checker.started, "禁用时不应标记已发起");
    assert!(checker.rx.is_none(), "禁用时不应产生在途任务");
}

#[tokio::test]
async fn test_checker_never_repeats_the_network_request() {
    let mut checker = UpdateChecker::new();
    checker.request_if_due(false);
    checker.started = true; // 模拟已经查过一次
    checker.request_if_due(true);
    assert!(
        checker.rx.is_none(),
        "同一进程内只应发起一次检查，不能因为第几次调用而重复 spawn"
    );
}

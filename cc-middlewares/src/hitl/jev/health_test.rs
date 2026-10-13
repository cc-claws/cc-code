use std::sync::Arc;

use super::*;

/// 初始状态不应处于熔断。
#[test]
fn test_health_not_open_initially() {
    let health = JevHealth::new();
    assert!(!health.is_open());
}

/// 首次打开返回 true（调用方据此告警），同一窗口内重复失败返回 false（不刷屏）。
#[test]
fn test_health_trip_reports_first_time_only() {
    let health = JevHealth::new();
    assert!(health.trip(), "首次打开应报告 true");
    assert!(health.is_open(), "打开后应处于熔断中");
    assert!(!health.trip(), "同一窗口内重复失败不应重复报告");
}

/// 复位后回到未熔断，且可再次报告（服务恢复后又挂掉应能重新告警）。
#[test]
fn test_health_reset_allows_new_report() {
    let health = JevHealth::new();
    assert!(health.trip());
    health.reset();
    assert!(!health.is_open(), "复位后不应仍处于熔断");
    assert!(health.trip(), "复位后再次失败应重新报告");
}

/// 注册表按 endpoint 分桶：同端点共享同一状态，不同端点互不影响。
#[test]
fn test_health_for_is_bucketed_by_endpoint() {
    let first = health_for("https://endpoint-a.example/v1/systemone");
    let same = health_for("https://endpoint-a.example/v1/systemone");
    let other = health_for("https://endpoint-b.example/v1/systemone");

    assert!(Arc::ptr_eq(&first, &same), "同 endpoint 应共享同一状态");
    assert!(!Arc::ptr_eq(&first, &other), "不同 endpoint 应各自独立");

    first.trip();
    assert!(first.is_open());
    assert!(!other.is_open(), "A 端点熔断不应波及 B 端点");
}

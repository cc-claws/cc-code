//! Sleep 工具的单测。

use crate::tools::sleep::{SleepTool, MAX_SLEEP_SECONDS};
use cc_agent::tools::BaseTool;
use serde_json::json;
use std::time::Instant;

#[test]
fn exposes_expected_schema() {
    let tool = SleepTool::new();
    assert_eq!(tool.name(), "Sleep");
    let params = tool.parameters();
    assert_eq!(params["required"], json!(["duration_seconds"]));
    assert_eq!(
        params["properties"]["duration_seconds"]["maximum"],
        json!(MAX_SLEEP_SECONDS)
    );
}

#[tokio::test]
async fn rejects_missing_duration() {
    let error = SleepTool::new()
        .invoke(json!({}))
        .await
        .expect_err("缺少参数应报错");
    assert!(error.to_string().contains("duration_seconds"), "{error}");
}

#[tokio::test]
async fn rejects_non_positive_duration() {
    for bad in [json!(0), json!(-1.0)] {
        let error = SleepTool::new()
            .invoke(json!({ "duration_seconds": bad }))
            .await
            .expect_err("非正数应报错");
        assert!(
            error.to_string().contains("positive"),
            "输入 {bad} 实际: {error}"
        );
    }
}

#[tokio::test]
async fn rejects_duration_above_cap() {
    let error = SleepTool::new()
        .invoke(json!({ "duration_seconds": MAX_SLEEP_SECONDS + 1.0 }))
        .await
        .expect_err("超过上限应报错");
    assert!(error.to_string().contains("must not exceed"), "{error}");
    // 报错要指出正确出路，而不是让模型反复重试。
    assert!(error.to_string().contains("run_in_background"), "{error}");
}

#[tokio::test]
async fn sleeps_for_requested_duration() {
    let started = Instant::now();
    let output = SleepTool::new()
        .invoke(json!({ "duration_seconds": 0.05 }))
        .await
        .expect("短期睡眠应成功");
    assert!(
        started.elapsed().as_millis() >= 40,
        "应至少等待约 50ms，实际 {:?}",
        started.elapsed()
    );
    assert!(output.contains("<sleep-result>"), "{output}");
    assert!(
        output.contains("<interrupted>false</interrupted>"),
        "{output}"
    );
}

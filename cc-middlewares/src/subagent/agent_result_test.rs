use super::*;

#[test]
fn test_agent_result_summary_preserves_complete_call_prohibition() {
    let tool = AgentResultTool::new();
    let first_line = tool.description().lines().next().expect("工具描述不为空");
    let summary: String = first_line.chars().take(160).collect();
    assert_eq!(summary, first_line, "首行必须能完整放进工具摘要");
    assert!(
        summary.starts_with("Do not call"),
        "禁止调用必须放在描述开头"
    );
    assert!(
        summary.contains("directly or through ExecuteExtraTool."),
        "直接与间接调用禁令均完整保留"
    );
    assert!(
        summary.contains("automatic background completion"),
        "明确说明结果自动回流"
    );
}

#[test]
fn test_agent_result_description_matches_notification_only_contract() {
    let tool = AgentResultTool::new();
    let description = tool.description();
    let parameters = tool.parameters();
    assert_eq!(tool.name(), "AgentResult", "保留合成消息使用的工具名");
    assert!(
        description.contains("cannot query or poll"),
        "不能承诺真实查询"
    );
    assert!(description.contains("Bash tasks"), "明确不支持 shell 查询");
    assert!(
        !description.contains("Returns the output"),
        "不能承诺读取输出"
    );
    assert_eq!(parameters["properties"]["task_id"]["type"], "string");
    assert!(
        parameters["properties"]["task_id"]["description"]
            .as_str()
            .is_some_and(|text| text.contains("direct invocation is unsupported")),
        "兼容参数必须明确标注不可查询"
    );
}

#[tokio::test]
async fn test_agent_result_invoke_never_reports_fabricated_task_status() {
    let tool = AgentResultTool::new();
    for input in [
        json!({}),
        json!({"task_id": "unknown-task"}),
        json!({"task_id": ""}),
    ] {
        let error = tool
            .invoke(input)
            .await
            .expect_err("占位工具必须明确拒绝查询");
        assert_eq!(
            error.to_string(),
            AGENT_RESULT_UNSUPPORTED,
            "task_id 不改变结果"
        );
        let io_error = error
            .downcast_ref::<std::io::Error>()
            .expect("明确的 unsupported 错误");
        assert_eq!(io_error.kind(), std::io::ErrorKind::Unsupported);
        assert!(error
            .to_string()
            .contains("No task status or output was checked"));
        assert!(
            !error.to_string().contains("No completed"),
            "不伪造未完成状态"
        );
    }
}

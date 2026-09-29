use super::*;

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

use super::*;
use cc_agent::agent::events::BackgroundTaskResult;

#[test]
fn test_background_results_preserve_history_marker_pairs() {
    let results: Vec<_> = [true, false]
        .into_iter()
        .enumerate()
        .map(|(index, success)| BackgroundTaskResult {
            task_id: format!("background-task-{index}"),
            agent_name: "reviewer".into(),
            prompt_summary: "检查后台任务".into(),
            success,
            output: format!("后台结果 {index}"),
            tool_calls_count: 1,
            duration_ms: 100,
            child_thread_id: None,
        })
        .collect();
    let (history, user_content) = inject_bg_result_messages(
        vec![BaseMessage::human("已有对话")],
        MessageContent::text("继续处理"),
        &results,
    );
    assert_eq!(history.len(), 4, "保留历史并追加一组调用与两个结果");
    assert_eq!(history[0].content(), "已有对话");
    assert_eq!(user_content.text_content(), "继续处理");
    let calls = history[1].tool_calls();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].id, calls[1].id, "不同结果须使用不同调用 ID");
    for (index, result) in results.iter().enumerate() {
        assert_eq!(calls[index].name, "AgentResult", "保留历史协议工具名");
        assert_eq!(calls[index].arguments["task_id"], result.task_id);
        let BaseMessage::Tool {
            tool_call_id,
            content,
            is_error,
            ..
        } = &history[index + 2]
        else {
            panic!("每个后台结果都必须写入工具结果消息");
        };
        assert_eq!(tool_call_id, &calls[index].id, "工具调用与结果须配对");
        assert_eq!(content.text_content(), result.to_notification());
        assert_eq!(*is_error, !result.success, "保留后台任务成功或失败状态");
    }
}

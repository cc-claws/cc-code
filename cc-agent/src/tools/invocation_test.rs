use super::*;

#[tokio::test]
async fn test_invocation_concurrent_calls_are_isolated() {
    let first = ToolInvocationContext {
        tool_call_id: "same-call".into(),
        source_agent_id: Some("first-agent".into()),
    };
    let second = ToolInvocationContext {
        tool_call_id: "same-call".into(),
        source_agent_id: Some("second-agent".into()),
    };
    let read = || async {
        tokio::task::yield_now().await;
        ToolInvocationContext::current()
    };
    let (a, b) = tokio::join!(first.clone().scope(read()), second.clone().scope(read()));
    assert_eq!(a, Some(first), "并发相同调用 ID 不能串到其他 Agent");
    assert_eq!(b, Some(second));
    assert_eq!(
        ToolInvocationContext::current(),
        None,
        "scope 结束必须恢复上下文"
    );
}

#[tokio::test]
async fn test_invocation_nested_scope_restores_parent() {
    let parent = ToolInvocationContext {
        tool_call_id: "parent".into(),
        source_agent_id: None,
    };
    let child = ToolInvocationContext {
        tool_call_id: "child".into(),
        source_agent_id: Some("child-agent".into()),
    };
    parent
        .clone()
        .scope(async {
            child
                .clone()
                .scope(async {
                    assert_eq!(ToolInvocationContext::current(), Some(child));
                })
                .await;
            assert_eq!(ToolInvocationContext::current(), Some(parent));
        })
        .await;
}

//! 调度器提供的调用身份，不从模型参数中读取，也不改变工具 schema。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocationContext {
    pub tool_call_id: String,
    pub source_agent_id: Option<String>,
}

tokio::task_local! {
    static TOOL_INVOCATION: ToolInvocationContext;
}

impl ToolInvocationContext {
    pub fn current() -> Option<Self> {
        TOOL_INVOCATION.try_with(Clone::clone).ok()
    }

    pub async fn scope<F: std::future::Future>(self, future: F) -> F::Output {
        TOOL_INVOCATION.scope(self, future).await
    }
}

#[cfg(test)]
#[path = "invocation_test.rs"]
mod tests;

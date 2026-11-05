//! 调度器提供的调用身份，不从模型参数中读取，也不改变工具 schema。

use crate::kernel::Kernel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocationContext {
    pub tool_call_id: String,
    pub source_agent_id: Option<String>,
}

tokio::task_local! {
    static TOOL_INVOCATION: ToolInvocationContext;
    /// 当前会话内核：与 invocation 同作用域，工具生成提示语时读取。
    static KERNEL_CONTEXT: Kernel;
}

impl ToolInvocationContext {
    pub fn current() -> Option<Self> {
        TOOL_INVOCATION.try_with(Clone::clone).ok()
    }

    /// 当前会话内核；不在工具调用作用域内时回退 cc。
    pub fn kernel() -> Kernel {
        KERNEL_CONTEXT.try_with(|k| *k).unwrap_or_default()
    }

    pub async fn scope<F: std::future::Future>(self, future: F) -> F::Output {
        TOOL_INVOCATION.scope(self, future).await
    }

    /// 带内核作用域：调度器用此方法包裹工具调用，让工具能生成方言提示语。
    pub async fn scope_with_kernel<F: std::future::Future>(
        self,
        kernel: Kernel,
        future: F,
    ) -> F::Output {
        KERNEL_CONTEXT
            .scope(kernel, TOOL_INVOCATION.scope(self, future))
            .await
    }
}

#[cfg(test)]
#[path = "invocation_test.rs"]
mod tests;
